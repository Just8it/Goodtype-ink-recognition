# Goodtype Ink Recognition 0.1.0

Portable Windows x64 offline TexTeller INT8 recognition helper for Goodtype.
Extract the entire ZIP; retain the models, ONNXRuntime DLL and license notices.
CLI/protocol compatibility: goodtype-math-recognize, JSON Lines protocol version 1.

Builds pin the public model revision, quantization recipe, tokenizer and runtime;
SHA-256 verification is mandatory. Source provenance and file hashes are included.
Release metadata is signed with the repository's dedicated Ed25519 key.

Development candidates require actual downloaded-package testing before promotion.
Synthetic inference checks do not establish handwriting accuracy or physical pen
compatibility. Review recognition results before applying them to a notebook.
