// Development-only tokenizer equivalence harness. No model required.
use goodtype_math_recognize::{Result, inference::Tokenizer};
use std::io::{self, BufRead};
fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or("tokenizer.json path required")?;
    let tokenizer = Tokenizer::from_json(&serde_json::from_slice(&std::fs::read(path)?)?)?;
    for line in io::stdin().lock().lines() {
        let ids: Vec<u64> = serde_json::from_str(&line?)?;
        println!("{}", serde_json::to_string(&tokenizer.decode(&ids)?)?);
    }
    Ok(())
}
