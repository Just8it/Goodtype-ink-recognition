# Releasing Goodtype Math Recognition

Pull requests and branch pushes verify the source and build a Windows x64 ZIP as a
CI artifact. Local and CI packages use the plain Cargo version, which must also
match goodtype-addon.json.

After checks pass and the change is merged into main, push a new matching version
tag such as v0.1.1 on the merged commit. Tag CI verifies main ancestry, rebuilds
and tests the ZIP, then signs and publishes recognizer-update.json and its detached
signature alongside the ZIP, build descriptor and SHA256SUMS. Published tags and
assets are immutable.

The release environment supplies MATH_RECOGNIZER_SIGNING_KEY. The publisher
verifies the signature with release-public-key.pem before uploading. Key rotation
requires a coordinated Goodtype catalog update.

Keep the existing required checks, Branch policy and Windows verification.
Configure a selected Tag rule such as `v*` in the `release` environment before
the first tag push. Protect published tags and enable immutable releases.

## Local verification and packaging

Install the pinned Rust toolchain, Node 24, and Windows x64 Python 3.12.10.
Run `cargo xtask verify`, then `./scripts/package.ps1` in PowerShell.
The binary name remains `goodtype-math-recognize`; the package name is
`goodtype-math-recognizer-VERSION-windows-x64`.

The public-input path downloads only the pinned Hugging Face revision in
model-provenance.json and SHA-256-pinned Windows wheels in scripts/build-inputs.json.
Downloads are size bounded and cached below target/public-inputs; cache hits are
rehashed. Source ONNX models and tokenizer are hashed, dynamic INT8 quantization
uses the recorded in-memory shape-inference recipe, and both generated models
must match the existing provenance hashes exactly. The ONNXRuntime DLL is extracted
from its pinned wheel and checked separately. Wheel installation uses --no-index
--no-deps; every required import is explicitly pinned. No private evaluation data,
Python runtime, source FP32 models or build caches enter the ZIP.

For an existing local evaluation cache, `./scripts/package.ps1 -EvalRoot PATH`
retains the older offline path and validates the same model/tokenizer/runtime hashes.
Only an explicit file allowlist is copied. Never upload the evaluation directory.
Build input network access is limited to public PyPI wheels (files.pythonhosted.org)
and the pinned Hugging Face model (including its HTTPS CDN redirects); the shipped
recognizer needs no network. No binaries are committed.

The extracted ZIP must pass inventory and file hashes, descriptor/SHA256SUMS,
manifest/protocol/version/model provenance, --check inference, a bounded
--serve request, and a --session handshake and invocation followed by EOF. Every subprocess has a 180-second timeout.
These are synthetic smoke tests, not recognition-quality or pen-hardware evidence.

## Implementation validation (2026-10-28)

Fresh public downloads in an isolated Python 3.12.10 environment reproduced both
INT8 outputs exactly: encoder dfb48cc64f63eba1ae79d3ad5dfd690484b503b0eb9bdb5651d3cf404df303a7,
decoder fdca7c0db645a16a2711bd2c6aebc3ad1ae00fdf9b45f4fe1d8f77b31c9c16c8.
The resulting local ZIP 0.1.0-local.20261028160326 (337067457 bytes,
SHA-256 3def02c94bef74f0fa1eb691af7f9ee5dcc5dd089032436d5a16e6d37e413282)
passed extracted-file hashes, version/protocol/provenance, --check and --serve.
This was a dirty local development build at source HEAD398c3e0, not a published
candidate or stable acceptance record. cargo xtask verify passed five Rust tests,
two Python release tests and six Node release tests, formatting and Clippy.
Protected-key signing and hosted CI publication still require the parent run.
