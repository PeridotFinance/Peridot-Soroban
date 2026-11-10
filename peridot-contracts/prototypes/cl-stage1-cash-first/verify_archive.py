"""Hash every archived member and replay exact final profile gates/errors offline."""
import argparse,hashlib,json,subprocess,tarfile,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('directory',type=Path);a=p.parse_args();root=Path(__file__).resolve().parents[2]
index=json.loads((a.directory/'index.json').read_text());archive=a.directory/index['archive']['file']
assert hashlib.sha256(archive.read_bytes()).hexdigest()==index['archive']['sha256']
assert archive.stat().st_size==index['archive']['bytes']
with tarfile.open(archive,'r:xz') as tar:
 assert set(tar.getnames())==set(index['members'])
 for m in tar:
  assert m.isfile();b=tar.extractfile(m).read();pin=index['members'][m.name]
  assert len(b)==pin['bytes'] and hashlib.sha256(b).hexdigest()==pin['sha256'],m.name
 with tempfile.TemporaryDirectory(prefix='cl-stage1-replay-',dir=root/'target') as tmp:
  tmp=Path(tmp);config=tmp/'network.json';config.write_bytes(tar.extractfile('network-config.json').read())
  for run in ['cl-stage1-internal-v6','cl-stage1-captured-v6']:
   dest=tmp/run;dest.mkdir()
   for name in ['resources.jsonl','transactions.jsonl','profile.json','gates.json','budget-exit-code.txt']:
    (dest/name).write_bytes(tar.extractfile('runs/'+run+'/'+name).read())
   profile=json.loads((dest/'profile.json').read_text());profile['network_config_path']=str(config)
   (dest/'profile.json').write_text(json.dumps(profile))
   result=subprocess.run(['bash','scripts/check_budget.sh','--local-report',str(dest/'resources.jsonl'),'--profile',str(dest/'profile.json'),'--transaction-evidence',str(dest/'transactions.jsonl'),'--gate','invocation','--summary-json',str(dest/'replayed.json')],cwd=root,text=True,capture_output=True)
   assert result.returncode==int((dest/'budget-exit-code.txt').read_text()),result.stdout+result.stderr
   expected=json.loads((dest/'gates.json').read_text());actual=json.loads((dest/'replayed.json').read_text())
   assert actual==expected,(expected,actual)
   assert actual['gates']=={'invocation':True,'lifecycle':False,'transaction':False,'checkpoint':False}
   print(run,'EXACT GATES/ERRORS REPLAY PASS',len(actual['missing_required_flows']),'missing',len(actual['lifecycle_errors']),'lifecycle',len(actual['transaction_errors']),'transaction')
print('HASH/MEMBER VERIFICATION PASS',len(index['members']))
