"""Dev-only exhaustive vocabulary + deterministic sequence parity with Python."""
import json,random,subprocess,sys
from pathlib import Path
from transformers import RobertaTokenizerFast
path=Path(sys.argv[1]);t=RobertaTokenizerFast.from_pretrained(path.parent,local_files_only=True)
r=Path(__file__).resolve().parents[1];rng=random.Random(0)
size=len(t)
sequences=[[i] for i in range(size)]+[[rng.randrange(size) for _ in range(rng.randrange(1,100))] for _ in range(1000)]
p=subprocess.run([str(r/'target/release/examples/decode.exe'),str(path)],input=''.join(json.dumps(s)+'\n' for s in sequences),text=True,encoding='utf8',stdout=subprocess.PIPE,check=True)
outputs=[json.loads(s) for s in p.stdout.rstrip("\n").split("\n")];mismatches=[]
for i,(seq,got) in enumerate(zip(sequences,outputs)):
 expected=t.decode(seq,skip_special_tokens=True)
 if got!=expected:mismatches.append(i)
assert len(outputs)==len(sequences)
print(json.dumps({'cases':len(sequences),'mismatches':len(mismatches),'first_indices':mismatches[:10]}))
assert not mismatches
