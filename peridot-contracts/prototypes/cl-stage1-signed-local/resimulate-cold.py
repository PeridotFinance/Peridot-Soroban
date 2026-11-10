"""Called while the private Core process retains its applied ledger. No network."""
from pathlib import Path
import json,subprocess,sys
root=Path(__file__).resolve().parents[2];d=Path(sys.argv[1]).resolve()
lines=(d/'restored-state.txt').read_text().splitlines();ledger,time,count=map(int,lines[:3]);assert len(lines)==3+count*3
v=json.loads((d/'input.json').read_text());v.update(ledger=ledger,timestamp=time,entries=[dict(key_xdr=lines[3+i*3],entry_xdr=lines[4+i*3],ttl=int(lines[5+i*3])or None)for i in range(count)],mode='record',result_path=str(d/'recorded.json'))
(d/'input.json').write_text(json.dumps(v))
# Decode the actual source sequence without reading any key store.
js="const fs=require('fs'),{xdr}=require('./prototypes/cl-stage1-signed-local/accounts.cjs');const p=process.argv[1],v=JSON.parse(fs.readFileSync(p));for(const r of v.entries){const e=xdr.LedgerEntry.fromXDR(r.entry_xdr,'base64');if(e.data().switch().name==='account'&&e.data().account().accountId().toXDR('base64')===v.source_account_xdr)v.source_sequence=e.data().account().seqNum().toString();}if(v.source_sequence===undefined)throw Error('source missing');fs.writeFileSync(p,JSON.stringify(v));"
subprocess.run(['node','-e',js,str(d/'input.json')],cwd=root,check=True)
subprocess.run(['bash','prototypes/cl-stage1-signed-local/run-host.sh',str(d/'input.json'),str(d/'record-run')],cwd=root,check=True)
subprocess.run(['node','prototypes/cl-stage1-signed-local/sign-local.cjs',str(d)],cwd=root,check=True)
subprocess.run(['bash','prototypes/cl-stage1-signed-local/run-host.sh',str(d/'enforce-input.json'),str(d/'enforce-run')],cwd=root,check=True)
