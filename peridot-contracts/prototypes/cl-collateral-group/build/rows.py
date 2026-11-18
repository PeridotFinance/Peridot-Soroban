"""Print measured harness rows (entries = memory reads + disk reads + writes; unique keys separate).
usage: python3 -I rows.py <run-result.json> [sequence-prefix]"""
import json, re, sys
d = json.load(open(sys.argv[1])); pre = sys.argv[2] if len(sys.argv) > 2 else ''
print('| Sequence | Stage | ok | CPU | Mem | Entries (m+d+w) | Keys | Writes | Auth | Error |')
print('|---|---|---|---:|---:|---|---:|---:|---:|---|')
for s in d['extra']['results']:
    if not s['sequence'].startswith(pre): continue
    for r in s['rows']:
        if not r['measured']: continue
        m = re.search(r'Error\((Contract|WasmVm), #?(\w+)\)', json.dumps(r['result'])) if not r['ok'] else None
        print(f"| {s['sequence']} | {r['label']} | {'yes' if r['ok'] else 'no'} | {r['cpu']:,} | {r['mem']:,} | "
              f"**{r['sdk_entries']}** ({r['mem_reads']}+{r['disk_reads']}+{r['writes']}) | {r['unique_keys']} | {r['writes']} | {len(r['auth_required'])} | {m.group(0) if m else ''} |")
