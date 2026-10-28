# Third-party components

* TexTeller 1.0.2 and OleehyO/TexTeller weights, Apache-2.0, base revision
  `7b96df06b9d81cdb129c3bef68b7250bc3e2b0ea`.
  Upstream: https://github.com/OleehyO/TexTeller and
  https://huggingface.co/OleehyO/TexTeller . The weights are modified: dynamically
  quantized from FP32 to INT8 by quantize_texteller.py with onnxruntime 1.31.1,
  QInt8 symmetric per-channel weights, dynamic QUInt8 activations, MatMul/Gemm,
  EnableSubgraph=true, MatMulConstBOnly=true, reduce_range=false, no external
  data. model-provenance.json retains exact source/output hashes and settings.
  Python preprocessing, byte decoding and style-removal behavior are ported to
  Rust; formatting-only steps are omitted. See licenses/TexTeller-Apache-2.0.txt.
* Microsoft ONNX Runtime **1.31.1**, MIT. Existing official Microsoft Python
  wheel's CPU `onnxruntime.dll`, 19,288,376 bytes, SHA-256
  `e45f61e50d8650885452be538a00bb14a715e46a1bc3691000ce947e89ed0306`.
  https://github.com/microsoft/onnxruntime . No build-time runtime binary download.
  API 24 via ort 2.0.0-rc.12; the newer DLL supplies the compatible API. Its MIT
  license and bundled ThirdPartyNotices.txt are retained under licenses/.
* Direct Rust dependencies: ort supplies safe dynamic ONNX bindings;
  serde/serde_json the protocol; tiny-skia pure-Rust antialiased paths. image
  is PNG-only, development-only. The inventory below includes all locked
  crates, including dev/platform dependencies; only normal Windows dependency
  licenses are copied into the distribution. The package's inventory separately
  lists shipped dependencies. No private samples are redistributed.

| Crate | Version | License |
|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| crc32fast | 1.5.3 | MIT OR Apache-2.0 |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| image | 0.25.10 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| libloading | 0.9.0 | ISC |
| log | 0.4.34 | MIT OR Apache-2.0 |
| matrixmultiply | 0.3.11 | MIT/Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| ndarray | 0.17.2 | MIT OR Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| ort | 2.0.0-rc.12 | MIT OR Apache-2.0 |
| ort-sys | 2.0.0-rc.12 | MIT OR Apache-2.0 |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| png | 0.18.1 | MIT OR Apache-2.0 |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| rawpointer | 0.2.1 | MIT/Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | MIT |
| smallvec | 1.16.4 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| tiny-skia | 0.11.4 | BSD-3-Clause |
| tiny-skia-path | 0.11.4 | BSD-3-Clause |
| tracing | 0.1.44 | MIT |
| tracing-core | 0.1.36 | MIT |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
