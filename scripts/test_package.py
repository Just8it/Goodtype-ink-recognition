"""Test the downloaded/extracted ZIP, never the staging directory."""
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tempfile
from threading import Timer
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


def check_addon_manifest(addon, base_version):
    assert set(addon) == {
        'manifestVersion', 'id', 'version', 'apiVersion', 'target', 'entrypoint',
        'transport', 'permissions', 'commands', 'settings', 'strings',
    }
    assert type(addon['manifestVersion']) is int and addon['manifestVersion'] == 1
    assert addon['id'] == 'math' and addon['version'] == base_version
    assert addon['apiVersion'] == 'goodtype-api 0.1'
    assert addon['target'] == 'x86_64-pc-windows-msvc'
    assert addon['entrypoint'] == 'goodtype-math-recognize.exe'
    assert addon['transport'] == 'stdio-jsonrpc'
    assert addon['permissions'] == ['selected-ink', 'add-page-objects']
    assert addon['commands'] == [{
        'id': 'recognize-math', 'label': 'recognize-math', 'surface': 'ink-selection',
    }]
    assert addon['settings'] == []
    assert set(addon['strings']) == {'en', 'de'}
    for locale in addon['strings'].values():
        assert set(locale) == {'name', 'recognize-math'}
        for value in locale.values():
            assert 1 <= len(value) <= 256
            assert all(c not in '<>' and not (ord(c) < 32 or 127 <= ord(c) <= 159) for c in value)
            assert '{{' not in value and '}}' not in value
    assert addon['strings']['en'] != addon['strings']['de']


def session_smoke(exe, cwd, addon, strokes):
    with subprocess.Popen([str(exe), '--session'], cwd=cwd, text=True,
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, encoding='utf8') as process:
        timer = Timer(180, process.kill)
        timer.start()
        try:
            def exchange(id, method, params):
                frame = {'jsonrpc': '2.0', 'id': id, 'method': method, 'params': params}
                process.stdin.write(json.dumps(frame) + '\n')
                process.stdin.flush()
                reply = json.loads(process.stdout.readline())
                assert set(reply) == {'jsonrpc', 'id', 'result'}, reply
                assert reply['jsonrpc'] == '2.0' and reply['id'] == id
                return reply

            identity = {'id': addon['id'], 'version': addon['version']}
            offer = {'apiVersion': addon['apiVersion'], 'addon': identity,
                     'capabilities': ['progress', 'cancel', 'page-batch-v0']}
            handshake = exchange('1', 'initialize', offer)
            assert handshake['result'] == {**offer, 'capabilities': ['page-batch-v0', 'cancel']}
            response = exchange('2', 'command/invoke', {
                'command': 'recognize-math', 'settings': {}, 'scope': {
                    'kind': 'ink-selection', 'page': {'widthPt': 595.28, 'heightPt': 841.89},
                    'strokes': [{**stroke, 'id': f's{i}'} for i, stroke in enumerate(strokes, 1)],
                },
            })
            operation, = response['result']['batch']['operations']
            assert operation['op'] == 'addTypstBlock' and operation['source'].startswith('$ ')
            assert (operation['xPt'], operation['yPt'], operation['layoutWidthPt']) == (12, 46, 120)
            process.stdin.close()
            assert not process.stdout.read() and not process.stderr.read()
            assert process.wait() == 0
            print(json.dumps(handshake))
            print(json.dumps(response))
        finally:
            timer.cancel()
            if process.poll() is None:
                process.kill()


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
        addon = json.loads((package / 'goodtype-addon.json').read_text(encoding='utf8'))
        check_addon_manifest(addon, manifest['baseVersion'])
        exe = package / addon['entrypoint']
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
        # A page-sized "x"; the earlier 10 pt cross decoded to degenerate output on one CI runner.
        request = {'id':731,'strokes':[{'widthPt':2,'points':[[0,0],[40,40]]},{'widthPt':2,'points':[[0,40],[40,0]]}]}
        rows = [json.loads(line) for line in run('--serve', input=json.dumps(request)+'\n').splitlines()]
        assert len(rows) == 2 and rows[0]['ready'] is True
        assert rows[1]['id'] == 731 and isinstance(rows[1]['latex'], str) and rows[1]['latex']
        assert 'error' not in rows[1]
        session_smoke(exe, destination, addon, request['strokes'])
    print('Extracted ZIP: hashes, provenance, manifests, CLI version, --check, --serve and --session passed')


if __name__ == '__main__':
    check(Path(sys.argv[1]).resolve())
