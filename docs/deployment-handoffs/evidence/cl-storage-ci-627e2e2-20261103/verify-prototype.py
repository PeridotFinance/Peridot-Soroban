import hashlib,json,re,sys,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT.parents[1]/'scripts'))
from cl_proof_evidence import cargo_outcomes
sha=lambda b:hashlib.sha256(b).hexdigest()
out={'scope':'Completed Linux isolated storage prototype CI; no new restoration-preparation or repayment candidate execution','profiles':{},'release_approval':False,'full_replay_performed':False}
for a in json.loads((ROOT/'github-1.json').read_text())['artifacts']:
 prof='internal-policy' if 'internal-policy' in a['name'] else 'captured-network-diagnostic';prefix='cl-storage-ci-'+prof+'/'
 with zipfile.ZipFile(ROOT/f"artifact-{a['id']}.zip") as z:
  read=lambda p:z.read(prefix+p)
  summary=json.loads(read('qualification-summary.json'));assert summary['profile']==prof and summary['release_approval'] is False and summary['final_exit_code']==1 and summary['ci_identity']['GITHUB_RUN_ID']=='41613837952'
  result={'artifact':a,'qualification_summary':summary,'views':{},'builds':{},'metadata_tests_passed':7,'comparison_logs':{}}
  metadata=z.read('cl-storage-ci-bootstrap/metadata-tests.log').decode();assert 'Ran 7 tests' in metadata and '\nOK\n' in metadata
  for n in z.namelist():
   if n.startswith(prefix+'builds/') and n.endswith('build-manifest.json'):
    m=json.loads(z.read(n));base=n.rsplit('/',1)[0];
    for fn,v in m['wasm'].items():
     b=z.read(base+'/artifacts/'+fn);assert sha(b)==v['sha256'] and len(b)==v['bytes']
    for fn,h in m['harness_generated_sources'].items():assert sha(z.read(base+'/'+fn))==h,(base,fn)
    build_log=z.read(base+'/optimization-build.log').decode() if base+'/optimization-build.log' in z.namelist() else ''
    if base.endswith('/candidate-original'):
     assert 'Compiling receipt-vault' in build_log and 'Compiling simple-peridottroller' in build_log
    result['builds'][base]={'manifest_sha256':sha(z.read(n)),'baseline_commit':m['baseline_commit'],'wasm':m['wasm'],'optimized_source_hashes':m.get('optimized_source_hashes'), 'cargo_harness_identity':m.get('cargo_harness_identity'),'explicit_compilation_lines':[line for line in build_log.splitlines() if 'Compiling receipt-vault' in line or 'Compiling simple-peridottroller' in line]}
  for variant,info in summary['variants'].items():
   for view,v in info['views'].items():
    path='runs/'+variant+'-'+view+'/'
    gatefile='checkpoint-gates.json' if view=='cold' else 'gates.json';exitfile='checkpoint-exit-code.txt' if view=='cold' else 'budget-exit-code.txt'
    gates=json.loads(read(path+gatefile));assert gates==v['summary'];assert gates['profile']==prof and gates['selected_gate']=='checkpoint' and gates['release_approval'] is False
    assert int(read(path+exitfile))==v['exit_code']==1 and gates['gates']['checkpoint'] is False
    log=read('commands/'+variant+'-cold-checkpoint.log').decode() if view=='cold' else read(path+'budget.log').decode()
    for g,ok in gates['gates'].items():assert g.upper()+' GATE: '+('PASS' if ok else 'FAIL') in log
    for key in ['missing_required_flows','lifecycle_errors','transaction_errors']:
     for error in gates[key]:assert error in log,(prof,variant,view,error)
    tests=read(path+'tests.log').decode();outcomes=cargo_outcomes(tests);counts={k:sum(v==k for v in outcomes.values()) for k in ['ok','FAILED','ignored']}
    aggregate=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',tests,re.M);assert len(aggregate)==1;assert list(counts.values())==list(map(int,aggregate[0])),(counts,aggregate)
    assert counts['ok']==info['fixture']['passed'] and counts['FAILED']==info['fixture']['failed']
    assert int(read(path+'test-exit-code.txt'))==info['fixture']['exit_code']
    manifest=json.loads(read(path+'proof-manifest.json'));assert manifest['resource_report_sha256']==sha(read(path+'resources.jsonl'))
    assert manifest['call_profile']['sha256']==sha(read(path+'calls.jsonl'))
    for run in manifest['test_runs']:assert run['sha256']==sha(read(path+'tests.log'))
    transactions=[json.loads(line) for line in read(path+'transactions.jsonl').splitlines() if line.strip()]
    unmeasured=[{'row':i,'flow':r.get('flow'),'error':r.get('error'),'success':r.get('success')} for i,r in enumerate(transactions) if 'resources' not in r]
    panics=[]
    for m in re.finditer(r"thread '([^']+)'[^\n]*panicked at[^\n]*\n",tests):
     tail=tests[m.end():];block=tail.split('\nFAILED',1)[0];panics.append({'test':m[1],'location':m[0].strip(),'resource_exhaustion': 'Budget' in block and 'ExceededLimit' in block,'first_lines':block.splitlines()[:4]})
    result['views'][variant+'-'+view]={'gates':gates,'test_counts':counts,'named_test_outcomes':outcomes,'test_exit_code':int(read(path+'test-exit-code.txt')),'transaction_rows':len(transactions),'rows_without_resources':unmeasured,'panic_classification':panics,'resource_report_sha256':manifest['resource_report_sha256'],'test_log_sha256':sha(read(path+'tests.log'))}
  for view,c in summary['comparisons'].items():
   log=read('commands/'+view+'-comparison.log').decode();result['comparison_logs'][view]=log
   if c['business_equal']:
    record=json.loads(read('comparisons/'+view+'.json'));assert record['business_equal'] is True and len(record['rows'])==c['rows']==151
   else:assert 'Error: ' in log
  out['profiles'][prof]=result
(ROOT/'prototype-verified-summary.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps({p:{'errors':s['qualification_summary']['errors'],'views':{n:{'tests':v['test_counts'],'gates':v['gates']['gates'],'error_counts':{k:len(v['gates'][k]) for k in ['missing_required_flows','lifecycle_errors','transaction_errors']},'missing_resources':len(v['rows_without_resources'])} for n,v in s['views'].items()}} for p,s in out['profiles'].items()},indent=2))
