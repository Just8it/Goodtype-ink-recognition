"""Test the downloaded/extracted ZIP, never the staging directory."""
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tempfile
import zipfile
from acquire_inputs import digest


def safe_members(archive):
    infos = archive.infolist()
    if len(infos) > 2000 or sum(i.file_size for i in infos) > 1024 * 1024 * 1024:
        raise ValueError('package exceeds limits')
    names = set()
    for i in infos:
        parts = PurePosixPath(i.filename).parts
        if not parts or any(p in ('.', '..') for p in parts) or any(c in i.filename for c in '\\:\0') or i.filename.startswith('/'):
            raise ValueError('unsafe ZIP path')
        if i.filename.casefold() in names or (i.external_attr >> 16) & 0o170000 == 0o120000:
            raise ValueError('duplicate or symlink ZIP member')
        names.add(i.filename.casefold())
    return infos


def check(archive):
    descriptor = json.loads(archive.with_suffix('.json').read_text(encoding='utf8'))
    assert digest(archive) == descriptor['archive']['sha256']
    assert archive.stat().st_size == descriptor['archive']['sizeBytes']
    assert archive.name == descriptor['archive']['fileName']
    sums = archive.parent.joinpath('SHA256SUMS').read_text().splitlines()
    assert f"{digest(archive)}  {archive.name}" in sums
    assert f"{digest(archive.with_suffix('.json'))}  {archive.with_suffix('.json').name}" in sums
    with tempfile.TemporaryDirectory(prefix='recognizer-extracted-') as temporary:
        destination = Path(temporary)
        with zipfile.ZipFile(archive) as zipped:
            infos = safe_members(zipped)
            assert all(i.filename.startswith(descriptor['packageName'] + '/') for i in infos)
            zipped.extractall(destination)
        package = destination / descriptor['packageName']
        manifest = json.loads((package / 'importer-manifest.json').read_text(encoding='utf8'))
        for key in ('version','baseVersion','channel','sourceCommit','sourceDirty','target','packageName'):
            assert manifest[key] == descriptor[key], key
        actual = {p.relative_to(package).as_posix() for p in package.rglob('*') if p.is_file()}
        assert actual == {f['path'] for f in manifest['files']} | {'importer-manifest.json'}
        for entry in manifest['files']:
            p = package / entry['path']
            assert p.stat().st_size == entry['sizeBytes'] and digest(p) == entry['sha256'], entry['path']
        exe = package / 'goodtype-math-recognize.exe'
        def run(*args, **kwargs):
            return subprocess.run([str(exe), *args], cwd=destination, text=True, capture_output=True, check=True, timeout=180, **kwargs).stdout
        version = json.loads(run('--version','--json'))
        assert version['name'] == 'goodtype-math-recognize'
        assert version['version'] == descriptor['version']
        assert version['protocolVersion'] == 1
        provenance = json.loads((package / 'model-provenance.json').read_text())
        assert version['model']['baseRevision'] == provenance['source_revision']
        for filename, key in [('encoder_model.onnx','encoderSha256'),('decoder_model_merged.onnx','decoderSha256')]:
            assert digest(package / 'models' / filename) == version['model'][key] == provenance['files'][filename]['output_sha256']
        assert json.loads(run('--check'))['ok'] is True
        request = {'id':731,'strokes':[{'widthPt':1,'points':[[0,0],[10,10]]},{'widthPt':1,'points':[[0,10],[10,0]]}]}
        rows = [json.loads(line) for line in run('--serve', input=json.dumps(request)+'\n').splitlines()]
        assert len(rows) == 2 and rows[0]['ready'] is True
        assert rows[1]['id'] == 731 and isinstance(rows[1]['latex'], str) and rows[1]['latex']
        assert 'error' not in rows[1]
    print('Extracted ZIP: hashes, provenance, CLI version/protocol, --check and bounded --serve passed')


if __name__ == '__main__':
    check(Path(sys.argv[1]).resolve())
