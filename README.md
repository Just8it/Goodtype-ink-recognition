[![checks](https://github.com/Just8it/Goodtype-ink-recognition/actions/workflows/ci.yml/badge.svg)](https://github.com/Just8it/Goodtype-ink-recognition/actions)
[![license](https://img.shields.io/badge/license-Apache%202.0-green)](./LICENSE)
[![platform](https://img.shields.io/badge/platform-Windows-blue)](#use)
[![built with](https://img.shields.io/badge/built%20with-Rust-orange)](#build)

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/mark-dark.svg">
    <img src="brand/mark-light.svg" width="130" alt="Goodtype">
  </picture>
</p>

<h1 align="center">Goodtype Ink Recognition</h1>

<p align="center">Turn handwritten math into editable content. Keep your notes on your own disk.</p>

An optional math recognition add-on for [Goodtype](https://github.com/Just8it/Goodtype).
Goodtype sends selected strokes to this helper, receives LaTeX and converts it to editable Typst.
You review and correct the result before inserting it. Your handwriting stays unchanged.

Recognition runs offline on your CPU. No account, upload or telemetry. The model and runtime
are a separate download of roughly 340 MB; Goodtype works without them. Ordinary text
recognition uses Windows directly and does not need this add-on.

## Alpha

The recognizer can misread letters, symbols and layout. Check the result before using it.
It handles selected expressions, not whole notebook pages. See the
[test results](./VERIFICATION.md) for measured accuracy, speed and limitations.

## Use

Download the **Windows x64 ZIP** from [Releases](https://github.com/Just8it/Goodtype-ink-recognition/releases),
extract it and keep the whole folder together. Use a stable release for normal use;
prereleases are development candidates. No Python or separate runtime installation is needed.

To check an extracted package:

    .\goodtype-math-recognize.exe --check

The helper communicates with Goodtype through a local child process. It does not open or modify
notebooks. [INTEGRATION.md](./INTEGRATION.md) describes the protocol and model implementation.

## Build

Requires the pinned Rust toolchain, Python for build scripts, and Visual Studio C++ Build Tools
on Windows. Python is not needed by the installed add-on.

    cargo xtask verify

See [RELEASING.md](./RELEASING.md) for packaging, pinned model inputs and the
feature → dev → main release process. Downloaded models and build output stay out of Git.

## License

Apache License 2.0. [THIRD_PARTY.md](./THIRD_PARTY.md) records TexTeller, ONNX Runtime and
Rust dependencies. Release packages include their licenses and model provenance.
