"""Offline decision analysis using preserved executions and a read-only capture.

No contract execution, profile generation, limit changes or release approval.
Archives are hash-checked in full; SDK and transaction populations stay separate.
"""
import argparse, hashlib, json, subprocess, tarfile
from pathlib import Path, PurePosixPath
from analyze_cl_decision import FLOWS, SDK, TX, maximum
from cl_resource_profiles import create
from cl_transaction_evidence import fee_configuration, compute_fees

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT.parent / 'docs/deployment-handoffs/evidence'
OLD_CONFIG = EVIDENCE / 'cl-resource-reconciliation-20261102/network-config.json'
PINS = {
    'macos_atomic_9cf4308': ('cl-decisions-reserve-20261102', 'checkpoint-payload.tar.xz',
        'b1b1017bf87ec095061846c81acc913a2b4bc57bbc918034dacc4f3056152fb0', 'checkpoint/'),
    'linux_atomic_9cf4308': ('cl-linux-ci-9cf4308-20261102', 'linux-ci-payload.tar.xz',
        'bd2f7c13727ffeede8a0f46fbcea395b9f03337ff0f309b81ba5cc175d42cd6a', 'artifact/cl-ci-checkpoint/'),
    'split_main': ('cl-split-prototype-20261102', 'checkpoint-payload.tar.xz',
        '3a6760f42ae337c731b6025f7855ecd259433e14d197760036cdef97e60075a7', ''),
    'split_recovery': ('cl-split-prototype-20261102/late-recovery', 'checkpoint-payload.tar.xz',
        '71cc4573337e7f6723c696e0d8e401903592dd5e2722fcac0c37d90ca0c72938', ''),
}

def require(ok, why):
    if not ok: raise ValueError(why)

def sha(data): return hashlib.sha256(data).hexdigest()

def verify_archive(pin):
    directory, filename, expected, prefix = pin
    root = EVIDENCE / directory; index = json.loads((root / 'index.json').read_text())
    archive = root / filename
    require(sha(archive.read_bytes()) == index['archive_sha256'] == expected, 'archive identity mismatch')
    require(archive.stat().st_size == index['archive_bytes'], 'archive length mismatch')
    members = index.get('payload_members', index.get('files'))
    with tarfile.open(archive, 'r:xz') as tar:
        entries = tar.getmembers()
        require(len(entries) == len(members) and {e.name for e in entries} == set(members), 'archive coverage mismatch')
        for entry in entries:
            name = PurePosixPath(entry.name)
            require(entry.isfile() and not name.is_absolute() and '..' not in name.parts, 'unsafe archive member')
            digest = hashlib.sha256(); size = 0
            source = tar.extractfile(entry)
            while True:
                data = source.read(1024 * 1024)
                if not data: break
                size += len(data); digest.update(data)
            require(size == members[entry.name]['bytes'] and digest.hexdigest() == members[entry.name]['sha256'], 'member mismatch: ' + entry.name)
    return index, archive, prefix

def read_json(tar, name): return json.load(tar.extractfile(name))
def rows(tar, name):
    for line in tar.extractfile(name):
        if line.strip(): yield json.loads(line)

def merge_max(current, row):
    for key, value in row.items(): current[key] = max(current.get(key, 0), value)

