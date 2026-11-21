use goodtype_math_recognize::{
    Result,
    inference::{DECODER_HASH, ENCODER_HASH, Recognizer},
    protocol, session,
};
use serde_json::json;
use std::{
    io::{self, BufReader, Write},
    time::Instant,
};
fn run(args: &[String]) -> Result<()> {
    if args == ["--version", "--json"] {
        let version = option_env!("GOODTYPE_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
        println!(
            "{}",
            json!({"name":"goodtype-math-recognize","version":version,"protocolVersion":1,"model":{"id":"texteller-int8","baseRevision":"7b96df06b9d81cdb129c3bef68b7250bc3e2b0ea","encoderSha256":ENCODER_HASH,"decoderSha256":DECODER_HASH}})
        );
        return Ok(());
    }
    if args != ["--check"] && args != ["--serve"] && args != ["--session"] {
        return Err(
            "usage: goodtype-math-recognize --version --json | --check | --serve | --session"
                .into(),
        );
    }
    let started = Instant::now();
    let dir = std::env::current_exe()?
        .parent()
        .ok_or("missing executable directory")?
        .to_owned();
    if args == ["--session"] {
        let mut model = Recognizer::load_for_session(&dir)?;
        return session::run(BufReader::new(io::stdin()), io::stdout().lock(), move |r| {
            model.strokes(r)
        });
    }
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--session"] {
        std::panic::set_hook(Box::new(|_| eprintln!("Math addon internal error")));
    }
    if let Err(e) = run(&args) {
        if args == ["--session"] {
            eprintln!("Math addon session failed");
        } else {
            println!("{}", json!({"ok":false,"error":e.to_string()}));
        }
        std::process::exit(1);
    }
}
