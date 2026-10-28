"""Local package assembly; never downloads assets or publishes a release."""
import hashlib,json,shutil,subprocess,sys,zipfile
from pathlib import Path
r=Path(__file__).resolve().parents[1];b=Path(sys.argv[1]).resolve()
name='goodtype-math-recognizer-0.1.0-windows-x64';out=r/'target/package';out.mkdir(parents=True,exist_ok=True)
dest=out/name
if dest.exists() or (out/(name+'.zip')).exists():raise SystemExit('package already exists; preserve or move it before rebuilding')
dest.mkdir();(dest/'models').mkdir()
assets=[(r/'target/release/goodtype-math-recognize.exe','goodtype-math-recognize.exe',None),
(b/'texteller-int8/encoder_model.onnx','models/encoder_model.onnx','dfb48cc64f63eba1ae79d3ad5dfd690484b503b0eb9bdb5651d3cf404df303a7'),
(b/'texteller-int8/decoder_model_merged.onnx','models/decoder_model_merged.onnx','fdca7c0db645a16a2711bd2c6aebc3ad1ae00fdf9b45f4fe1d8f77b31c9c16c8'),
(b/'hf-cache/hub/models--OleehyO--TexTeller/snapshots/7b96df06b9d81cdb129c3bef68b7250bc3e2b0ea/tokenizer.json','models/tokenizer.json','3fd70ac72edab8213b7dd13271b3c69b6c2252ab3e04f8045939dad0847309be'),
(b/'.venv-texteller/Lib/site-packages/onnxruntime/capi/onnxruntime.dll','onnxruntime.dll','e45f61e50d8650885452be538a00bb14a715e46a1bc3691000ce947e89ed0306')]
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
for source,target,expected in assets:
 if expected and digest(source)!=expected:raise ValueError(f'asset hash mismatch: {target}')
 shutil.copyfile(source,dest/target)
for file in ['LICENSE','THIRD_PARTY.md','README.md','INTEGRATION.md','model-provenance.json']:shutil.copyfile(r/file,dest/file)
shutil.copytree(r/'licenses',dest/'licenses')
metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--filter-platform','x86_64-pc-windows-msvc'],cwd=r))
# Dev-only PNG dependencies are excluded from the shipped executable's license list.
packages={p['id']:p for p in metadata['packages']};nodes={n['id']:n for n in metadata['resolve']['nodes']};wanted=set()
def visit(id):
 if id in wanted:return
 wanted.add(id)
 for dep in nodes[id]['deps']:
  if any(k['kind']!='dev' for k in dep['dep_kinds']):visit(dep['pkg'])
visit(metadata['resolve']['root']);catalog=[]
for id in sorted(wanted):
 p=packages[id]
 if p['name']=='goodtype-math-recognize':continue
 source=Path(p['manifest_path']).parent
 files=set(f for pattern in ['LICENSE*','LICENCE*','COPYING*','NOTICE*'] for f in source.glob(pattern) if f.is_file())
 if p.get('license_file'):files.add(source/p['license_file'])
 if not files:raise ValueError('missing license: '+p['name'])
 target=dest/'licenses/rust'/f"{p['name']}-{p['version']}";target.mkdir(parents=True)
 for f in files:shutil.copyfile(f,target/f.name)
 catalog.append({'name':p['name'],'version':p['version'],'license':p['license']})
(dest/'licenses/rust-dependencies.json').write_text(json.dumps(catalog,indent=2)+'\n',encoding='utf8')
with (dest/'THIRD_PARTY.md').open('a',encoding='utf8') as f:
 f.write('\n## Locked shipped Rust dependency inventory\n\n| Crate | Version | License |\n|---|---|---|\n')
 for p in catalog:f.write(f"| {p['name']} | {p['version']} | {p['license']} |\n")
manifest={'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=r,text=True).strip(),'sourceDirty':bool(subprocess.check_output(['git','status','--porcelain'],cwd=r).strip()),'files':[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':digest(p)} for p in sorted(dest.rglob('*')) if p.is_file()]}
(dest/'package-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
archive=out/(name+'.zip')
with zipfile.ZipFile(archive,'x',zipfile.ZIP_DEFLATED,strict_timestamps=False) as z:
 for p in sorted(dest.rglob('*')):
  if p.is_file():z.write(p,Path(name)/p.relative_to(dest))
# Test bytes extracted from the archive, independently of build and staging directories.
extracted=out/'extracted';extracted.mkdir(exist_ok=True)
with zipfile.ZipFile(archive) as z:z.extractall(extracted)
exe=extracted/name/'goodtype-math-recognize.exe'
for args in [['--version','--json'],['--check']]:subprocess.run([str(exe),*args],cwd=extracted,check=True)
result={'file':archive.name,'bytes':archive.stat().st_size,'sha256':digest(archive)}
(out/'package-result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
