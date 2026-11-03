#!/usr/bin/env python3
"""Summarize complete recorded roots; failures never become zero-cost successes."""
import argparse
import hashlib
import importlib.util
import json
from collections import defaultdict
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--run', type=Path, action='append', required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--calls', type=Path, action='append', default=[])
a = p.parse_args()
groups = defaultdict(list)
inputs = {}
for run in a.run:
    file = run / 'journeys.jsonl'
    inputs[str(file)] = hashlib.sha256(file.read_bytes()).hexdigest()
    for line in file.read_text().splitlines():
        row = json.loads(line)
        groups[(row['profile'], row['operation'])].append(row)

results = []
for (profile, operation), rows in sorted(groups.items()):
    host = [r['final_transaction'] for r in rows]
    sdk = [r['sdk_invocation'] for r in rows]
    complete = [r for r in host if r.get('success') is True]
    sdk_complete = [r for r in sdk if r.get('ok') is True]
    def maximum(values):
        return max(values, default=None)
    def span(values):
        values = [v for v in values if v is not None]
        return [min(values), max(values)] if values else None
    batches = [b for r in rows for b in r['preparation']]
    out = {
        'profile': profile, 'operation': operation, 'attempts': len(rows),
        'host_completed': len(complete), 'sdk_completed': len(sdk_complete),
        'combined_limits_fit': sum(r['limits_fit'] is True for r in rows),
        'host_max_completed': {k: maximum(r['resources'][k] for r in complete)
                               for k in ['instructions', 'memory_bytes', 'write_entries',
                                         'disk_read_bytes', 'write_bytes', 'transaction_size_bytes']},
        'sdk_max_attempted': {k: maximum(r[k] for r in sdk) for k in ['instructions', 'memory_bytes', 'write_entries']},
        'sdk_entries_max_attempted': maximum(r['read_entries'] + r['disk_read_entries'] + r['write_entries'] for r in sdk),
        'unique_keys_max_completed': maximum(len(set(r['footprint']['read_only_xdr'] + r['footprint']['read_write_xdr'])) for r in complete),
        'required_archived_keys': span(r['required_archived_keys'] for r in rows),
        'preparation_transactions': span(r['preparation_transactions'] for r in rows),
        'serial_rounds_if_waiting_each': span(r['serial_confirmation_rounds_if_waiting_each'] for r in rows),
        'preparation_fees_stroops': span(r['preparation_fee_total'] for r in rows),
        'preparation_rent_stroops': span(r['preparation_rent_total'] for r in rows),
        'complete_journey_fees_stroops': span(r['journey_fee_total'] for r in rows if r['final_transaction'].get('success')),
        'max_restore_batch': {k: maximum(r['resources'][k] for r in batches)
                              for k in ['write_entries', 'disk_read_bytes', 'write_bytes', 'transaction_size_bytes']},
        'sdk_conservation_passes': sum(r.get('reward_conservation', {}).get('exact_scaled') is True for r in sdk),
        'no_additional_automatic_restoration': all(r.get('restored_rw_entry_indices') == [] for r in complete),
        'core_restore_application_verified': False, 'signed_authorization_measured': False,
        'live_state_validation': False, 'release_approval': False,
    }
    results.append(out)
attribution = []
spec = importlib.util.spec_from_file_location('preserved_attribution', Path(__file__).with_name('cpu-attribution.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
for file in a.calls:
    inputs[str(file)] = hashlib.sha256(file.read_bytes()).hexdigest()
    for i, line in enumerate(file.read_text().splitlines()):
        row = json.loads(line)
        if row['flow'] in ('swap_open_position_v3', 'swap_close_position_v3'):
            attribution.append({'source': str(file),
                'scope': 'combined compiled comparison invocation; separate from post-preparation host/SDK totals',
                **module.summarize(row, i + 1)})
a.output.parent.mkdir(parents=True, exist_ok=True)
a.output.write_text(json.dumps({'schema': 'cl-restoration-assessment-v1', 'inputs': inputs,
    'scope': 'independent controlled-state cold-root probes; not one sequential restored lifecycle',
    'network_latency_measured': False, 'results': results, 'current_call_attribution': attribution}, indent=2) + '\n')
print(a.output)
