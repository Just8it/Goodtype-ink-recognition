use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
#[derive(Deserialize, Debug)]
pub struct Stroke {
    #[serde(rename = "widthPt")]
    pub width: f64,
    pub points: Vec<[f64; 2]>,
}
#[derive(Deserialize, Debug)]
pub struct Request {
    pub id: u64,
    pub strokes: Vec<Stroke>,
}
pub fn validate(r: &Request) -> Result<(), &'static str> {
    if r.strokes.is_empty() || r.strokes.len() > 2000 {
        return Err("expected 1-2000 strokes");
    }
    let mut total = 0;
    for s in &r.strokes {
        if !s.width.is_finite() || s.width <= 0. || s.width > 100. {
            return Err("invalid widthPt");
        }
        total += s.points.len();
        if total > 200000 {
            return Err("too many points");
        }
        if s.points
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 100000.)
        {
            return Err("invalid coordinate");
        }
    }
    if total == 0 {
        return Err("no nonempty strokes");
    }
    Ok(())
}
pub const MAX_LINE: usize = 4 * 1024 * 1024;
pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    mut recognize: impl FnMut(&Request) -> crate::Result<String>,
) -> crate::Result<()> {
    loop {
        let mut line = Vec::new();
        let mut too_long = false;
        loop {
            let b = input.fill_buf()?;
            if b.is_empty() {
                break;
            }
            let n = b
                .iter()
                .position(|&v| v == b'\n')
                .map_or(b.len(), |p| p + 1);
            if line.len() + n > MAX_LINE {
                too_long = true;
            }
            if !too_long {
                line.extend_from_slice(&b[..n]);
            }
            let end = b[n - 1] == b'\n';
            input.consume(n);
            if end {
                break;
            }
        }
        if line.is_empty() && !too_long {
            break;
        }
        let id = serde_json::from_slice::<Value>(&line)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_u64))
            .unwrap_or(0);
        let result = (|| -> crate::Result<String> {
            if too_long {
                return Err("request exceeds 4 MiB".into());
            }
            let request: Request = serde_json::from_slice(&line)?;
            validate(&request)?;
            recognize(&request)
        })();
        let response = match result {
            Ok(latex) => json!({"id":id,"latex":latex}),
            Err(e) => json!({"id":id,"error":e.to_string()}),
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing() {
        let data = b"{\"id\":7,\"strokes\":[{\"widthPt\":1,\"points\":[[0,0]]}]}\n{\"id\":8,\"strokes\":[]}\nno\n";
        let mut out = vec![];
        serve(&data[..], &mut out, |_| Ok("x".into())).unwrap();
        let rows: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], json!({"id":7,"latex":"x"}));
        assert_eq!(rows[1]["id"], 8);
        assert!(rows[1].get("error").is_some());
        assert_eq!(rows[2]["id"], 0);
        let mut out = vec![];
        serve(&b""[..], &mut out, |_| unreachable!()).unwrap();
        assert!(out.is_empty());
    }
    #[test]
    fn limits() {
        let mut r = Request {
            id: 0,
            strokes: vec![Stroke {
                width: 1.,
                points: vec![[0., 0.]],
            }],
        };
        assert!(validate(&r).is_ok());
        r.strokes[0].width = 0.;
        assert!(validate(&r).is_err());
        r.strokes[0].width = 1.;
        r.strokes[0].points[0][0] = f64::NAN;
        assert!(validate(&r).is_err());
        r.strokes[0].points = vec![[0., 0.]; 200001];
        assert!(validate(&r).is_err());
        r.strokes[0].points = vec![[100000., -100000.]];
        assert!(validate(&r).is_ok());
        r.strokes[0].points[0][0] = 100001.;
        assert!(validate(&r).is_err());
        r.strokes[0].points.clear();
        assert!(validate(&r).is_err());
        r.strokes[0].points.push([0., 0.]);
        r.strokes[0].width = 100.;
        assert!(validate(&r).is_ok());
        r.strokes[0].width = 100.1;
        assert!(validate(&r).is_err());
        r.strokes[0].width = f64::INFINITY;
        assert!(validate(&r).is_err());
        r.strokes = (0..2001)
            .map(|_| Stroke {
                width: 1.,
                points: vec![[0., 0.]],
            })
            .collect();
        assert!(validate(&r).is_err());
        let mut out = vec![];
        let mut input = vec![b' '; MAX_LINE + 1];
        input.extend_from_slice(b"\n{}\n");
        serve(&input[..], &mut out, |_| unreachable!()).unwrap();
        assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), 2);
    }
}
