# Releasing Goodtype Ink Recognition

The release owner uses feature/* or fix/* -> dev -> a tested candidate -> main.
Never push directly to dev/main. Promotion uses a merge commit; feature -> main,
squash/rebase promotions, and a main tree different from the tested dev tree fail policy.

## Local verification and packaging

Install the pinned Rust toolchain, Node 24, and Windows x64 Python 3.12.10.
Run `cargo xtask verify`, then `./scripts/package.ps1` (PowerShell). The default
local version has a timestamp. `-Channel dev` and `-Channel stable` require the
matching GitHub push environment and a clean checkout. Dev versions are
`CargoVersion-dev.GITHUB_RUN_NUMBER.GITHUB_RUN_ATTEMPT`; stable uses Cargo.toml.
The prepared version is compiled through GOODTYPE_BUILD_VERSION without editing
Cargo.toml. Binary/protocol name remains goodtype-math-recognize; package name
remains goodtype-math-recognizer-VERSION-windows-x64.

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
manifest/protocol/version/model provenance, --check inference, and a bounded
--serve request followed by EOF. Every subprocess has a 180-second timeout.
These are synthetic smoke tests, not recognition-quality or pen-hardware evidence.

## Candidate acceptance and promotion

Each successful dev push publishes a new prerelease tag and downloadable ZIP,
versioned JSON descriptor, SHA256SUMS, RELEASE_NOTES.md, recognizer-update.json
and recognizer-update.json.sig. The ZIP contains importer-manifest.json with
version, protocolVersion=1, notebookSchemaVersion=0, decoderCommit="", target,
source provenance and files[{path,sizeBytes,sha256}]. The signed update descriptor
uses archive{fileName,sizeBytes,sha256,url}, matching Goodtype's installer.
Goodtype should fetch recognizer-update.json plus recognizer-update.json.sig.

Download the exact candidate ZIP and descriptor/SHA256SUMS, verify the signed
metadata with release-public-key.pem, and run `python scripts/test_package.py ZIP`
with the matching descriptor and SHA256SUMS beside it. Also test the actual intended
Goodtype recognition workflow and record limitations. In the dev -> main PR body:

```
Tested dev build: v0.1.0-dev.RUN.ATTEMPT
Test evidence: actual downloaded-ZIP, installer and recognition results; limitations
```

The tag must be a published prerelease at the exact PR head, with successful dev
CI. Changes to dev require a new candidate and fresh evidence. Policy rechecks
main publication against the merged PR, its second parent and identical source
tree. Stable is rebuilt after all checks pass, never copied from an unverified
working tree. Bump Cargo.toml and Cargo.lock together through dev before the next
stable version. Synchronize main back to dev through a PR after promotion.

## Required repository configuration

- Protect dev/main: PRs only, strict up-to-date required checks `Branch policy` and
  `Windows verification`, resolved conversations, no force push/deletion/bypass,
  including administrators. Do not require `Publish Windows release` on PRs
  because that job intentionally runs only after pushes.
- Enable merge commits. Main promotions must use them; policy rejects other methods.
- Protect v* tags and enable GitHub immutable releases. Publication only creates
  new tags/releases and fails if a version already exists; it never edits assets.
- Configure environment `release` to allow protected branches only. The only
  write permission is contents:write in the post-verification publisher; no job
  pushes main/dev. Give the workflow actions:read for candidate CI verification.
- Environment secret `MATH_RECOGNIZER_SIGNING_KEY`: the Ed25519 private key in PEM
  form matching the tracked release-public-key.pem. Parent has provisioned it.
  No additional public-key variable is needed by the script. No key is generated
  by CI. Signing follows the sibling importer OpenSSL pkeyutl -rawin approach,
  validates source identity and verifies the signature before publication.
- Branch/setting/key changes and publication belong to the parent. No PAT needed;
  the publisher uses GITHUB_TOKEN. Do not expose signing credentials to PR jobs.

The CI artifact upload explicitly includes release outputs only. Actions caches
and private local files are never uploaded. CI starts fresh; local successful
packages are retained under target/package. Failed packages must be inspected and
moved aside before retrying the same version; versions are never overwritten.

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
