"""Acquire only pinned public build inputs; never reads private evaluation data."""
import gc
import hashlib
from importlib import import_module
import json
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TOKENIZER_HASH = '3fd70ac72edab8213b7dd13271b3c69b6c2252ab3e04f8045939dad0847309be'
RUNTIME_HASH = 'e45f61e50d8650885452be538a00bb14a715e46a1bc3691000ce947e89ed0306'


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def validate(path, expected):
    if digest(path) != expected:
        raise ValueError(f'checksum mismatch: {path}')


def fetch(url, path, expected, limit):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        validate(path, expected)
        return
    partial = path.with_suffix(path.suffix + '.partial')
    try:
        with urllib.request.urlopen(url, timeout=120) as response, partial.open('wb') as stream:
            total = 0
            while chunk := response.read(1024 * 1024):
                total += len(chunk)
                if total > limit:
                    raise ValueError('download exceeds pinned size limit')
                stream.write(chunk)
        validate(partial, expected)
        partial.rename(path)
    finally:
        partial.unlink(missing_ok=True)


def main():
    if sys.platform != 'win32' or sys.version_info[:2] != (3, 12):
        raise ValueError('reproduction requires Windows x64 Python 3.12')
    base = ROOT / 'target/public-inputs'
    pins = json.loads((ROOT / 'scripts/build-inputs.json').read_text())
    wheels = []
    for pin in pins.values():
        wheel = base / 'wheels' / pin['file']
        fetch(pin['url'], wheel, pin['sha256'], 100 * 1024 * 1024)
        wheels.append(str(wheel))
    subprocess.run([sys.executable, '-m', 'pip', 'install', '--no-index', '--no-deps', '--force-reinstall', *wheels], check=True)
    # Extract just the shipped DLL from the hash-checked Windows wheel.
    runtime = base / 'onnxruntime.dll'
    with zipfile.ZipFile(base / 'wheels' / pins['onnxruntime']['file']) as archive:
        runtime.write_bytes(archive.read('onnxruntime/capi/onnxruntime.dll'))
    validate(runtime, RUNTIME_HASH)
    import onnx
    import onnxruntime
    from onnxruntime.quantization import QuantType, quantize_dynamic
    from onnxruntime.quantization.quant_utils import add_infer_metadata
    provenance = json.loads((ROOT / 'model-provenance.json').read_text())
    if onnx.__version__ != provenance['onnx'] or onnxruntime.__version__ != provenance['onnxruntime']:
        raise ValueError('quantizer version differs from provenance')
    url = f"https://huggingface.co/OleehyO/TexTeller/resolve/{provenance['source_revision']}"
    fetch(url + '/tokenizer.json', base / 'tokenizer.json', TOKENIZER_HASH, 16 * 1024 * 1024)
    def load_in_memory(path):
        model = onnx.shape_inference.infer_shapes(onnx.load(str(path)))
        add_infer_metadata(model)
        return model
    for name, pin in provenance['files'].items():
        source, target = base / 'source' / name, base / 'texteller-int8' / name
        fetch(url + '/' + name, source, pin['source_sha256'], pin['source_bytes'])
        target.parent.mkdir(exist_ok=True)
        if not target.exists():
            temporary = target.with_suffix('.partial.onnx')
            try:
                with patch.object(import_module('onnxruntime.quantization.quantize'), 'load_model_with_shape_infer', load_in_memory):
                    quantize_dynamic(source, temporary, weight_type=QuantType.QInt8, **provenance['settings'])
                validate(temporary, pin['output_sha256'])
                onnx.checker.check_model(str(temporary))
                temporary.rename(target)
            finally:
                temporary.unlink(missing_ok=True)
            gc.collect()
        validate(target, pin['output_sha256'])
        validate(source, pin['source_sha256'])
    print(f'Hash-verified public assets: {base}')


if __name__ == '__main__':
    main()
