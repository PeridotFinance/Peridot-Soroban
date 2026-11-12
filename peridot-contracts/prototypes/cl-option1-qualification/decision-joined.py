#!/usr/bin/env python3
"""One decision table for the measured joined candidate, never an active profile."""
import argparse
import hashlib
import json
from pathlib import Path

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
BOUNDARIES={'instructions':240000000,'memory_bytes':26843545,'sdk_entries':240,'unique_keys':240,'write_entries':80}
REQUIRED_ROOTS={'swap-open','activation','prepare-close','swap-close','finish-close','active funded liquidation','full repayment','debt-free release','expired PendingOpen','unhealthy PendingOpen','stalled funded settlement (six Long asset states)'}
def validate(report):
    assert report['proposed_workload_boundaries']==BOUNDARIES,'Unchanged proposed boundaries required'
    measured=report['profiles']['captured-network-diagnostic']
    assert measured['driver']['profile']==measured['gates']['profile']=='captured-network-diagnostic'
    assert measured['driver']['test_counts']==[{'passed':7,'failed':0}]
    roots=measured['roots']
    assert set(roots)==REQUIRED_ROOTS,'Required measured root missing or unreviewed root set'
    assert all(r['positive_sdk_rows_accepted'] and r['host_completed'] for r in roots.values())
    maxima={k:max(r['observed_maxima'][k] for r in roots.values()) for k in BOUNDARIES}
    assert all(maxima[k]<=v for k,v in BOUNDARIES.items()),'Observed workload exceeds proposal; optimization/redesign decision required, no fit recommendation generated'
    manifest=measured['build_manifest']
    patches=[ROOT/'prototypes/cl-atomic-qualification/optimization.patch',ROOT/'prototypes/cl-storage-qualification/storage.patch',ROOT/'prototypes/cl-storage-qualification/repayment.patch']+[HERE/n for n in ['pending-recovery.patch','pending-unhealthy.patch','closing-recovery.patch']]
    assert manifest['optimization_sha256']==hashlib.sha256(b''.join(p.read_bytes() for p in patches)).hexdigest()
    assert manifest['margin_build_feature']=='funded-liquidation-validation' and manifest['release_approval'] is False
    internal=report['profiles']['internal-policy']
    assert internal['driver']['profile']==internal['gates']['profile']=='internal-policy'
    assert internal['build_manifest']['wasm']==manifest['wasm'],'Comparison must use identical artifacts'
    return measured,maxima,manifest

