"""CL collateral GROUP: summarize run-result.json into results.json and a markdown table (from the spike summarizer).
usage: python3 -I summarize.py <state_dir> <out_dir>"""
import json, sys, hashlib, os, re
WHO = {'c1': 'group controller', 'c2': 'USDC market', 'c5': 'group sleeve', 'd1': 'group controller', 'd2': 'USDC market', 'd5': 'group sleeve'}
CODES = {1: 'NavStale', 2: 'DepegGuard', 3: 'PoolOutOfBand', 4: 'NavRefreshable', 5: 'OracleStale', 6: 'InsufficientCollateral', 7: 'UnitsLocked', 8: 'NavInvalid', 9: 'NoPending', 10: 'FallbackZero'}
def cause(diag):
    for d in diag:
        m = re.search(r'Error\(Contract\((\d+)\)\)', d)
        if m and ('c5c5c5' in d or 'd5d5d5' in d): return 'sleeve error #%s %s' % (m.group(1), CODES.get(int(m.group(1)), ''))
    for d in diag:
        m = re.search(r'ContractId\(Hash\(([0-9a-f]{2})', d)
        if m and 'trapped' in d: return 'trap in ' + WHO.get(m.group(1), m.group(1))
    return ''
sd, od = sys.argv[1], sys.argv[2]
rr = json.load(open(os.path.join(sd, 'run-result.json'))); st = json.load(open(os.path.join(sd, 'state.json')))
LIM = dict(cpu=100_000_000, mem=41_943_040, entries=100, keys=100, writes=50)
TARGET = 90  # requested headroom target (entries, cold)
def verdict(r):
    f = []
    if r['cpu'] > LIM['cpu']: f.append('CPU')
    if r['mem'] > LIM['mem']: f.append('MEM')
    if r['sdk_entries'] > LIM['entries']: f.append('ENTRIES')
    if r['unique_keys'] > LIM['keys']: f.append('KEYS')
    if r['writes'] > LIM['writes']: f.append('WRITES')
    s = 'FIT' if not f else 'OVER:' + '+'.join(f)
    if r['cpu'] >= 97_000_000 and r['cpu'] <= LIM['cpu']: s += ' KEEPER-STOP(97M)'
    if not f and r['sdk_entries'] > TARGET: s += ' >90'
    return s
rows = []
seqs = rr['extra'].get('results', [])
for s in seqs:
    for r in s['rows']:
        if not r['measured']: continue
        rows.append({'sequence': s['sequence'], 'label': r['label'], 'ok': r['ok'], 'cpu': r['cpu'], 'cpu_host_default_model': r['cpu_host_default_model'], 'cpu_live_adj': r['cpu_live_model_adjustment'], 'cpu_adj_detail': r['cpu_adjustment_detail'], 'mem': r['mem'], 'mem_reads': r['mem_reads'], 'disk_reads': r['disk_reads'],
                     'writes': r['writes'], 'sdk_entries': r['sdk_entries'], 'unique_keys': r['unique_keys'], 'pool_touched': r['pool_touched'], 'archived': r['archived_keys_touched'],
                     'disk_read_bytes': r['disk_read_bytes'], 'write_bytes': r['write_bytes'], 'keys_by_owner': r['keys_by_owner'], 'expect_fail': r.get('expect_fail', False), 'dt_extra': r.get('dt_extra', 0), 'verdict': ('REVERTED-AS-EXPECTED' if r.get('expect_fail') and not r['ok'] else 'UNEXPECTED-SUCCESS' if r.get('expect_fail') else verdict(r) if r['ok'] else 'FAILED'),
                     'auth_required': r.get('auth_required', []), 'revert_cause': '' if r['ok'] else cause(r['diagnostics_if_failed']), 'error': None if r['ok'] else (r['result'] if isinstance(r['result'], str) else json.dumps(r['result']))[:300], 'diag': r['diagnostics_if_failed'][:4] if not r['ok'] else []})
setup = []
for item in rr['log']:
    if isinstance(item, dict) and 'rows' in item:
        for r in item['rows']:
            setup.append({'sequence': item['sequence'], 'label': r['label'], 'ok': r['ok'], 'cpu': r['cpu'], 'mem': r['mem'], 'sdk_entries': r['sdk_entries'], 'mem_reads': r['mem_reads'], 'disk_reads': r['disk_reads'], 'writes': r['writes'], 'unique_keys': r['unique_keys'], 'pool_touched': r['pool_touched']})
other = [x for x in rr['log'] if not (isinstance(x, dict) and 'rows' in x)]
out = {'schema': 'cl-collateral-group-results-v1', 'evidence_class': 'LOCAL HOST-ONLY soroban-env-host 29.0.1 testutils, mock_all_auths; NOT Core, NOT signed',
       'state': {'ledger': st['ledger'], 'closeTime': st['closeTime'], 'protocol': st['protocolVersion'], 'entries': len(st['entries']), 'absent': len(st['absent']), 'refreshLedgers': st.get('refreshLedgers'),
                 'state_sha256': hashlib.sha256(open(os.path.join(sd, 'state.json'), 'rb').read()).hexdigest()},
       'network_limits_live': st.get('networkLimits'), 'unknown_keys': rr['unknown_keys'], 'setup_info': other, 'setup_rows': setup, 'measured_rows': rows}
os.makedirs(od, exist_ok=True)
json.dump(out, open(os.path.join(od, 'results.json'), 'w'), indent=1)
print('| # | sequence | row | CPU (live model) | mem bytes | entries (mem+disk+writes) | unique keys | writes | pool | auth | verdict |')
print('|---|---|---|---:|---:|---:|---:|---:|---|---|---|')
for i, r in enumerate(rows, 1):
    err = (' — ' + r['revert_cause']) if r['verdict'] in ('REVERTED-AS-EXPECTED', 'FAILED') else ''
    print(f"| {i} | {r['sequence']} | `{r['label']}`{' (+%ds)' % r['dt_extra'] if r['dt_extra'] else ''} | {r['cpu']:,} | {r['mem']:,} | {r['sdk_entries']} ({r['mem_reads']}+{r['disk_reads']}+{r['writes']}) | {r['unique_keys']} | {r['writes']} | {'Y' if r['pool_touched'] else 'n'} | {len(r['auth_required'])} | {r['verdict']}{err} |")
print('unknown', rr['unknown_keys'])
