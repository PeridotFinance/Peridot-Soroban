#!/usr/bin/env python3
import subprocess,json,pathlib,datetime,hashlib,sys
P=pathlib.Path(__file__).resolve().parent
name,url=sys.argv[1:]
if (P/name).exists(): name=str(len(list(P.iterdir())))+'-'+name
r=subprocess.run(['curl','-sS','-L','--max-time','45','-A','Mozilla/5.0',url,'-w','\n%{http_code}'],capture_output=True)
b,_,status=r.stdout.rpartition(b'\n');(P/name).write_bytes(b)
p=P/'sources.json';s=json.loads(p.read_text())if p.exists()else[];s.append(dict(file=name,url=url,time=datetime.datetime.now(datetime.timezone.utc).isoformat(),status=status.decode(),exit=r.returncode,error=r.stderr.decode(),bytes=len(b),sha256=hashlib.sha256(b).hexdigest()));p.write_text(json.dumps(s,indent=2)+'\n');print(name,status.decode(),len(b))
