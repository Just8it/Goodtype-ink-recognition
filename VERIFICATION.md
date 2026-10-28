# Local verification ? 2026-10-28

Windows x64, Rust 1.97.1, CPU ONNX Runtime 1.31.1, four intra-op threads.
Private formulas/strokes are not stored here. Evaluations ran sequentially,
without simultaneous inference. Other ordinary host processes were present;
these timings are observations, not isolated-machine benchmarks or device gates.
p95 is nearest rank ceil(0.95*26). Timing includes request/response serialization
for strokes and PNG loading for images. Load time is DLL/model/tokenizer setup.

| Pipeline | Matches to Python PNG reference | Load seconds | Median seconds | p95 seconds | Peak RSS bytes |
|---|---:|---:|---:|---:|---:|
| Rust PNG | 26/26 | 2.6993622 | 0.58824385 | 0.7892835 | 656424960 |
| Rust strokes | 24/26 | 2.4129421 | 0.56918130 | 0.8165154 | 655794176 |

RSS uses Windows peak working set from psutil, sampled every 10 ms. PNG matching
removes whitespace and display delimiters only. There are no PNG mismatches.
Stroke mismatch IDs are m04 and m08; full Python/Rust outputs remain exclusively
in the private result file's mismatches array. The same PNGs reproduce all 26
predictions, isolating stroke differences to rasterization/crop phase rather
than inference. tiny-skia and Canvas antialiasing and the computed ink-bounds
origin versus selection-box origin are not pixel-identical.

Scorer results (26 samples, conservative format normalization):

| Pipeline | Exact | Within 1 | Within 2 | Within 3 | Median edits | Token edit rate | Parseable pairs | Same layout |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Python INT8 PNG | 10 | 16 | 18 | 19 | 1 | 0.2084942084942085 | 22 | 17 |
| Rust INT8 strokes | 11 | 17 | 18 | 19 | 1 | 0.20077220077220076 | 22 | 17 |

The only sibling source edit adds rust-int8-math.json to the scorer's two lists.
Its execution redirected the scorer's aggregate output path in memory into
private-goodtype/rust-partial-comparison.json to preserve the allowed write
boundary; scoring code was unchanged. Private result files have id, label,
prediction, seconds, load_seconds and observed_rss_bytes.

`cargo xtask verify` passes rustfmt, five model-free unit tests and Clippy with
warnings denied. Tests cover bounds/nonfinite values/stroke and point limits,
framing/recovery/id echo/EOF, round point geometry/margins, byte decoding with
known IDs and nested style removal. Both real evaluation child processes exited
0 on EOF and all 26 serve responses echoed the request id.

`check_tokenizer.py` passes 16000 cases: all 15000 token IDs independently plus
1000 deterministic mixed sequences, against local RobertaTokenizerFast. No
mismatches. PNG parity checks all 26 private reference outputs. A developer tensor
check also identified and resolved grayscale conversion, float scaling and
adaptive resize coefficient precision; one checked tensor matched bit for bit.

The packaging script verifies all model/tokenizer/DLL source hashes, packages
licenses and source provenance, extracts the ZIP, then runs that copy's
`--version --json` and `--check`. Exact archive results are recorded separately
in target/package/package-result.json and the local handoff; this avoids a ZIP
hash self-reference. This is a local build, not a release or signed artifact.

Protocol considerations: no deadline/cancel request; all-empty strokes return
an error; enormous valid coordinate spans hit a documented 64-megapixel resource
guard; oversized/unparseable frames can return id 0. The DLL uses compatible
ONNX C API 24. No physical pen validation, Goodtype end-to-end integration,
networked deployment or fresh-machine portability test is claimed.
