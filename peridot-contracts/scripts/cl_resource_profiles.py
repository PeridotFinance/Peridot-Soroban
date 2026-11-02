"""Explicit policy and captured-network diagnostics. Neither is release approval."""
import argparse
import hashlib
import json
from pathlib import Path

FIELDS = {
 'instructions':('configSettingContractComputeV0','txMaxInstructions'),
 'mem_bytes':('configSettingContractComputeV0','txMemoryLimit'),
 'disk_read_entries':('configSettingContractLedgerCostV0','txMaxDiskReadEntries'),
 'write_entries':('configSettingContractLedgerCostV0','txMaxWriteLedgerEntries'),
 'ledger_entries':('configSettingContractLedgerCostExtV0','txMaxFootprintEntries'),
 'disk_read_bytes':('configSettingContractLedgerCostV0','txMaxDiskReadBytes'),
 'write_bytes':('configSettingContractLedgerCostV0','txMaxWriteBytes'),
 'contract_events_size_bytes':('configSettingContractEventsV0','txMaxContractEventsSizeBytes'),
 'max_contract_data_key_size_bytes':('configSettingContractDataKeySizeBytes',None),
 'max_contract_data_entry_size_bytes':('configSettingContractDataEntrySizeBytes',None),
 'max_contract_code_entry_size_bytes':('configSettingContractMaxSizeBytes',None),
 'transaction_size_bytes':('configSettingContractBandwidthV0','txMaxSizeBytes'),
}
POLICY = dict(instructions=100_000_000,mem_bytes=41_943_040,disk_read_entries=100,
 write_entries=50,ledger_entries=100,disk_read_bytes=200_000,write_bytes=132_096,
 contract_events_size_bytes=16_384,max_contract_data_key_size_bytes=250,
 max_contract_data_entry_size_bytes=65_536,max_contract_code_entry_size_bytes=131_072,
 transaction_size_bytes=132_096)

def create(config, name, mode='acceptance'):
    if name not in ('internal-policy','captured-network-diagnostic') or mode not in ('acceptance','profiling-only'):
        raise ValueError('unknown profile or mode')
    raw=Path(config).read_bytes(); c=json.loads(raw)
    if c['network']['passphrase']!='Public Global Stellar Network ; September 2015':raise ValueError('wrong captured network')
    if type(c['ledger']) is not int or c['ledger']<=0:raise ValueError('missing ledger')
    if type(c['network']['protocolVersion']) is not int or c['network']['protocolVersion']<=0:raise ValueError('missing protocol')
    models=[c['settings']['configSettingContractCostParams'+part]['values'] for part in ('CpuInstructions','MemoryBytes')]
    if len(models[0])!=len(models[1]):raise ValueError('cost model vector lengths differ')
    limits={}
    for field,(setting,key) in FIELDS.items():
        v=c['settings'][setting]['values'];v=v[key] if key else v
        if type(v) is not int or v<=0:raise ValueError('invalid limit '+field)
        limits[field]=v
    for namepart in ('CpuInstructions','MemoryBytes'):
        values=c['settings']['configSettingContractCostParams'+namepart]['values']
        if not isinstance(values,list) or not values:raise ValueError('missing cost model')
        for entry in values:
            if entry['ext']!={'variant':0} or any(type(entry[k]) is not int or entry[k]<0 for k in ('constTerm','linearTerm')):raise ValueError('invalid cost model')
    if name=='internal-policy':limits={k:min(v,limits[k]) for k,v in POLICY.items()}
    return dict(schema='cl-resource-profile-v1',name=name,mode=mode,network_config_path=str(Path(config).resolve()),network_sha256=hashlib.sha256(raw).hexdigest(),captured_ledger=c['ledger'],target_protocol=c['network']['protocolVersion'],limits=limits,release_approval=False)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('config');p.add_argument('output');p.add_argument('--profile',required=True);p.add_argument('--mode',default='acceptance');a=p.parse_args()
    try:profile=create(a.config,a.profile,a.mode)
    except (KeyError,TypeError,ValueError) as e:raise SystemExit('INVALID RESOURCE CONFIG: '+str(e))
    Path(a.output).write_text(json.dumps(profile,indent=2)+'\n')