def load_mixed(manifest):
    path=HERE/'mixed-results.json';mixed=json.loads(path.read_text())
    assert mixed['wasm']==manifest['wasm'],'Mixed evidence is a different candidate'
    build=ROOT/mixed['build'];raw=(build/'build-manifest.json').read_bytes()
    assert hashlib.sha256(raw).hexdigest()==mixed['build_manifest_sha256']
    pinned=json.loads(raw)
    assert hashlib.sha256((build/'harness/src/mixed-funding-tests.rs').read_bytes()).hexdigest()==pinned['mixed_test_sha256']
    metrics=[]
    for label,profile in [('captured','captured-network-diagnostic'),('internal','internal-policy')]:
        run=mixed['runs'][label];directory=ROOT/run['directory']
        assert run['test_exit']==0 and run['gates']['invocation'] and '1 passed; 0 failed;' in run['summary']
        assert json.loads((directory/'profile.json').read_text())['name']==profile
        for leaf,expected in run['evidence_sha256'].items():assert hashlib.sha256((directory/leaf).read_bytes()).hexdigest()==expected
        rows=[json.loads(x) for x in (directory/'resources.jsonl').read_text().splitlines()]
        positive=[r for r in rows if r['flow']=='option1_mixed_own_proceeds_external_full_repayment']
        assert len(positive)==1 and positive[0]['ok'] and positive[0]['profile']==profile
        sdk=positive[0]
        txs=[json.loads(x) for x in (directory/'transactions.jsonl').read_text().splitlines()]
        success=[r for r in txs if r['flow']=='settle_stalled_funded_v3' and r['success'] and r['resources']==run['positive_unsigned_host']['resources']]
        assert len(success)==1 and success[0]['limits_pass'] and success[0]['profile']==profile
        host=success[0];f=host['footprint']
        metrics.append({'instructions':max(sdk['instructions'],host['resources']['instructions']),
            'memory_bytes':max(sdk['memory_bytes'],host['resources']['memory_bytes']),
            'sdk_entries':sdk['read_entries']+sdk['write_entries']+sdk['disk_read_entries'],
            'unique_keys':len(set(f['read_only_xdr'])|set(f['read_write_xdr'])),
            'write_entries':max(sdk['write_entries'],host['resources']['write_entries'])})
    peak={k:max(r[k] for r in metrics) for k in BOUNDARIES}
    assert all(peak[k]<=v for k,v in BOUNDARIES.items())
    return mixed,peak,hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser();p.add_argument('--assessment',required=True,type=Path);a=p.parse_args()
    report=json.loads(a.assessment.read_text())
    measured,maxima,manifest=validate(report)
    mixed,mixed_metrics,mixed_sha=load_mixed(manifest)
    assert all(mixed_metrics[k]<=maxima[k] for k in BOUNDARIES),'Companion changes global maxima; revise table, do not hide it'
    settings={'instructions':(100000000,300000000,240000000,400000000),
              'memory_bytes':(41943040,33554432,26843545,41943040),
              'sdk_entries':(100,300,240,None),
              'unique_keys':(100,300,240,400),'write_entries':(50,100,80,200)}
    text='''# Resource decision — joined atomic candidate, policy unadopted

**Continue bounded Option1 qualification; do not adopt it or enable borrowing.**
The complete measured roots fit its proposed workload bounds with useful unsigned
headroom. Core restoration, full financial application, supported signatures,
live workload bounds, oracle availability and economic funding remain separate
decisions. The unchanged100M policy requires a venue/accounting ABI redesign;
further wrapper tuning or a new financial-stage split has no demonstrated path.

This table uses the corrected joined V3 (`@@V3_IDENTITY@@`) and its exact
seven-case joined run. It does not relabel old independently cold probes as
restored executions of every new route. Maxima across roots are not one combined
worst-case transaction. SDK counts, actual unique keys and network settings are
different quantities.

| Dimension | Internal, unchanged | Option1 proposed ceiling | Option1 workload boundary | Joined measured maximum | Remaining workload headroom | Captured network ceiling |
|---|---:|---:|---:|---:|---:|---:|
'''
    for k,(internal,ceiling,boundary,network) in settings.items():
        text+='| '+' | '.join([k,f'{internal:,}',f'{ceiling:,}',f'{boundary:,}',f'{maxima[k]:,}',f'{boundary-maxima[k]:,}',f'{network:,}' if network is not None else 'No SDK-total field'])+' |\n'
    text+='''
| Option | Concrete implementation dependency | Evidence still missing | Decision |
|---|---|---|---|
| Keep100M/100SDK/50writes (and100 unique keys) | Replace or revise the venue's fee-inclusive valuation/liquidity ABI while preserving actual claims and fee ownership. Current Aquarius-exclusive open162.817M and close137.760M already exceed100M. Fee collection plus position deposit118.186M / withdrawal115.816M also exceeds100M. Reduce actual receipt/controller/custody touches; another wrapper does not reset a budget. | A controlled, reviewable cheaper venue implementation; supported supplier mint/redeem and complete open/close/recovery; entitlement correctness; migration/dependency control; full signed costs. No assumed savings or new split is credited. | Concrete venue/accounting redesign required if this policy remains mandatory. Stop incremental wrapper tuning. |
| Option1 proposal only | Keep present financial stages, all three claim-first boundaries, fresh valuation, reserves and atomic funded repayment/seizure. Use measured native restoration preparation, bounded new-risk admission and funded recovery independent of borrower/governance attribution. | Actual Core financial lifecycles, real account signatures/nonce/envelope costs, live archive discovery, tick/registry/history capacity, elapsed re-expiry, remaining mixed-asset/delivery/reentry and fee branches, funded/incentivized operators. The largest CPU case leaves about@@CPU_ROOM_PCT@@% of240M before unmeasured costs; the20% ceiling reserve is not available workload. | Technically worth a bounded next qualification, economically conditional. No policy adoption until those gates and a funded operating model are established. |

The old checker first introduced100M/100/50 at2d92318 without a documented
quantitative rationale.100M matches host25's default;100/50 match SDK25.3.1's
entry/write defaults, whose CPU default was600M. A copied-default origin is an
inference, not established intent. The later named internal profile deliberately
keeps a conservative gate. Full provenance is in handoff12; no gate is rewritten.

Captured settings are ledger65270655, protocol29, hash
`4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`:
all86 CPU and86 memory terms are used directly in host29.0.1. This is not a fresh
live-state observation. Native RestoreFootprint has separate applicable checks;
zero declared instructions is not zero measured Core CPU or memory. Disk/read,
write-byte, event/return, envelope, key/data/code-size, fee and missing-flow checks
all remain in force. Signatures and account application are not in these host
recording templates. SDK host-shadow counters do not replace acceptance limits.

## Supplier entry and exit remain required

Preserved atomic deposit/redeem evidence is a separate population; the joined V3
recovery run does not rerun these roots. Their unchanged internal failures remain.

| Root | CPU | Memory bytes | SDK entries | Unique keys | Writes | CPU room to240M |
|---|---:|---:|---:|---:|---:|---:|
'''
    supplier=json.loads((HERE/'decision-table.json').read_text())['preserved_supplier_maxima']
    for name,r in supplier.items():
        text+='| '+' | '.join([name]+[f'{r[k]:,}' for k in ('instructions','memory_bytes','sdk_entries','unique_keys','writes')]+[f'{240000000-r["instructions"]:,}'])+' |\n'
    text+='''
Warm and cold costs must be assessed against actual collected income. The shared
eight-borrower recovery projection is613.5261428XLM, including60.6771448XLM
synthetic-authorizer rent; this is not eight independent cold probes. Operating
balance, supplier cash, dedicated loss reserve and debt-token liquidation capital
are separate. A positive resource margin alone does not establish profitability,
liquidator willingness, reserve coverage or a safe exposure cap.

A separate same-bytecode companion passes both profiles for own proceeds plus
external deficit in one converted Closing settlement, including@@RESERVED@@ previously
reserved fee shares and backing. Its@@MIXED_METRICS@@ leave only@@INTERNAL_ROOM@@ SDK entries
under today's internal policy before missing real-auth costs. It does not change
these global maxima or qualify every other asset shape. See `mixed-results.md`.

See `operating-plan.md`, `lifecycle-proof-map.md`, `core-research.md` and the exact
assessment for evidence levels and remaining failures. Both global checkpoint
gates remain failed. No boundary is increased to turn a failure green.
'''
    v3=manifest['wasm']['margin_controller.wasm']
    text=text.replace('@@V3_IDENTITY@@',f'{v3["bytes"]:,} bytes, SHA256 {v3["sha256"]}')
    text=text.replace('@@CPU_ROOM_PCT@@',f'{100*(240000000-maxima["instructions"])/240000000:.2f}')
    text=text.replace('@@RESERVED@@',f'{mixed["amounts_raw"]["reserved_fee_ptokens"]:,}')
    text=text.replace('@@MIXED_METRICS@@',f'{mixed_metrics["instructions"]:,} instructions/{mixed_metrics["memory_bytes"]:,} memory bytes/{mixed_metrics["sdk_entries"]} SDK entries/{mixed_metrics["unique_keys"]} unique host keys/{mixed_metrics["write_entries"]} writes')
    text=text.replace('@@INTERNAL_ROOM@@',f' {100-mixed_metrics["sdk_entries"]}')
    (HERE/'decision-joined.md').write_text(text)
    (HERE/'decision-joined.json').write_text(json.dumps({'assessment_sha256':hashlib.sha256(a.assessment.read_bytes()).hexdigest(),'joined_maxima':maxima,'settings':settings,'supplier_separate_population':supplier,'mixed_evidence_sha256':mixed_sha,'mixed_maxima':mixed_metrics,'wasm':manifest['wasm'],'policy_adopted':False,'release_approval':False},indent=2)+'\n')

if __name__=='__main__':main()
