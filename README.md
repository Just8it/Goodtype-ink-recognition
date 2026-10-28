# Goodtype Math Recognizer

Experimental optional, local-only handwritten math to LaTeX helper for Goodtype.
Rust owns this independent child process; Goodtype never links ONNX Runtime and
performs its own LaTeX to Typst conversion. Apache-2.0. No account, telemetry,
network code, subprocess execution, or file writes in the shipped helper.
Recognition is a proposal: users must review errors before replacing ink.

## Use

Keep the extracted folder together. Windows x64 CPU; no Python at runtime.

```powershell
.\goodtype-math-recognize.exe --version --json
.\goodtype-math-recognize.exe --check
.\goodtype-math-recognize.exe --serve
```

See [INTEGRATION.md](INTEGRATION.md) for the fixed protocol v1. Models load only
from `models/` beside the executable, and ONNX Runtime only from its own directory.
`--check` recognizes a synthetic cross, rather than merely loading the DLL.

## Build and verify

Requires the pinned Rust toolchain and Visual Studio C++ Build Tools. Cargo may
fetch locked dependencies during setup; the runtime has no network dependency.
Build-time binary downloads are disabled in ort. All model/runtime inputs to the
package script are existing local files and are verified by SHA-256.

```powershell
cargo xtask verify
# EvalRoot is the local recognition-eval input directory, never committed.
.\scripts\package.ps1 -EvalRoot C:\path\to\recognition-eval
```

The ZIP, extracted verification copy and hashes are under `target/package/`.
Existing packages are never overwritten. The ZIP includes source provenance,
model provenance, dependency licenses and ONNX Runtime notices. No publishing,
signing keys, installer or automatic download/update implementation is included.

Development-only PNG inference isolates rasterization from model parity:
`cargo build --release --example png`. Stage the same `models/` and DLL beside
`target/release/examples/png.exe`, then feed PNG paths, one per stdin line. The
example uses the dev-only image crate and is absent from the ZIP. Set
`GOODTYPE_DEV_TENSOR` to dump the last preprocessed float32 tensor for numerical
checks; this file-writing feature exists only in the example.

Opt-in evaluation (Python + psutil, developer only):
`python scripts/evaluate.py C:\path\to\recognition-eval --mode image`
and then `--mode stroke`, sequentially. Results and mismatch details stay under
that directory's `private-goodtype/`, never in Git. RSS is sampled every 10 ms.

## Inference

TexTeller 1.0.2 preprocessing is ported from its local Python source: majority
corner background, >15 grayscale difference trim, Torchvision RGB grayscale
(including 255 becoming 254), antialiased bicubic uint8 resize to short side 447
with maximum side 448, adaptive signed-16-bit coefficient precision, float32
multiply by 1/255, mean 0.9545467/std 0.15394445, then right/bottom zero padding
after normalization to 448 square. Bicubic coefficient is -0.5; separable passes
round and clamp uint8 intermediates. Blank images return an error.

Strokes render in tiny-skia, black on opaque white, antialiased, 6 px/point,
12 px outside ink bounds, widths times six, round caps/joins and filled circles
for single points. Crop origin is computed from the ink bounds. The pilot Canvas
crops used selection-box origins: raster phase and rasterizer antialiasing can
change pixels and predictions even for identical ink.

CPU execution provider, four intra-op threads, full graph optimization. Encoder
runs once; merged decoder takes a one-token input and past tensors thereafter,
retaining first-pass cross-attention cache because the cached branch returns
empty cross-attention outputs. Initial dummy tensors have length one and are
ignored by the false cache branch. Greedy first-index argmax, EOS 2, PAD 1,
maximum 1024 new tokens. Effective reference BOS is 0: its newly constructed
GenerationConfig omits decoder_start_token_id and overrides the nested config's
value 2. This behavior was checked against Python's emitted token sequence.
No forced EOS is added at the length limit.

Byte-level BPE *decoding* is a small inverse byte-to-Unicode table, vocabulary
lookup from tokenizer.json, added-token handling, special-token skipping,
UTF-8 replacement decoding, and the reference tokenizer's cleanup replacements.
No BPE encoder/training engine is necessary. TexTeller remove_style is ported
for bm, boldsymbol, textit, textbf and mathbf, including nested/escaped braces
and malformed groups. Display wrappers are stripped like the reference runner.
The format_latex and add_newlines whitespace formatting is omitted; no
mathematical token rewrite or LaTeX-to-Typst conversion is performed.

## Dependencies

`ort` 2.0.0-rc.12 dynamically calls the local ONNX Runtime 1.31.1 DLL using API 24
(backward-compatible runtime API). `serde`/`serde_json` provide bounded protocol
parsing/serialization. `tiny-skia` 0.11.4 rasterizes ink in pure Rust. `image`
0.25.10 is PNG-only and dev-only. No tokenizers crate, CLI framework, async runtime,
network client, hashing crate or PNG decoder ships. Cargo.lock pins transitive
versions; THIRD_PARTY.md records their licenses. See VERIFICATION.md for measured
results and limitations.
