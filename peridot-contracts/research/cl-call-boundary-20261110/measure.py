"""Reconcile report21's preserved call trees. Offline analysis; no new execution."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=Path(__file__).parent
ARCHIVE=ROOT.parent/'docs/deployment-handoffs/evidence/cl-restoration-preparation-20261103'
INDEX=json.loads((ARCHIVE/'index.json').read_text())
PREFIX='runs/cl-restoration-final-v7-open-close/checkpoint/'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
archive=ARCHIVE/INDEX['archive']['file']
assert archive.stat().st_size==INDEX['archive']['bytes'] and sha(archive)==INDEX['archive']['sha256']
# Read only pinned public evidence members; no extraction or target/wallet traversal.
import tarfile
with tarfile.open(archive,'r:xz') as tar:
    def blob(name):
        pin=INDEX['members'][name];entry=tar.getmember(name);assert entry.isfile()
        data=tar.extractfile(entry).read()
        assert len(data)==pin['bytes'] and hashlib.sha256(data).hexdigest()==pin['sha256'],name
        return data
    calls=blob(PREFIX+'calls.jsonl');profile=blob(PREFIX+'profile.json')
    sources=json.loads(blob(PREFIX+'source-before.json'))
    manifest=blob('build/build-manifest.json');build=json.loads(manifest)
    recorded_manifest=[v for k,v in sources.items() if k.endswith('/build-manifest.json')]
    assert recorded_manifest==[hashlib.sha256(manifest).hexdigest()]
    artifacts={}
    for name,pin in build['wasm'].items():
        data=blob('build/artifacts/'+name)
        assert hashlib.sha256(data).hexdigest()==pin['sha256'] and len(data)==pin['bytes']
        artifacts[name]=pin
    venue=[v for k,v in sources.items() if k.endswith('aquarius-pool.wasm')]
    assert venue==['12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6']
    artifacts['aquarius-pool.wasm']={'sha256':venue[0],'identity_source':'archived source-before manifest; bytecode not rerun'}
    config=blob(PREFIX+'network-config.json')
p=json.loads(profile);assert p['name']=='captured-network-diagnostic' and p['mode']=='acceptance'
assert hashlib.sha256(config).hexdigest()==p['network_sha256']=='4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a'
flows={'atomic_qualification_aged_swap_open':'open_with_actual_deposit','atomic_qualification_swap_open':'later_open_without_deposit','atomic_qualification_swap_close':'close'}
results={};all_venue=[]
for row in map(json.loads,calls.decode().splitlines()):
    assert row['invocation_ok'] and row['profile']==p['name'] and row['runtime_protocol']==29 and row['run_mode']=='acceptance'
    nodes=row['calls'];paths={n['path']:n for n in nodes};assert len(paths)==len(nodes)
    for n in nodes:
        children=[c for c in nodes if c['path'].rsplit('.',1)[0]==n['path'] and c['path']!=n['path']]
        assert len(children)==n['direct_child_count']
        assert n['inclusive']['instructions']-sum(c['inclusive']['instructions'] for c in children)==n['exclusive']['instructions']
    assert sum(n['exclusive']['instructions']for n in nodes)==row['root_instructions']==nodes[0]['inclusive']['instructions']
    venue=[dict(path=n['path'],function=n['function'],inclusive_cpu=n['inclusive']['instructions'],exclusive_cpu=n['exclusive']['instructions'])for n in nodes if n['role']=='aquarius']
    # Disjoint venue roots permit their inclusive sum; no nested Aquarius double count.
    assert all(not b['path'].startswith(a['path']+'.') for a in venue for b in venue if a!=b)
    all_venue += [dict(flow=row['flow'],**n)for n in venue]
    if row['flow'] not in flows:continue
    assert row['flow']not in results
    results[row['flow']]={'case':flows[row['flow']],'root_cpu':row['root_instructions'],'aquarius_exclusive_cpu':sum(n['exclusive_cpu']for n in venue),'aquarius_inclusive_cpu':sum(n['inclusive_cpu']for n in venue),'calls':venue,'largest_call':max(venue,key=lambda n:n['inclusive_cpu'])}
assert len(results)==3
assert results['atomic_qualification_aged_swap_open']['aquarius_exclusive_cpu']==162817228
assert results['atomic_qualification_swap_close']['aquarius_exclusive_cpu']==137759524
maximum=max(all_venue,key=lambda n:n['inclusive_cpu'])
result={'schema':'cl-report21-individual-venue-calls-v1','compiled_artifacts':artifacts,'new_contract_execution':False,'source_calls_sha256':hashlib.sha256(calls).hexdigest(),'source_profile_sha256':hashlib.sha256(profile).hexdigest(),'archive_index_sha256':sha(ARCHIVE/'index.json'),'config_sha256':hashlib.sha256(config).hexdigest(),'source_archive_sha256':sha(archive),'runtime_protocol':29,'selected_profile':p,'comparison_cpu_limit':100000000,'all_captured_rows_maximum_venue_call':maximum,'all_observed_venue_calls_below_100m':all(n['inclusive_cpu']<100000000 for n in all_venue),'observed_venue_call_count':len(all_venue),'cases':results,'scope':'SDK call-boundary attribution only; no per-call official-host tree, no signed or live-state qualification; original full transactions still fail internal limits'}
(OUT/'measurement.json').write_text(json.dumps(result,indent=2)+'\n')
text='# Report21 individual Aquarius call measurement\n\nOffline re-analysis of the original captured protocol29 trees; no new harness or contract execution. Complete call cost is inclusive of token/plane descendants. Exclusive totals allocate CPU by executing contract and must not be added to inclusive totals.\n\n'
for flow,r in results.items():
 text+='## '+r['case']+'\n\n| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |\n|---|---|---:|---:|---|\n'
 for n in r['calls']:text+=f"| {n['path']} | {n['function']} | {n['inclusive_cpu']:,} | {n['exclusive_cpu']:,} | {'YES' if n['inclusive_cpu']<100000000 else 'NO'} |\n"
 text+=f"\nExclusive venue total **{r['aquarius_exclusive_cpu']:,}**, disjoint inclusive total **{r['aquarius_inclusive_cpu']:,}**, complete SDK root **{r['root_cpu']:,}**.\n\n"
text+='Every captured node reconciles inclusive minus direct children to exclusive, and all exclusive nodes to its root. Input hashes match the durable report21 archive. The later open never calls deposit_position and is not a full deposit feasibility proof.\n\nLargest observed invocation across this captured lifecycle: **'+maximum['function']+' '+str(maximum['inclusive_cpu'])+' CPU**. This is the largest observed indivisible external ABI call, not a universal tick/history bound. Separate fee collection and position mutation cannot simply be assumed safe across transactions: fresh fee ownership, valuation, all reward boundaries, debt/custody and recovery still require a design. No statement about source-line or instruction-level indivisibility is made.\n'
(OUT/'measurement.md').write_text(text)
print(json.dumps({'maximum':maximum,'all_below_100m':result['all_observed_venue_calls_below_100m'],'calls':len(all_venue),'input_sha256':hashlib.sha256(calls).hexdigest()}))
