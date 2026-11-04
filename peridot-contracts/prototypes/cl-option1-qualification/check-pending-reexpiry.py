#!/usr/bin/env python3
"""Validate explicitly synthetic re-expiry batch evidence without Core claims."""
import argparse, hashlib, importlib.util, json
from pathlib import Path
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('restore_checker',HERE.parent/'cl-restoration-preparation/restore-checker.py')
checker=importlib.util.module_from_spec(spec);spec.loader.exec_module(checker)
p=argparse.ArgumentParser();p.add_argument('--run',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--gate',choices=['synthetic-projection','qualification'],default='qualification');a=p.parse_args()
run=a.run.resolve();errors=[];results={}
def rows(path):
    data=[json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if not data:raise ValueError('missing required rows: '+str(path))
    return data
try:
    batches=rows(run.with_name(run.name+'-reexpiry-batches.jsonl'))
    probes=rows(run.with_name(run.name+'-reexpiry.jsonl'))
    journeys=rows(run/'journeys.jsonl')
    config=json.loads((run/'network-config.json').read_text())
    build=Path(json.loads((run/'checkpoint/run-configuration.json').read_text())['build'])
    producer=hashlib.sha256((build/'harness/src/restoration.rs').read_bytes()).hexdigest()
    if not(len(batches)==len(probes)==len(journeys)):errors.append('extra-batch/probe/main-journey counts differ')
    for name in ['internal-policy','captured-network-diagnostic']:
        profile=checker.create(run/'network-config.json',name)
        bs=[r for r in batches if r.get('profile')==name];ps=[r for r in probes if r.get('profile')==name];js=[r for r in journeys if r.get('profile')==name]
        failures=[]
        if not bs or not ps or not js:failures.append('missing required profile rows')
        if not(len(bs)==len(ps)==len(js)):failures.append('profile batch/probe/journey counts differ')
        for i,(batch,probe,journey) in enumerate(zip(bs,ps,js)):
            failures.extend(f'{i}: '+e for e in checker.validate(batch,profile,config,producer_sha256=producer))
            checks={
                'schema':probe.get('schema')=='cl-pending-synthetic-reexpiry-v1',
                'exact extra batch':probe.get('repeated_batch')==batch,
                'exact attached probe':journey.get('synthetic_reexpiry_probe')==probe,
                'producer':probe.get('source_sha256')==producer,
                'root':probe.get('flow')==journey.get('operation')==batch.get('flow'),
                'new pending ABI':probe.get('flow') in ['recover_expired_pending_funded','liquidate_pending_open_funded'],
                'prepared state restored':probe.get('prepared_snapshot_sha256')==probe.get('reprepared_snapshot_sha256'),
                'actual synthetic expiry changed snapshot':probe.get('expired_snapshot_sha256')!=probe.get('prepared_snapshot_sha256'),
                'batch input':batch.get('snapshot_before_sha256')==probe.get('expired_snapshot_sha256'),
                'batch output':batch.get('snapshot_after_sha256')==probe.get('reprepared_snapshot_sha256'),
                'interruption':probe.get('serialized_interruption_before_reprepare') is True,
                'logical data':probe.get('financial_data_unchanged') is True,
                'no time inference':probe.get('fixture_ledger_time_advance')==0 and probe.get('real_reexpiry_verified') is False,
                'no Core inference':probe.get('core_application_verified') is False and probe.get('release_approval') is False,
                'extra cost':probe.get('extra_fee')==batch.get('fees',{}).get('total'),
                'extra count':probe.get('extra_transaction_count')==1,
                'total count':journey.get('transaction_count_with_synthetic_reexpiry_probe')==journey.get('transaction_count',-100)+1,
                'total fee':journey.get('journey_fee_total_with_synthetic_reexpiry_probe')==(journey['journey_fee_total']+probe['extra_fee'] if journey.get('journey_fee_total') is not None else None),
                'prefix slot':probe.get('prepared_snapshot_sha256')==journey.get('preparation',[{}])[0].get('snapshot_after_sha256'),
                'same retry keys':[x.get('key_xdr') for x in batch.get('keys',[])]==[x.get('key_xdr') for x in journey.get('preparation',[{}])[0].get('keys',[])],
            }
            failures.extend(f'{i}: '+label for label,okay in checks.items() if not okay)
        results[name]={'synthetic_batch_and_link_checks_passed':not failures,'errors':failures,'rows':len(ps),'financial_execution_limits_fit':bool(js) and all(j.get('limits_fit') is True for j in js),'extra_fee_total_across_independent_probes':sum(p.get('extra_fee',0) for p in ps)}
except (OSError,ValueError,KeyError,TypeError,IndexError) as error:errors.append(str(error))
report={'schema':'cl-pending-synthetic-reexpiry-report-v1','errors':errors,'profiles':results,'synthetic_projection_checks_passed':not errors and bool(results) and all(r['synthetic_batch_and_link_checks_passed'] for r in results.values()),'real_elapsed_time_reexpiry_verified':False,'Core_application_verified':False,'live_state_validation':False,'release_approval':False,'scope':'explicit TTL fault and official native restore simulation; complete recorded fee plus separate extra retry fee; no actual passage of time, signatures or Core transaction application'}
report['selected_gate']=a.gate
report['selected_gate_passed']=a.gate=='synthetic-projection' and report['synthetic_projection_checks_passed']
a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report));raise SystemExit(0 if report['selected_gate_passed'] else 1)
