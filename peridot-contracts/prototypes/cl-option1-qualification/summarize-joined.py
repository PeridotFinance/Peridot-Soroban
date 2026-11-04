#!/usr/bin/env python3
"""Summarize exact joined runs; analysis only, never installs an Option1 profile."""
import argparse
import hashlib
import json
from pathlib import Path

ROOTS = {
    'swap-open': ['atomic_qualification_aged_swap_open', 'atomic_qualification_swap_open'],
    'activation': ['atomic_qualification_activate'],
    'prepare-close': ['atomic_qualification_prepare_close'],
    'swap-close': ['atomic_qualification_swap_close'],
    'finish-close': ['atomic_qualification_finish_close'],
    'active funded liquidation': ['combined_liquidation_without_borrower_auth'],
    'full repayment': ['combined_repayment_aged_full_max', 'combined_repayment_after_rejections_full_repay'],
    'debt-free release': ['combined_repayment_debt_free_release', 'combined_repayment_after_rejections_release'],
    'expired PendingOpen': ['option1_pending_independent_full_recovery'],
    'unhealthy PendingOpen': ['option1_pending_preexpiry_unhealthy_funded'],
    'stalled funded settlement (six Long asset states)': [
        'option1_closing_withdrawn_funded','option1_closing_converted_proceeds',
        'option1_liquidated_started_funded','option1_liquidated_converted_proceeds',
        'option1_closing_partial_finish_residual_funded',
        'option1_liquidated_inherited_underlying_successor_funded'],
}
# The existing host recorder names real ABI roots, whereas SDK proof rows name
# assertions. Bind by actual test and ABI, without inventing checker aliases.
STRESS='compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade'
HOST_SELECTORS={
    'swap-open':('swap_open_position_v3',STRESS),
    'activation':('activate_open_position_v3',STRESS),
    'prepare-close':('prepare_close_position_v3',STRESS),
    'swap-close':('swap_close_position_v3',STRESS),
    'finish-close':('finish_close_position_v3',STRESS),
    'active funded liquidation':('liquidate_funded_v3','compiled_combined_funded_liquidation_without_borrower_authorization'),
    'full repayment':('repay_full_margin_position_v3','compiled_combined_repayment_'),
    'debt-free release':('release_debt_free_position_v3','compiled_combined_repayment_'),
    'expired PendingOpen':('recover_expired_pending_funded','compiled_option1_expired_pending_independent_funding_outage_trade_and_rollback'),
    'unhealthy PendingOpen':('liquidate_pending_open_funded','compiled_option1_unhealthy_pending_before_expiry_independent_recovery'),
    'stalled funded settlement (six Long asset states)':('settle_stalled_funded_v3','compiled_option1_closing_and_liquidated_six_states_independent_full_settlement'),
}
BOUNDARIES = {'instructions': 240000000, 'memory_bytes': 26843545,
              'sdk_entries': 240, 'unique_keys': 240, 'write_entries': 80}

def records(p):
    with p.open() as f:
        return [json.loads(line) for line in f if line.strip()]

def sha(p):
    h = hashlib.sha256()
    with p.open('rb') as f:
        for chunk in iter(lambda: f.read(1024*1024), b''):
            h.update(chunk)
    return h.hexdigest()

def maxima(rows, fields):
    return {k: max((r[k] for r in rows), default=None) for k in fields}

