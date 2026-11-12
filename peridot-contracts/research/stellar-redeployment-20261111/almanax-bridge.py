"""Bounded MCP client; existing launch-services credential stays in memory."""
import json,subprocess,sys,urllib.request
key=subprocess.run(['launchctl','getenv','ALMANAX_API_KEY'],capture_output=True,check=True).stdout.decode().strip()
if not key: raise SystemExit('Credential unavailable')
headers={'Authorization':'Bearer '+key,'Content-Type':'application/json','Accept':'application/json, text/event-stream'}
def call(body):
 req=urllib.request.Request('https://mcp.almanax.ai',data=json.dumps(body).encode(),headers=headers)
 with urllib.request.urlopen(req,timeout=60) as r:
  if r.headers.get('Mcp-Session-Id'):headers['Mcp-Session-Id']=r.headers['Mcp-Session-Id']
  raw=r.read().decode()
  try:return json.loads(raw)
  except ValueError:
   for line in raw.splitlines():
    if line.startswith('data: '):return json.loads(line[6:])
call({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'peridot-review','version':'1'}}})
name=sys.argv[1]
assert name in ['almanax_scan_commit','almanax_get_scan_status','almanax_get_findings_for_scan','almanax_get_findings_for_commit']
print(json.dumps(call({'jsonrpc':'2.0','id':2,'method':'tools/call','params':{'name':name,'arguments':json.loads(sys.argv[2])}}),indent=2))
