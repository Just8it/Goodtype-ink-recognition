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