def summarize(run, expected_profile):
    driver = json.loads(run.with_name(run.name+'.driver.json').read_text())
    assert driver['seven_cases_completed'], 'Seven actual outcomes required (passing is separate)'
    profile=json.loads((run/'profile.json').read_text())
    assert driver['profile']==profile['name']==expected_profile, 'Recorded profile does not match requested evidence slot'
    build=Path(driver['build']);manifest_file=build/'build-manifest.json'
    assert sha(manifest_file)==driver['source_sha256'][str(manifest_file)]
    manifest=json.loads(manifest_file.read_text())
    for name,pin in manifest['wasm'].items():
        assert sha(build/'artifacts'/name)==pin['sha256']
        assert (build/'artifacts'/name).stat().st_size==pin['bytes']
    original_gates = json.loads((run/'joined-gates.json').read_text())
    reviewed=run/'reviewed-gates.json'
    gate_file=reviewed if reviewed.exists() else run/'joined-gates.json'
    if reviewed.exists():
        review=json.loads((run/'mapping-review.json').read_text())
        assert review['reviewed_gates_sha256']==sha(reviewed)
        assert review['original_gates_sha256']==sha(run/'joined-gates.json')
    gates=json.loads(gate_file.read_text())
    assert gates['profile']==original_gates['profile']==expected_profile
    assert gates['config_sha256']==profile['network_sha256']
    sdk = records(run/'resources.jsonl')
    tx = records(run/'transactions.jsonl')
    calls = records(run/'calls.jsonl')
    assert all(r['profile']==expected_profile and r['config_sha256']==profile['network_sha256'] for r in sdk+tx)
    result = {'run': str(run), 'driver': driver, 'gates': gates,
              'build_manifest_sha256':sha(manifest_file),'build_manifest':manifest,
              'gate_file':str(gate_file),'gate_sha256':sha(gate_file),'original_execution_gates':original_gates,
              'source_sha256': {n: sha(run/n) for n in ['resources.jsonl','transactions.jsonl','calls.jsonl','tests.log','joined-gates.json']},
              'roots': {}}
    for root, labels in ROOTS.items():
        abi,test_fragment=HOST_SELECTORS[root]
        selected=[r for r in sdk if r['flow'] in labels]
        fallback=not selected
        if fallback:
            # SDK finite-limit rejection can happen before the assertion-label
            # row is written. Retain the actual ABI's attempted resource row;
            # do not manufacture a completed proof label or a passing outcome.
            selected=[r for r in sdk if r['flow']==abi]
        s = [dict(r, sdk_entries=r['read_entries']+r['write_entries']+r.get('disk_read_entries',0)) for r in selected]
        t = [r for r in tx if r['flow']==abi and test_fragment in r.get('test_name','')]
        success = [r for r in t if r['success']]
        tm = []
        for r in success:
            footprint = r['footprint']
            tm.append(dict(r['resources'], unique_keys=len(set(footprint['read_only_xdr'])|set(footprint['read_write_xdr']))))
        sm = maxima(s, ['instructions','memory_bytes','sdk_entries','write_entries','write_bytes','contract_events_size_bytes'])
        hm = maxima(tm, ['instructions','memory_bytes','unique_keys','write_entries','write_bytes','transaction_size_bytes','disk_read_bytes','contract_events_size_bytes'])
        actual = {k: max(v for v in [sm.get(k),hm.get(k)] if v is not None) if any(v is not None for v in [sm.get(k),hm.get(k)]) else None for k in BOUNDARIES}
        complete_positive=bool(not fallback and s and all(r['ok'] for r in s) and any(r['limits_pass'] for r in success))
        breakdown = []
        for c in calls:
            if c['flow'] not in labels:
                continue
            roles = {}
            for n in c['calls']:
                role = n['role']
                roles[role] = roles.get(role,0)+n['exclusive']['instructions']
            breakdown.append({'flow':c['flow'], 'exclusive_cpu_by_role':roles,
                              'root_inclusive_cpu':c['calls'][0]['inclusive']['instructions'] if c['calls'] else None})
        result['roots'][root] = {
            'labels': labels, 'host_selector':{'abi':abi,'test_name_contains':test_fragment},
            'sdk_fallback_to_actual_abi_attempts':fallback,
            'sdk_attempts':len(s), 'sdk_accepted':sum(r['ok'] for r in s),
            'host_attempts':len(t), 'host_completed':len(success),
            'host_completed_with_limits_pass':sum(r['limits_pass'] for r in success),
            'sdk_max_attempted':sm, 'host_max_completed':hm,
            'observed_maxima':actual,
            'proposed_workload_headroom':{k: BOUNDARIES[k]-v if complete_positive and v is not None else None for k,v in actual.items()},
            'estimated_fee_stroops_range': [min(r['fees']['total'] for r in success),max(r['fees']['total'] for r in success)] if success else None,
            'call_cpu_breakdown':breakdown,
            'positive_sdk_rows_accepted':bool(not fallback and s and all(r['ok'] for r in s)),
            'host_expected_rejections':sum(r['expected_rejection'] for r in t),
            'host_signature_or_actual_sdk_auth_proof':False}
    return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--captured',required=True,type=Path);p.add_argument('--internal',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args()
    assert not a.output.exists(), 'Preserve prior evidence'
    assert a.captured.resolve()!=a.internal.resolve(), 'Profiles require distinct recorded runs'
    d={'schema':'cl-option1-joined-assessment-v1','scope':'controlled compiled roots and unsigned host transactions; no Core financial application',
       'policy_adopted':False,'release_approval':False,'signed_authorization_measured':False,
       'proposed_workload_boundaries':BOUNDARIES,'profiles':{}}
    for name,run in [('captured-network-diagnostic',a.captured),('internal-policy',a.internal)]:
        d['profiles'][name]=summarize(run,name)
    a.output.write_text(json.dumps(d,indent=2)+'\n')
    text='# Joined candidate measured roots\n\nNo policy adoption, signed authorization, live-state or Core financial-application proof. Maxima use complete unsigned host executions and SDK attempts separately. Missing/rejected roots are explicit; they are not zero-cost passes.\n\n'
    for profile, report in d['profiles'].items():
        text+=f'## {profile}\n\n| Root | SDK accepted/attempted | Host completed/attempted | CPU maximum | Memory maximum | SDK entries | Unique keys | Writes | CPU headroom to240M |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n'
        for root,r in report['roots'].items():
            m=r['observed_maxima'];h=r['proposed_workload_headroom']
            values=[root,f"{r['sdk_accepted']}/{r['sdk_attempts']}",f"{r['host_completed']}/{r['host_attempts']}"]+[str(m[k]) if m[k] is not None else 'MISSING' for k in BOUNDARIES]+[str(h['instructions']) if h['instructions'] is not None else 'UNQUALIFIED']
            text+='| '+' | '.join(values)+' |\n'
        text+='\nExact gates: `'+json.dumps(report['gates']['gates'],sort_keys=True)+'`. Per-flow errors remain in the bound gate report.\n\n'
    text+='CPU breakdown, fee ranges, source hashes and remaining headroom in every proposed dimension are in the sibling JSON. SDK entries are read+write+disk counts, not unique LedgerKey cardinality. Host maxima include every completed recording of the named ABI in its exact test group, including recording-only counterparts of SDK authorization negatives; host success does not prove their authorization. Stalled settlement groups all six asset states conservatively. Diagnostic shadow counters are excluded. Fees are per independent recorded invocation, not a lifecycle sum.\n'
    a.output.with_suffix('.md').write_text(text)

if __name__=='__main__':main()