def atomic(pin, fresh):
    index, archive, prefix = verify_archive(pin)
    fresh_fee, fresh_rent = fee_configuration(fresh)
    with tarfile.open(archive, 'r:xz') as tar:
        profiles = {}
        for name in ['internal-policy', 'captured-network-diagnostic']:
            gates = read_json(tar, prefix + name + '-checkpoint-summary.json')
            require(gates == index['profiles'][name], 'saved gates/index differ')
            require(gates['release_approval'] is False and not gates['missing_required_flows'], 'missing proof coverage')
            passed = name == 'captured-network-diagnostic'
            require(gates['gates'] == dict.fromkeys(['invocation','lifecycle','transaction','checkpoint'], passed), 'baseline gates changed')
            require((len(gates['lifecycle_errors']), len(gates['transaction_errors'])) == ((0,0) if passed else (106,303)), 'baseline errors changed')
            profiles[name] = gates
        def check(row):
            require(row['runtime_protocol'] == 29 and row['config_sha256'] == sha(OLD_CONFIG.read_bytes())
                    and row['exact_captured_cost_model'] and row['sdk_limits_enforced']
                    and row['run_mode'] == 'acceptance', 'unsupported measurement provenance')
        measured = list(rows(tar, prefix + 'captured-network-diagnostic-resources.jsonl'))
        for row in measured: check(row)
        require(len(measured) == 1652, 'incomplete SDK evidence')
        flows = {name: {'sdk_maxima': maximum([r for r in measured if r['flow'] == name and r['ok']], SDK),
                        'transaction_entrypoint': abi, 'unsigned_transaction_maxima': {}, 'transaction_samples': 0}
                 for name, abi in FLOWS.items()}
        tx_max, restored_max, counts = {}, {}, {'all':0, 'success':0, 'restored_success':0, 'expected_rejections':0}
        for row in rows(tar, prefix + 'captured-network-diagnostic-transactions.jsonl'):
            check(row); counts['all'] += 1
            if not row['success']:
                require(row['expected_rejection'], 'unexpected captured transaction failure')
                counts['expected_rejections'] += 1; continue
            counts['success'] += 1; values = {k:f(row) for k,f in TX.items()}; merge_max(tx_max, values)
            # Same host/cost coefficients; retain original state/TTL quantities.
            # This is price compatibility, NOT fresh execution or live-state proof.
            fees = compute_fees(row['resources'], fresh_fee, row['rent_changes'], fresh_rent,
                                row['current_ledger'], row['fees']['inclusion'])
            require(fees == row['fees'], 'fresh price inputs change a preserved fee estimate')
            if row['archive']['restoration_observed']:
                counts['restored_success'] += 1; merge_max(restored_max, values)
            for value in flows.values():
                if value['transaction_entrypoint'] == row['flow']:
                    value['transaction_samples'] += 1; merge_max(value['unsigned_transaction_maxima'], values)
        require(counts['all'] == 2741 and all(v['transaction_samples'] for v in flows.values()), 'missing transaction flows')
        peaks = {}; call_count = 0
        for row in rows(tar, prefix + 'captured-network-diagnostic-calls.jsonl'):
            call_count += 1
            require(row['exclusive_reconciles'] and sum(c['exclusive']['instructions'] for c in row['calls']) == row['root_instructions'], 'CPU tree does not reconcile')
            name = row['flow']
            if name in flows and row['invocation_ok'] and (name not in peaks or row['root_instructions'] > peaks[name]['root_instructions']): peaks[name] = row
        require(call_count == 1652 and set(peaks) == set(flows), 'missing call profiles')
        for name, peak in peaks.items():
            venue = [c for c in peak['calls'] if c['role'] == 'aquarius']
            outer = [c for c in venue if not any(c['path'].startswith(p['path']+'.') for p in venue if p != c)]
            calls = [c for c in outer if c['function'] in ['claim','claim_all_position_fees','deposit_position','withdraw_position','swap']]
            flows[name]['venue_subtrees'] = [{'path':c['path'],'function':c['function'],'instructions':c['inclusive']['instructions']} for c in calls]
            flows[name]['conditional_preserved_venue_sum'] = sum(c['inclusive']['instructions'] for c in calls)
    return {'archive_sha256': index['archive_sha256'], 'profiles':profiles, 'sdk_rows':len(measured),
            'sdk_maxima':maximum([r for r in measured if r['ok']], SDK), 'unsigned_counts':counts,
            'unsigned_transaction_maxima':tx_max, 'restored_success_maxima':restored_max, 'flows':flows,
            'wasm':index.get('linux_wasm', index.get('wasm'))}

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--capture',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    require(not args.output.exists(), 'refusing existing analysis output')
    fresh=json.loads(args.capture.read_text());old=json.loads(OLD_CONFIG.read_text())
    # Validate required numeric limits and every cost coefficient without writing a profile.
    current=create(args.capture, 'captured-network-diagnostic');prior=create(OLD_CONFIG, 'captured-network-diagnostic')
    require(current['target_protocol'] == prior['target_protocol'] == 29, 'runtime changed: execution evidence must be renewed')
    model_names=['configSettingContractCostParamsCpuInstructions','configSettingContractCostParamsMemoryBytes']
    require(all(fresh['settings'][k]['values'] == old['settings'][k]['values'] for k in model_names), 'cost model changed: cannot reuse execution quantities')
    # Independently decode all raw setting XDR and compare the stored plain values.
    decoder=r'''
const fs=require('node:fs');const {xdr}=require('./bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const c=JSON.parse(fs.readFileSync(process.argv[1],'utf8'));
const plain=v=>typeof v==='number'?v:Array.isArray(v)?v.map(plain):v&&v._attributes?Object.fromEntries(Object.entries(v._attributes).map(([k,w])=>[k,plain(w)])):v&&v.switch?{variant:v.switch().name||v.switch()}:Number(v.toString());
for(const [name,r] of Object.entries(c.settings)){const e=xdr.LedgerEntryData.fromXDR(r.xdr,'base64');if(e.toXDR('base64')!==r.xdr||e.configSetting().switch().name!==name||JSON.stringify(plain(e.configSetting().value()))!==JSON.stringify(r.values))throw Error('XDR mismatch '+name);}
console.log('All '+Object.keys(c.settings).length+' setting XDR values independently decoded');
'''
    decoded=subprocess.run(['node','-e',decoder,str(args.capture.resolve())],cwd=ROOT,text=True,capture_output=True,check=True)
    builds={name:atomic(PINS[name],fresh) for name in ['macos_atomic_9cf4308','linux_atomic_9cf4308']}
    negative={}
    for name in ['split_main','split_recovery']:
        index,archive,_=verify_archive(PINS[name])
        with tarfile.open(archive,'r:xz') as tar: summary=read_json(tar,'execution-summary.json')
        require(summary['stage1_complete'] is False and summary['release_approval'] is False, 'negative experiment status changed')
        negative[name]={'archive_sha256':index['archive_sha256'],'profiles':{n:{k:s[k] for k in ['test_counts','gates','maxima','core_stages','core_lifecycle']} for n,s in summary['profiles'].items()}}
    maxima={}
    for build in builds.values():
        for kind in ['sdk_maxima','unsigned_transaction_maxima']:merge_max(maxima,build[kind])
    proposal={'instructions':300000000,'memory_bytes':33554432,'sdk_entry_total':300,'unique_footprint_keys':300,'write_entries':100}
    result={'schema':'cl-resource-decision-refresh-v1','stage1_complete':False,'release_approval':False,
        'policy_change_implemented':False,'stages_changed':False,'scope':'Preserved controlled-state measurements; fresh network settings only. No new contract execution.',
        'network_refresh':{'ledger':fresh['ledger'],'protocol':current['target_protocol'],'capture_sha256':sha(args.capture.read_bytes()),
            'prior_ledger':old['ledger'],'prior_sha256':sha(OLD_CONFIG.read_bytes()),'limits':current['limits'],
            'ceilings_unchanged':current['limits']==prior['limits'],'changed_settings':[k for k in fresh['settings'] if fresh['settings'][k]!=old['settings'][k]],
            'cost_models_unchanged':True,'cost_model_terms':{k:len(fresh['settings'][k]['values']) for k in model_names},
            'fee_inputs_unchanged':fee_configuration(fresh)==fee_configuration(old),'current_fee_inputs':fee_configuration(fresh),
            'raw_xdr_verification':decoded.stdout.strip()},
        'atomic_builds':builds,'negative_split':negative,'atomic_observed_maxima':maxima,
        'proposal_analysis_only':{k:{'ceiling':v,'workload_ceiling_80_percent':v*4//5,'observed':maxima[k],
            'remaining_to_workload_ceiling':v*4//5-maxima[k]} for k,v in proposal.items()}}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'analysis_verified':True,'output_sha256':sha(args.output.read_bytes()),'network_ledger':fresh['ledger'],'release_approval':False}))

if __name__=='__main__':main()
