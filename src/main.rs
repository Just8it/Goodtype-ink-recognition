use goodtype_math_recognize::{
    Result,
    inference::{DECODER_HASH, ENCODER_HASH, Recognizer},
    protocol,
};
use serde_json::json;
use std::{
    io::{self, Write},
    time::Instant,
};
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version", "--json"] {
        println!(
            "{}",
            json!({"name":"goodtype-math-recognize","version":"0.1.0","protocolVersion":1,"model":{"id":"texteller-int8","baseRevision":"7b96df06b9d81cdb129c3bef68b7250bc3e2b0ea","encoderSha256":ENCODER_HASH,"decoderSha256":DECODER_HASH}})
        );
        return Ok(());
    }
    if args != ["--check"] && args != ["--serve"] {
        return Err("usage: goodtype-math-recognize --version --json | --check | --serve".into());
    }
    let started = Instant::now();
    let dir = std::env::current_exe()?
        .parent()
        .ok_or("missing executable directory")?
        .to_owned();
    let mut model = Recognizer::load(&dir)?;
    let load = started.elapsed().as_secs_f64() * 1000.;
    if args == ["--check"] {
        let r = serde_json::from_str(
            r#"{"id":0,"strokes":[{"widthPt":1,"points":[[0,0],[10,10]]},{"widthPt":1,"points":[[0,10],[10,0]]}]}"#,
        )?;
        let started = Instant::now();
        let latex = model.strokes(&r)?;
        println!(
            "{}",
            json!({"ok":true,"loadMs":load,"recognizeMs":started.elapsed().as_secs_f64()*1000.,"latex":latex})
        );
    } else {
        println!("{}", json!({"ready":true,"loadMs":load}));
        io::stdout().flush()?;
        protocol::serve(io::stdin().lock(), io::stdout().lock(), |r| {
            model.strokes(r)
        })?;
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        println!("{}", json!({"ok":false,"error":e.to_string()}));
        std::process::exit(1);
    }
}
