"""Bind the six executed combined-candidate tests; never invent missing flows."""
import argparse,hashlib,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--output',type=Path,default=ROOT/'target/cl-option1-proof/reconciled')
OUT=p.parse_args().output.resolve()
assert not OUT.exists()
OUT.mkdir(parents=True)
build=ROOT/'target/cl-restoration-final-v7-build'
manifest=json.loads((build/'build-manifest.json').read_text())
for path,digest in manifest['harness_generated_sources'].items():
 assert hashlib.sha256((build/path).read_bytes()).hexdigest()==digest
runs=[ROOT/'target'/('cl-restoration-final-v7-'+n) for n in ['combined','open-close','liquidations','pending']]
old=json.loads((ROOT/'prototypes/cl-atomic-qualification/proof-map.json').read_text())
old['proofs']=old['proofs'][:3]
old['sources']={test:str(ROOT/'prototypes/cl-atomic-qualification/tests/combined.rs') for proof in old['proofs'] for test in proof['tests']}
new=[('aged_full_max_refund_release_and_beneficiary_claims',['combined_repayment_aged_full_max','combined_repayment_zero_debt_repeat','combined_repayment_debt_free_release','combined_repayment_beneficiary_claim_outage'],'Accrued full repayment, bounded payer maximum/refund, zero debt/repeat, release and retained historical beneficiary claims.'),('insufficient_max_unauthorized_and_closed_rollback',['combined_repayment_after_rejections_full_repay','combined_repayment_after_rejections_release'],'Insufficient maximum and unauthorized/closed-state rejection with ledger rollback, followed by authorized full repayment/release.')]
for name,flows,assertion in new:
 test='test::repayment_qualification::combined_repayment_cases::compiled_combined_repayment_'+name
 old['sources'][test]=str(build/'harness/src/combined-repayment-tests.rs')
 old['proofs'].append(dict(label='combined_'+name,tests=[test],assertions=[assertion],resource_flows=flows))
test='test::repayment_qualification::combined_repayment_cases::compiled_combined_funded_liquidation_without_borrower_authorization'
old['sources'][test]=str(build/'harness/src/combined-repayment-tests.rs')
old['proofs'].append(dict(label='combined_funded_active_liquidation',tests=[test],assertions=['Distinct funded liquidator, actual nested contract authorization, no borrower signature or forced waiver; full repayment/seizure atomic.'],resource_flows=['combined_liquidation_without_borrower_auth']))
(OUT/'proof-map.json').write_text(json.dumps(old,indent=2)+'\n')
# These are the captured comparison invocations used by the six outer tests.
# Internal prepared-root failures stay in their original separate reports.
for leaf in ['resources.jsonl','transactions.jsonl','calls.jsonl']:
 (OUT/leaf).write_bytes(b''.join((r/'checkpoint'/leaf).read_bytes() for r in runs))
profile=runs[0]/'checkpoint/profile.json'
command=['python3','scripts/make_cl_proof_manifest.py','--map',str(OUT/'proof-map.json'),'--profile',str(profile),'--report',str(OUT/'resources.jsonl'),'--call-profile',str(OUT/'calls.jsonl'),'--output',str(OUT/'proof-manifest.json')]
for run in runs:
 code=int((run/'checkpoint/test-exit-code.txt').read_text());assert code==0
 command+=['--test-log',str(run/'checkpoint/tests.log'),'--test-exit-code',str(code)]
subprocess.run(command,cwd=ROOT,check=True)
check=['bash','scripts/check_budget.sh','--local-report',str(OUT/'resources.jsonl'),'--profile',str(profile),'--transaction-evidence',str(OUT/'transactions.jsonl'),'--proof-manifest',str(OUT/'proof-manifest.json'),'--gate','checkpoint','--summary-json',str(OUT/'gates.json')]
with (OUT/'budget.log').open('w') as log: result=subprocess.run(check,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
(OUT/'commands.json').write_text(json.dumps({'bind':command,'check':check,'checker_exit':result.returncode,'release_approval':False},indent=2)+'\n')
print(OUT,result.returncode)
raise SystemExit(result.returncode)
