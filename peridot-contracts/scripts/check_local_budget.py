"""Check every measured flow; missing coverage is a failure, never a pass."""
import json
import sys
from pathlib import Path
from cl_fee_estimate import estimate

REQUIRED = {
    'deposit', 'redeem', 'transfer', 'emergency_redeem', 'claim_rewards',
    'begin_open', 'swap_open_position_v3',
    'activate_open_position_v3', 'prepare_close_position_v3',
    'swap_close', 'finish_close', 'funded_liquidation', 'repay_release',
    'claim_failure_recovery', 'cancel_before_execute', 'cancel_after_execute',
    'cancel_after_swap',
}
path = Path(sys.argv[1])
rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()] if path.exists() else []
failed = False
for r in rows:
    entries = r['read_entries'] + r['write_entries'] + r.get('disk_read_entries', 0)
    ok = (r['ok'] and entries <= 100 and r['write_entries'] <= 50
          and r['instructions'] <= 100_000_000 and r['memory_bytes'] <= 40 * 1024 * 1024
          and r['read_bytes'] <= 200_000 and r['write_bytes'] <= 132_096)
    print(f"{r['flow']}: {'PASS' if ok else 'FAIL'} cpu={r['instructions']} memory={r['memory_bytes']} entries={entries} writes={r['write_entries']} read_bytes={r['read_bytes']} write_bytes={r['write_bytes']}")
    try:
        fee = estimate(r, path.parent / 'network-config.json')
        print('  fee_estimate=' + json.dumps(fee, sort_keys=True))
    except (KeyError, FileNotFoundError) as error:
        print(f'  FEE UNMEASURED: {error}')
        ok = False
    failed |= not ok
missing = REQUIRED - {r['flow'] for r in rows}
if missing:
    print('UNMEASURED: ' + ', '.join(sorted(missing)))
    failed = True
print('Scope: per-row fixture classification; host25 modeled resources at captured protocol29 fee rates. No Mainnet-state transaction simulation. Full-fee envelope/history/base/return bytes remain unmeasured.')
sys.exit(1 if failed else 0)
