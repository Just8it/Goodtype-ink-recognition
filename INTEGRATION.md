# Protocol v1 integration contract

Executable: `goodtype-math-recognize.exe`, version 0.1.0. Optional child process;
Goodtype downloads only after opt-in and owns process lifetime, cancellation,
user consent and LaTeX-to-Typst conversion. The helper never changes notebooks.

* `--version --json`: exactly one JSON line with name, version, protocolVersion=1,
  model id=texteller-int8, baseRevision, encoderSha256, decoderSha256; exit 0.
* `--check`: load plus synthetic cross recognition. One JSON line with ok,
  loadMs, recognizeMs, latex; nonzero failure includes ok=false and error.
* `--serve`: load once, emit and flush ready=true/loadMs before reading stdin.
  Process requests serially, exactly one flushed response per line; EOF exits 0.

Request: `{"id":1,"strokes":[{"widthPt":1,"points":[[0,0],[10,10]]}]}`.
Success: `{"id":1,"latex":"..."}`. Failure: `{"id":1,"error":"..."}`.
Coordinates are absolute page points. Empty strokes are ignored; all-empty input
has no recognizable ink and produces an error. No timestamps are used.

Limits: 4 MiB per request line (including newline), 1?2000 strokes,
200000 total points, finite coordinates of absolute value at most 100000,
finite widthPt strictly greater than zero and at most 100. Parse errors use a
recoverable u64 id where possible, otherwise 0. Oversized lines are drained
without buffering unbounded input and use id 0; serving continues. Unknown JSON
fields are ignored. Malformed or invalid requests do not unload the model.

Additional resource guard: raster allocation above 64 megapixels returns an
error, because protocol-valid coordinates can otherwise request terabytes.
Valid ink can take the full 1024-token decode budget; the protocol has no
cancellation request or deadline. The parent should use a Job Object and timeout.
No protocol extension was made. Startup failure is ok=false/error and nonzero
exit; it cannot emit ready=true. Logs/diagnostics are stderr only. Stdout is JSONL.

Package model/runtime hashes are verified by the build script and recorded in
package-manifest.json. Runtime version reports pinned model identity, not an
on-start cryptographic integrity check. Goodtype must authenticate the optional
archive before extraction. No signing or remote release mechanism is included.

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

CPU execution provider, four intra-op threads, full graph optimization. ONNX
Runtime telemetry is explicitly disabled before model loading. Encoder
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
