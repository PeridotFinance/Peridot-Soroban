#!/usr/bin/env python3
"""Run both immutable resource profiles; preserve and propagate every failed gate."""
import json
from pathlib import Path
import subprocess
import sys

PROFILES=('internal-policy','captured-network-diagnostic')
ROOT=Path(__file__).resolve().parents[2]

def completed_status(summary):
    return bool(summary) and set(summary)==set(PROFILES) and all(
        set(value)=={'financial','preparation'} and all(
            result.get('exit_code')==0 and result.get('selected_gate_passed') is True
            for result in value.values()) for value in summary.values())


def main(run):
    run=run.resolve();summary={};errors=[]
    try:
        selected={}
        for source,dest in [('preparation.jsonl','preparation.jsonl'),('journeys.jsonl','journeys.jsonl'),('post-resources.jsonl','resources.jsonl'),('post-transactions.jsonl','transactions.jsonl')]:
            rows=[json.loads(line) for line in (run/source).read_text().splitlines() if line.strip()]
            if not rows or any(r.get('profile') not in PROFILES for r in rows):raise ValueError('missing records or unexpected profile: '+source)
            selected[dest]=rows
        build=Path(json.loads((run/'checkpoint/run-configuration.json').read_text())['build'])
        for profile in PROFILES:
            out=run/profile;out.mkdir(exist_ok=False)
            for dest,rows in selected.items():
                (out/dest).write_text(''.join(json.dumps(r)+'\n' for r in rows if r['profile']==profile))
            commands={
                'financial':['bash','scripts/check_budget.sh','--local-report',str(out/'resources.jsonl'),'--profile',str(run/'profiles'/f'{profile}.json'),'--transaction-evidence',str(out/'transactions.jsonl'),'--gate','checkpoint','--summary-json',str(out/'gates.json')],
                'preparation':['python3','prototypes/cl-restoration-preparation/restore-checker.py','--input',str(out/'preparation.jsonl'),'--journeys-input',str(out/'journeys.jsonl'),'--profile',str(run/'profiles'/f'{profile}.json'),'--network-config',str(run/'network-config.json'),'--output',str(out/'preparation-gates.json'),'--producer-file',str(build/'harness/src/restoration.rs')]}
            summary[profile]={}
            for label,cmd in commands.items():
                with (out/f'{label}.log').open('w') as log:result=subprocess.run(cmd,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
                entry={'command':cmd,'exit_code':result.returncode,'selected_gate_passed':False};summary[profile][label]=entry
                path=out/('gates.json' if label=='financial' else 'preparation-gates.json')
                try:
                    gates=json.loads(path.read_text());entry['gate_summary']=gates
                    if gates.get('profile')!=profile or gates.get('release_approval') is not False:raise ValueError('invalid profile/approval in gate report')
                    entry['selected_gate_passed']=gates['gates']['checkpoint' if label=='financial' else 'release'] is True
                    if result.returncode==0 and not entry['selected_gate_passed']:errors.append(profile+'/'+label+': exit0 contradicts failed selected gate')
                except (OSError,ValueError,KeyError,TypeError) as error:errors.append(profile+'/'+label+': '+str(error))
    except (OSError,ValueError,KeyError,TypeError) as error:errors.append(str(error))
    passed=not errors and completed_status(summary)
    report={'release_approval':False,'profiles':summary,'verification_errors':errors,'qualification_passed':passed,'exit_code':0 if passed else 1}
    (run/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
    return report['exit_code']

if __name__=='__main__':raise SystemExit(main(Path(sys.argv[1])))
