// Development only: executable directory still owns all model/runtime paths.
use goodtype_math_recognize::{
    Result,
    inference::Recognizer,
    pixels::{Gray, preprocess},
};
use std::{
    io::{self, BufRead, Write},
    time::Instant,
};
fn main() -> Result<()> {
    let dir = std::env::current_exe()?.parent().unwrap().to_owned();
    let started = Instant::now();
    let mut model = Recognizer::load(&dir)?;
    println!(
        "{}",
        serde_json::json!({"ready":true,"loadMs":started.elapsed().as_secs_f64()*1000.})
    );
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let path = line?;
        let started = Instant::now();
        let image = image::open(&path)?.to_luma8();
        let g = Gray {
            width: image.width() as usize,
            height: image.height() as usize,
            data: image.into_raw(),
        };
        let tensor = preprocess(&g)?;
        // Optional developer tensor dump for numerical preprocessing comparisons.
        if let Some(path) = std::env::var_os("GOODTYPE_DEV_TENSOR") {
            std::fs::write(
                path,
                tensor
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            )?;
        }
        let latex = model.tensor(tensor)?;
        println!(
            "{}",
            serde_json::json!({"latex":latex,"seconds":started.elapsed().as_secs_f64()})
        );
        io::stdout().flush()?;
    }
    Ok(())
}
