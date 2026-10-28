use crate::{
    Result,
    pixels::{Gray, preprocess},
    protocol::Request,
};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::{DynValue, Tensor},
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
pub const ENCODER_HASH: &str = "dfb48cc64f63eba1ae79d3ad5dfd690484b503b0eb9bdb5651d3cf404df303a7";
pub const DECODER_HASH: &str = "fdca7c0db645a16a2711bd2c6aebc3ad1ae00fdf9b45f4fe1d8f77b31c9c16c8";
pub struct Tokenizer {
    vocab: HashMap<u64, String>,
    special: HashSet<u64>,
    bytes: HashMap<char, u8>,
}
impl Tokenizer {
    pub fn from_json(j: &Value) -> Result<Self> {
        if j["decoder"]["type"] != "ByteLevel" {
            return Err("expected ByteLevel decoder".into());
        }
        let mut vocab = HashMap::new();
        for (s, id) in j["model"]["vocab"]
            .as_object()
            .ok_or("missing vocabulary")?
        {
            vocab.insert(id.as_u64().ok_or("invalid token id")?, s.clone());
        }
        let mut special = HashSet::new();
        for t in j["added_tokens"].as_array().ok_or("missing added tokens")? {
            let id = t["id"].as_u64().ok_or("invalid added id")?;
            vocab.insert(id, t["content"].as_str().ok_or("invalid token")?.into());
            if t["special"] == true {
                special.insert(id);
            }
        }
        let mut bytes = HashMap::new();
        let mut n = 256;
        for b in 0..=255u32 {
            let c = if (33..=126).contains(&b)
                || (161..=172).contains(&b)
                || (174..=255).contains(&b)
            {
                b
            } else {
                let c = n;
                n += 1;
                c
            };
            bytes.insert(char::from_u32(c).unwrap(), b as u8);
        }
        Ok(Self {
            vocab,
            special,
            bytes,
        })
    }
    pub fn decode(&self, ids: &[u64]) -> Result<String> {
        let mut bytes = vec![];
        for id in ids {
            if self.special.contains(id) {
                continue;
            }
            for c in self.vocab.get(id).ok_or("unknown token id")?.chars() {
                if let Some(b) = self.bytes.get(&c) {
                    bytes.push(*b);
                } else {
                    let mut buf = [0; 4];
                    bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
        let mut s = String::from_utf8_lossy(&bytes).into_owned();
        for (a, b) in [
            (" .", "."),
            (" ?", "?"),
            (" !", "!"),
            (" ,", ","),
            (" ' ", "'"),
            (" n't", "n't"),
            (" 'm", "'m"),
            (" 's", "'s"),
            (" 've", "'ve"),
            (" 're", "'re"),
        ] {
            s = s.replace(a, b);
        }
        Ok(s)
    }
}
pub fn postprocess(mut s: String) -> String {
    for cmd in [r"\bm", r"\boldsymbol", r"\textit", r"\textbf", r"\mathbf"] {
        let needle = format!("{cmd}{{");
        while let Some(i) = s.rfind(&needle) {
            let start = i + needle.len();
            let mut depth = 1;
            let mut escaped = false;
            let mut end = None;
            for (offset, c) in s[start..].char_indices() {
                if c == '\\' && !escaped {
                    escaped = true;
                    continue;
                }
                if !escaped {
                    if c == '{' {
                        depth += 1;
                    }
                    if c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(start + offset);
                            break;
                        }
                    }
                }
                escaped = false;
            }
            if let Some(end) = end {
                let inner = s[start..end].to_owned();
                s.replace_range(i..=end, &format!(" {inner} "));
            } else {
                s.replace_range(i..start, " ");
            }
        }
    }
    let s = s.trim();
    s.strip_prefix(r"\[")
        .and_then(|s| s.strip_suffix(r"\]"))
        .unwrap_or(s)
        .trim()
        .into()
}
pub struct Recognizer {
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
}
impl Recognizer {
    pub fn load(exe_dir: &Path) -> Result<Self> {
        ort::init_from(exe_dir.join("onnxruntime.dll"))?
            .with_name("goodtype-math")
            .commit();
        let model = exe_dir.join("models");
        let session = |name: &str| -> Result<Session> {
            Ok(Session::builder()?
                .with_execution_providers([ort::ep::CPU::default().build()])?
                .with_intra_threads(4)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .commit_from_file(model.join(name))?)
        };
        Ok(Self {
            encoder: session("encoder_model.onnx")?,
            decoder: session("decoder_model_merged.onnx")?,
            tokenizer: Tokenizer::from_json(&serde_json::from_slice(&std::fs::read(
                model.join("tokenizer.json"),
            )?)?)?,
        })
    }
    pub fn strokes(&mut self, r: &Request) -> Result<String> {
        self.image(&crate::pixels::rasterize(r)?)
    }
    pub fn image(&mut self, g: &Gray) -> Result<String> {
        self.tensor(preprocess(g)?)
    }
    pub fn tensor(&mut self, pixels: Vec<f32>) -> Result<String> {
        let mut enc = self.encoder.run(ort::inputs![Tensor::from_array((
            [1, 1, 448, 448],
            pixels
        ))?])?;
        let hidden = enc
            .remove("last_hidden_state")
            .ok_or("missing encoder output")?;
        let mut cache: HashMap<String, DynValue> = HashMap::new();
        for layer in 0..12 {
            for side in ["decoder", "encoder"] {
                for kind in ["key", "value"] {
                    cache.insert(
                        format!("past_key_values.{layer}.{side}.{kind}"),
                        Tensor::from_array(([1, 16, 1, 64], vec![0f32; 1024]))?.into_dyn(),
                    );
                }
            }
        }
        // The reference passes a fresh GenerationConfig: BOS 0 wins over the
        // nested decoder_start_token_id=2. EOS=2, PAD=1.
        let mut ids = vec![];
        let mut token = 0i64;
        for step in 0..1024 {
            let mut inputs: Vec<(String, ort::session::SessionInputValue<'_>)> = vec![
                (
                    "input_ids".into(),
                    Tensor::from_array(([1, 1], vec![token]))?.into(),
                ),
                ("encoder_hidden_states".into(), hidden.view().into()),
                (
                    "use_cache_branch".into(),
                    Tensor::from_array(([1], vec![step > 0]))?.into(),
                ),
            ];
            for (name, value) in &cache {
                inputs.push((name.clone(), value.view().into()));
            }
            let mut out = self.decoder.run(inputs)?;
            let (_, logits) = out["logits"].try_extract_tensor::<f32>()?;
            let mut best = 0;
            for i in 1..logits.len() {
                if logits[i] > logits[best] {
                    best = i;
                }
            }
            token = best as i64;
            if token == 2 {
                break;
            }
            ids.push(token as u64);
            for layer in 0..12 {
                for side in ["decoder", "encoder"] {
                    if step > 0 && side == "encoder" {
                        continue;
                    }
                    for kind in ["key", "value"] {
                        let name = format!("present.{layer}.{side}.{kind}");
                        cache.insert(
                            name.replace("present", "past_key_values"),
                            out.remove(&name).ok_or("missing decoder cache")?,
                        );
                    }
                }
            }
        }
        Ok(postprocess(self.tokenizer.decode(&ids)?))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_decode() {
        let j=serde_json::from_str(r#"{"decoder":{"type":"ByteLevel"},"model":{"vocab":{"<s>":0,"x":92,"\u0120":225,"\u00c3":132,"\u00a9":107}},"added_tokens":[{"id":0,"content":"<s>","special":true}]}"#).unwrap();
        let t = Tokenizer::from_json(&j).unwrap();
        assert_eq!(t.decode(&[0, 92, 225, 132, 107]).unwrap(), "x \u{e9}");
    }
    #[test]
    fn styles() {
        assert_eq!(
            postprocess(r"\[\mathbf{x_{\bm{y}}}\]".into())
                .split_whitespace()
                .collect::<String>(),
            "x_{y}"
        );
    }
}
