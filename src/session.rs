//! Goodtype addon session v0. The host owns the deadline and process lifetime.
use crate::{
    Result,
    latex::latex_to_typst,
    protocol::{self, Request, Stroke},
};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashSet,
    fmt,
    io::{BufRead, Read, Write},
    sync::mpsc::sync_channel,
    thread,
};

const API: &str = "goodtype-api 0.1";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BATCH: &str = "page-batch-v0";
const MAX_LATEX: usize = 16 * 1024;

struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Json;
        impl<'de> Visitor<'de> for Json {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut object = Map::new();
                while let Some((key, Strict(value))) = map.next_entry()? {
                    if object.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(Strict(Value::Object(object)))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut list = Vec::new();
                while let Some(Strict(value)) = seq.next_element()? {
                    list.push(value);
                }
                Ok(Strict(Value::Array(list)))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
        }
        d.deserialize_any(Json)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    jsonrpc: String,
    #[serde(default, deserialize_with = "request_id")]
    id: Option<String>,
    method: String,
    params: Value,
}

fn request_id<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

fn read_frame(input: &mut impl BufRead) -> Result<Option<Frame>> {
    let mut line = Vec::new();
    if input
        .take((protocol::MAX_LINE + 2) as u64)
        .read_until(b'\n', &mut line)?
        == 0
    {
        return Ok(None);
    }
    let payload = line
        .strip_suffix(b"\n")
        .ok_or("unterminated or oversized frame")?;
    let payload = payload.strip_suffix(b"\r").unwrap_or(payload);
    if payload.len() > protocol::MAX_LINE {
        return Err("oversized frame".into());
    }
    let Strict(value) = serde_json::from_slice(payload)?;
    let frame: Frame = parse_object(value)?;
    if frame.jsonrpc != "2.0" || frame.id.as_deref().is_some_and(|id| !valid_id(id)) {
        return Err("invalid RPC envelope".into());
    }
    Ok(Some(frame))
}

fn valid_id(id: &str) -> bool {
    id.bytes().all(|b| b.is_ascii_digit()) && id.bytes().any(|b| b != b'0')
}

