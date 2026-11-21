#!/usr/bin/env python3
"""On-chain D3 heartbeat gap distribution (TESTNET, Horizon, public data only).
usage: gaps.py <since-iso> <until-iso|now> <receipt-contract> <account>...
Lists every SUCCESSFUL transaction from the given accounts whose op invokes `checkpoint_nav` on the
receipt, merges them by ledger close time and reports the gaps between consecutive included calls."""
import json, sys, urllib.request, base64, datetime, struct

def get(url):
    with urllib.request.urlopen(url, timeout=30) as r:
        return json.load(r)

def has_call(env_b64, fn):
    raw = base64.b64decode(env_b64)
    return fn.encode() in raw  # function-name symbol bytes appear verbatim in the envelope

since, until, receipt, accts = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
until = until if until != 'now' else datetime.datetime.utcnow().strftime('%Y-%m-%dT%H:%M:%SZ')
times = []
for a in accts:
    url = f"https://horizon-testnet.stellar.org/accounts/{a}/transactions?order=asc&limit=200"
    while True:
        recs = get(url)['_embedded']['records']
        if not recs: break
        for x in recs:
            if since <= x['created_at'] <= until and x['successful'] and has_call(x['envelope_xdr'], 'checkpoint_nav') and not has_call(x['envelope_xdr'], 'checkpoint_fallback'):
                t = int(datetime.datetime.strptime(x['created_at'], '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=datetime.timezone.utc).timestamp())
                times.append((t, a[:6], x['hash'], int(x['fee_charged'])))
        url = get(url)['_links']['next']['href'] if False else recs and f"https://horizon-testnet.stellar.org/accounts/{a}/transactions?order=asc&limit=200&cursor={recs[-1]['paging_token']}"
times.sort()
g = [b[0] - a[0] for a, b in zip(times, times[1:])]
s = sorted(g)
q = lambda p: s[min(len(s) - 1, max(0, -(-int(p * len(s) * 1000) // 1000) - 1))] if s else None
out = {'since': since, 'until': until, 'calls': len(times), 'by_account': {k: sum(1 for t in times if t[1] == k) for k in {t[1] for t in times}},
       'span_s': times[-1][0] - times[0][0] if times else 0, 'gaps': len(g), 'min': s[0] if s else None, 'p50': q(0.5), 'p95': q(0.95), 'max': s[-1] if s else None,
       'over20': sum(1 for x in g if x > 20), 'over60': sum(1 for x in g if x > 60), 'fee_total_stroops': sum(t[3] for t in times),
       'hist': {str(k): s.count(k) for k in sorted(set(s))}}
print(json.dumps(out))
