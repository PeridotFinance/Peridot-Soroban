#!/usr/bin/env python3
import json,hashlib,urllib.request,urllib.error,sys,datetime
from pathlib import Path
P=Path(__file__).resolve().parent
name,url=sys.argv[1:3]
r={'name':name,'url':url,'retrieved_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
try:
 with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'Peridot-read-only-research/1.0'}),timeout=30)as x:b=x.read();r.update(status=x.status)
except urllib.error.HTTPError as e:b=e.read();r.update(status=e.code,error=str(e))
except Exception as e:b=b'';r.update(error=str(e))
p=P/(name+'.raw');p.write_bytes(b);r.update(file=p.name,bytes=len(b),sha256=hashlib.sha256(b).hexdigest());m=P/'sources.json';rows=json.loads(m.read_text())if m.exists()else[];rows.append(r);m.write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(r))
