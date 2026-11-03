// A lower bound from a COMPLETED call tree, not a fabricated cold execution.
const fs=require('node:fs');
const {xdr}=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const [run,out,flowFilter]=process.argv.slice(2);
if(!run||!out||fs.existsSync(out))throw Error('Need run and fresh output');
const config=JSON.parse(fs.readFileSync(run+'/run-configuration.json'));
const build=JSON.parse(fs.readFileSync(config.build+'/build-manifest.json'));
const selected=new Map(['receipt_vault.wasm','simple_peridottroller.wasm','margin_controller.wasm','cl_share_vault.wasm'].map(n=>[build.wasm[n].sha256,n]));
// The separately pinned Aquarius binary is not one of the eight project WASMs.
selected.set('12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6','aquarius-pool.wasm');
const roots={}, required=flowFilter?flowFilter.split(','):['liquidate_funded_v3','activate_open_position_v3','swap_close_position_v3','swap_open_position_v3'];
for(const line of fs.readFileSync(run+'/transactions.jsonl','utf8').trim().split('\n')){
 const raw=JSON.parse(line),r=raw.storage_data_transaction||raw;
 const flow=raw.flow;
 if(!required.includes(flow)||!r.success)continue;
 const footprint=new Set([...r.footprint.read_only_xdr,...r.footprint.read_write_xdr]);
 const entries=new Map();
 for(const item of r.entry_size_checks){
  if(!footprint.has(item.key_xdr))continue;
  const entry=xdr.LedgerEntry.fromXDR(item.entry_xdr,'base64');
  if(entry.data().switch().name!=='contractCode')continue;
  const code=entry.data().contractCode(),hash=code.hash().toString('hex');
  if(selected.has(hash))entries.set(hash,{wasm:selected.get(hash),sha256:hash,wasm_bytes:code.code().length,serialized_ledger_entry_bytes:entry.toXDR().length});
 }
 const codes=[...entries.values()],total=codes.reduce((n,c)=>n+c.serialized_ledger_entry_bytes,0);
 const candidate={code_entries:codes,required_archived_code_bytes_lower_bound:total,network_disk_read_limit:200000,excess:Math.max(0,total-200000),completed_data_execution_cpu:r.resources.instructions};
 if(!roots[flow]||total>roots[flow].required_archived_code_bytes_lower_bound)roots[flow]=candidate;
}
if(required.some(f=>!roots[f]))throw Error('Missing completed required root');
fs.writeFileSync(out,JSON.stringify({schema:'cl-cold-code-lower-bound-v1',scope:'Required selected code entries in completed data-restored call trees. Excludes instances/data/authorization. Not a completed cold execution or fee estimate.',release_approval:false,roots},null,2)+'\n');
console.log(JSON.stringify(roots));
