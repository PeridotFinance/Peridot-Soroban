import json,subprocess,pathlib
P=pathlib.Path(__file__).resolve().parent
for i,pool in enumerate(json.loads((P/'selected-pools.json').read_text()),1):
 page=1
 while True:
  name=f'p{i}-week-events-{page}.json';url=f'https://amm-api.aqua.network/events/pool/{pool["address"]}/?size=1000&page={page}&ledger_close_at__gte=2026-11-04T00%3A00%3A00Z&ledger_close_at__lte=2026-11-11T00%3A00%3A00Z'
  subprocess.run(['python3',str(P/'pull.py'),name,url],check=True)
  d=json.loads((P/name).read_text())
  if not d.get('next'):break
  page+=1
  if page>12:raise RuntimeError('bounded pagination cap reached; history incomplete')
