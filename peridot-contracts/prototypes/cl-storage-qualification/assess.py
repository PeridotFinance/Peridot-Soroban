#!/usr/bin/env python3
"""Analysis-only proposal distances; never create a resource policy profile."""
import argparse
import hashlib
import json
from pathlib import Path

BOUNDARIES={'instructions':240_000_000,'memory_bytes':26_843_545,
            'unique_footprint_keys':240,'write_entries':80,'sdk_entry_total':240}
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--run',type=Path,action='append',required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args()
assert not a.output.exists()
results=[]
for directory in a.run:
    raw=(directory/'transactions.jsonl').read_bytes()
    rows=[json.loads(l) for l in raw.splitlines()]
    roots={}
    for r in rows:
        root=roots.setdefault(r['flow'],{'completed_successes':0,'unexpected_failures':0,'expected_rejections':0,'completed_maxima':{},'failures':[]})
        if r.get('expected_rejection'):
            root['expected_rejections']+=1
            continue
        if not r['success'] or 'resources' not in r:
            root['unexpected_failures']+=1
            root['failures'].append({'error':r.get('error'),'resources':r.get('resources'),'quantities_are_partial':True})
            continue
        root['completed_successes']+=1
        values=dict(r['resources'])
        values['unique_footprint_keys']=len(set(r['footprint']['read_only_xdr']+r['footprint']['read_write_xdr']))
        values['restored_entries']=len(r['restored_rw_entry_indices'])
        values['authorization_template_bytes']=r['authorization_size_bytes']
        values.update({'fee_'+k+'_stroops':v for k,v in r['fees'].items()})
        for k,v in values.items():root['completed_maxima'][k]=max(root['completed_maxima'].get(k,0),v)
    for root in roots.values():
        root['distance_to_proposed_workload']={k:boundary-root['completed_maxima'][k] for k,boundary in BOUNDARIES.items() if k in root['completed_maxima']}
        root['completed_unsigned_quantities_fit']=bool(root['completed_successes']) and not root['unexpected_failures'] and all(v>=0 for v in root['distance_to_proposed_workload'].values())
    sdk={}
    for line in (directory/'resources.jsonl').read_text().splitlines():
        r=json.loads(line)
        d=sdk.setdefault(r['flow'],{})
        values={k:r[k] for k in ['instructions','memory_bytes','write_entries']}
        values['sdk_entry_total']=r['read_entries']+r['disk_read_entries']+r['write_entries']
        for k,v in values.items():d[k]=max(d.get(k,0),v)
    results.append({'run':str(directory),'transactions_sha256':hashlib.sha256(raw).hexdigest(),
                    'gates':json.loads((directory/'gates.json').read_text()),'roots':roots,'sdk_invocation_maxima':sdk})
output={'schema':'cl-storage-assessment-v1','proposal_only':True,'policy_changed':False,'release_approval':False,
        'boundaries':BOUNDARIES,'maxima_are_independent_not_one_transaction':True,
        'signature_verification_measured':False,'live_state_validation':False,'runs':results}
a.output.write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps({'runs':len(results),'release_approval':False}))