fn parse_object<T: de::DeserializeOwned>(value: Value) -> serde_json::Result<T> {
    serde_json::from_value(Value::Object(serde_json::from_value(value)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Initialize {
    api_version: String,
    capabilities: Vec<String>,
    addon: Map<String, Value>,
}

fn initialize(params: Value) -> Option<Vec<&'static str>> {
    let params: Initialize = parse_object(params).ok()?;
    let unique: HashSet<_> = params.capabilities.iter().map(String::as_str).collect();
    if params.api_version != API
        || params.addon != *json!({"id":"math","version":VERSION}).as_object()?
        || unique.len() != params.capabilities.len()
        || !unique.contains(BATCH)
    {
        return None;
    }
    // Progress is optional. The host can cancel even while ONNX is running.
    Some(
        [BATCH, "cancel"]
            .into_iter()
            .filter(|cap| unique.contains(cap))
            .collect(),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invoke {
    command: String,
    scope: Map<String, Value>,
    settings: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    kind: String,
    page: Map<String, Value>,
    strokes: Vec<Map<String, Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Page {
    width_pt: f64,
    height_pt: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SelectedStroke {
    id: String,
    width_pt: f64,
    points: Vec<[f64; 2]>,
}

fn invocation(params: Value) -> Result<(Request, Page)> {
    let invoke: Invoke = parse_object(params)?;
    let scope: Scope = serde_json::from_value(Value::Object(invoke.scope))?;
    let page: Page = serde_json::from_value(Value::Object(scope.page))?;
    if invoke.command != "recognize-math"
        || !invoke.settings.is_empty()
        || scope.kind != "ink-selection"
        || !page.width_pt.is_finite()
        || !page.height_pt.is_finite()
        || page.width_pt <= 0.
        || page.height_pt <= 0.
        || !(1..=2000).contains(&scope.strokes.len())
    {
        return Err("invalid math scope".into());
    }
    let mut aliases = HashSet::new();
    let mut strokes = Vec::with_capacity(scope.strokes.len());
    for stroke in scope.strokes {
        let stroke: SelectedStroke = serde_json::from_value(Value::Object(stroke))?;
        if stroke.id.is_empty() || !aliases.insert(stroke.id) {
            return Err("invalid stroke alias".into());
        }
        strokes.push(Stroke {
            width: stroke.width_pt,
            points: stroke.points,
        });
    }
    let request = Request { id: 0, strokes };
    protocol::validate(&request)?;
    Ok((request, page))
}

fn error(code: i32, message: &str) -> Value {
    json!({"error":{"code":code,"message":message}})
}

fn recognize(
    request: &Request,
    page: Page,
    model: &mut impl FnMut(&Request) -> Result<String>,
) -> Value {
    let Ok(latex) = model(request) else {
        return error(-32001, "Math recognition failed");
    };
    match latex_to_typst(&latex) {
        Ok(content) if !content.trim().is_empty() && content.trim().len() <= 64 * 1024 - 4 => {
            let mut points = request.strokes.iter().flat_map(|stroke| &stroke.points);
            let &[mut left, mut top] = points.next().expect("validated nonempty ink");
            let (mut right, mut bottom) = (left, top);
            for &[x, y] in points {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
            // Match Goodtype's transcriptionPlacement on centerline bounds.
            let x = left.max(12.).min((page.width_pt - 12. - 120.).max(12.));
            let width = (right - left).min(page.width_pt - 12. - x).max(120.);
            let below = bottom + 6.;
            let y = if below < page.height_pt - 24. {
                below
            } else {
                (top - 30.).max(12.)
            };
            json!({"result":{"batch":{"operations":[{
                "op":"addTypstBlock","source":format!("$ {} $", content.trim()),
                "xPt":x,"yPt":y,"layoutWidthPt":width
            }]}}})
        }
        _ => {
            let bounded = &latex[..latex.floor_char_boundary(MAX_LATEX.min(latex.len()))];
            json!({"error":{"code":-32003,"message":"Unsupported math output","data":{"latex":bounded}}})
        }
    }
}

fn reply(output: &mut impl Write, id: &str, mut body: Value) -> Result<()> {
    body["jsonrpc"] = json!("2.0");
    body["id"] = json!(id);
    let frame = serde_json::to_vec(&body)?;
    if frame.len() > protocol::MAX_LINE {
        return Err("reply exceeds frame limit".into());
    }
    output.write_all(&frame)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

enum Event {
    Frame(Frame),
    Eof,
    InvalidFrame,
    Done(Value),
    InternalError,
}

pub fn run(
    mut input: impl BufRead + Send + 'static,
    mut output: impl Write,
    mut model: impl FnMut(&Request) -> Result<String> + Send + 'static,
) -> Result<()> {
    let (events, inbox) = sync_channel(1);
    let reader_events = events.clone();
    thread::spawn(move || {
        loop {
            let event = match read_frame(&mut input) {
                Ok(Some(frame)) => Event::Frame(frame),
                Ok(None) => Event::Eof,
                Err(_) => Event::InvalidFrame,
            };
            let last = !matches!(event, Event::Frame(_));
            if reader_events.send(event).is_err() || last {
                break;
            }
        }
    });

    let (jobs, work) = sync_channel::<(Request, Page)>(0);
    // The main thread remains responsive even if an inference call is stuck. On
    // cancel it flushes the terminal reply and main exits, ending the worker too.
    thread::spawn(move || {
        while let Ok((request, page)) = work.recv() {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                recognize(&request, page, &mut model)
            }));
            let event = match outcome {
                Ok(body) => Event::Done(body),
                Err(_) => {
                    let _ = events.send(Event::InternalError);
                    break;
                }
            };
            if events.send(event).is_err() {
                break;
            }
        }
    });

    let mut capabilities = None;
    let mut outstanding: Option<String> = None;
    let mut eof = false;
    let mut completed = None;
    let mut seen = HashSet::new();
    let mut id_bytes = 0;
    while let Ok(event) = inbox.recv() {
        let frame = match event {
            Event::Frame(frame) => frame,
            Event::InvalidFrame => return Err("invalid frame".into()),
            Event::Eof => {
                if outstanding.is_none() {
                    return Ok(());
                }
                eof = true;
                continue;
            }
            Event::Done(body) => {
                let id = outstanding.take().ok_or("unexpected result")?;
                reply(&mut output, &id, body)?;
                completed = Some(id);
                if eof {
                    return Ok(());
                }
                continue;
            }
            Event::InternalError => {
                reply(
                    &mut output,
                    &outstanding.take().ok_or("unexpected error")?,
                    error(-32603, "Internal error"),
                )?;
                return Err("inference worker failed".into());
            }
        };

        let Some(id) = frame.id else {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Cancel {
                id: String,
            }
            let params: Cancel = parse_object(frame.params)?;
            if frame.method != "command/cancel"
                || !capabilities
                    .as_ref()
                    .is_some_and(|caps: &Vec<_>| caps.contains(&"cancel"))
                || outstanding.as_ref().or(completed.as_ref()) != Some(&params.id)
            {
                return Err("unexpected notification".into());
            }
            if let Some(id) = outstanding.take() {
                if id != params.id {
                    return Err("cancel ID mismatch".into());
                }
                reply(&mut output, &id, error(-32002, "Cancelled"))?;
            }
            // The completion can win the race. The host still discards it.
            return Ok(());
        };
        completed = None;
        id_bytes += id.len();
        if outstanding.is_some()
            || seen.len() == 65_536
            || id_bytes > protocol::MAX_LINE
            || !seen.insert(id.clone())
        {
            return Err("invalid request sequence or session ID budget exhausted".into());
        }
        if capabilities.is_none() && frame.method != "initialize" {
            return Err("request before initialization".into());
        }
        if frame.method == "initialize" {
            if capabilities.is_some() {
                return Err("repeated initialization".into());
            }
            let Some(accepted) = initialize(frame.params) else {
                reply(
                    &mut output,
                    &id,
                    error(-32602, "Incompatible initialization"),
                )?;
                return Err("incompatible initialization".into());
            };
            reply(
                &mut output,
                &id,
                json!({"result":{"apiVersion":API,"capabilities":accepted,"addon":{"id":"math","version":VERSION}}}),
            )?;
            capabilities = Some(accepted);
        } else if frame.method == "command/invoke" {
            match invocation(frame.params) {
                Ok(job) => {
                    outstanding = Some(id);
                    jobs.send(job)?;
                }
                Err(_) => reply(&mut output, &id, error(-32602, "Invalid math invocation"))?,
            }
        } else {
            reply(&mut output, &id, error(-32601, "Unknown method"))?;
        }
    }
    Err("session workers stopped".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn handshake() -> Value {
        json!({"apiVersion":API,"capabilities":["progress","cancel",BATCH,"future"],
            "addon":{"id":"math","version":VERSION}})
    }

    fn invoke() -> Value {
        json!({"command":"recognize-math","scope":{"kind":"ink-selection",
            "page":{"widthPt":595.28,"heightPt":841.89},
            "strokes":[{"id":"s1","widthPt":2,"points":[[100,200],[110,205]]}]},"settings":{}})
    }

    fn request(id: &str, method: &str, params: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
    }

    fn fixture(
        frames: &[Value],
        model: impl FnMut(&Request) -> Result<String> + Send + 'static,
    ) -> (bool, Vec<Value>) {
        let input: String = frames.iter().map(|frame| format!("{frame}\n")).collect();
        let mut output = Vec::new();
        let ok = run(Cursor::new(input), &mut output, model).is_ok();
        let rows = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (ok, rows)
    }

    #[test]
    fn handshake_and_placement() {
        let mut params = invoke();
        params["scope"]["page"]["heightPt"] = json!(220);
        let (ok, rows) = fixture(
            &[
                request("7", "initialize", handshake()),
                request("4", "command/invoke", params),
            ],
            |_| Ok("x^2".into()),
        );
        assert!(ok);
        assert_eq!(
            rows[0],
            json!({"jsonrpc":"2.0","id":"7","result":{
            "apiVersion":API,"capabilities":[BATCH,"cancel"],"addon":{"id":"math","version":VERSION}}})
        );
        assert_eq!(
            rows[1],
            json!({"jsonrpc":"2.0","id":"4","result":{"batch":{"operations":[
                {"op":"addTypstBlock","source":"$ x^2 $","xPt":100.0,"yPt":170.0,"layoutWidthPt":120.0}
            ]}}})
        );
    }

    #[test]
    fn mismatch_and_invalid_sequence() {
        for (key, value) in [
            ("apiVersion", json!("goodtype-api 0.2")),
            ("addon", json!({"id":"math","version":"99.0.0"})),
            ("capabilities", json!(["cancel"])),
            ("capabilities", json!([BATCH, BATCH])),
        ] {
            let mut params = handshake();
            params[key] = value;
            let (ok, rows) = fixture(&[request("1", "initialize", params)], |_| unreachable!());
            assert!(!ok);
            assert_eq!(rows[0]["error"]["code"], -32602);
        }
        let (ok, rows) = fixture(
            &[
                request("1", "initialize", handshake()),
                request("1", "unknown", json!({})),
            ],
            |_| unreachable!(),
        );
        assert!(!ok);
        assert_eq!(rows.len(), 1);
        assert!(
            !fixture(
                &[request("1", "command/invoke", invoke())],
                |_| unreachable!()
            )
            .0
        );
    }

    #[test]
    fn strict_framing() {
        for input in [
            b"{}".to_vec(),
            vec![b' '; protocol::MAX_LINE + 4],
            b"{\"id\":\"1\",\"id\":\"2\"}\n".to_vec(),
            b"{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"initialize\",\"params\":{\"a\":1,\"\\u0061\":2}}\n".to_vec(),
            b"{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"initialize\",\"params\":{},\"extra\":true}\n".to_vec(),
            b"{\"jsonrpc\":\"1.0\",\"id\":\"1\",\"method\":\"initialize\",\"params\":{}}\n".to_vec(),
            b"{\"jsonrpc\":\"2.0\",\"method\":\"\xff\",\"params\":{}}\n".to_vec(),
        ] {
            assert!(read_frame(&mut Cursor::new(input)).is_err());
        }
        let mut exact = request("1", "initialize", handshake())
            .to_string()
            .into_bytes();
        exact.resize(protocol::MAX_LINE, b' ');
        exact.extend_from_slice(b"\r\n");
        assert!(read_frame(&mut Cursor::new(&exact)).unwrap().is_some());
        exact.insert(0, b' ');
        assert!(read_frame(&mut Cursor::new(&exact)).is_err());
    }

    #[test]
    fn oversized_reply_writes_no_partial_frame() {
        let mut output = Vec::new();
        let id = "1".repeat(protocol::MAX_LINE);
        assert!(reply(&mut output, &id, error(-32602, "Invalid request")).is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn command_errors_and_unsupported_output() {
        let mut invalid = invoke();
        invalid["scope"]["strokes"][0]["extra"] = json!(true);
        let (ok, rows) = fixture(
            &[
                request("1", "initialize", handshake()),
                request("2", "absent", json!({})),
                request("3", "command/invoke", invalid),
                request("4", "command/invoke", invoke()),
            ],
            |_| Ok(format!("\\unknown{}", "\u{03B1}".repeat(MAX_LATEX))),
        );
        assert!(ok);
        for (row, code) in rows[1..].iter().zip([-32601, -32602, -32003]) {
            assert_eq!(row["error"]["code"], code);
        }
        let latex = rows[3]["error"]["data"]["latex"].as_str().unwrap();
        assert!(latex.starts_with("\\unknown"));
        assert!(latex.len() <= MAX_LATEX);
    }

    #[test]
    fn cancellation_is_terminal_while_inference_is_running() {
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let (ok, rows) = fixture(
            &[
                request("1", "initialize", handshake()),
                request("2", "command/invoke", invoke()),
                json!({"jsonrpc":"2.0","method":"command/cancel","params":{"id":"2"}}),
            ],
            move |_| {
                let _ = wait.recv();
                Ok("late".into())
            },
        );
        drop(release);
        assert!(ok);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["id"], "2");
        assert_eq!(rows[1]["error"]["code"], -32002);
    }
}
