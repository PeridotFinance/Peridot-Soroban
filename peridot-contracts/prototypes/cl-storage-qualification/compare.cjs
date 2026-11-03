// Business equivalence is strict. Maintenance footprint/TTL changes are emitted
// separately for review, never normalized into an accounting-equivalence claim.
const fs=require('node:fs'),crypto=require('node:crypto');
const {xdr}=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const [base,candidate,output,testFilter]=process.argv.slice(2);
if(!base||!candidate||!output||fs.existsSync(output))throw Error('Need two runs and fresh output');
const read=p=>fs.readFileSync(p,'utf8').trim().split('\n').filter(Boolean).map(JSON.parse);
const build=run=>JSON.parse(fs.readFileSync(JSON.parse(fs.readFileSync(run+'/run-configuration.json')).build+'/build-manifest.json'));
const select=rows=>testFilter?rows.filter(r=>r.test_name.includes(testFilter)):rows;
const a=select(read(base+'/transactions.jsonl')),b=select(read(candidate+'/transactions.jsonl'));
if(a.length!==b.length||!a.length)throw Error('Incomplete/mismatched row coverage');
const codeRoles=new Map();
for(const m of [build(base),build(candidate)])for(const name of ['receipt_vault.wasm','simple_peridottroller.wasm','margin_controller.wasm'])codeRoles.set(m.wasm[name].sha256,name);
const key=encoded=>{
 const k=xdr.LedgerKey.fromXDR(encoded,'base64');
 return k.switch().name==='contractCode'&&codeRoles.has(k.contractCode().hash().toString('hex'))?'reviewed-code:'+codeRoles.get(k.contractCode().hash().toString('hex')):encoded;
};
const business=r=>({test:r.test_name,flow:r.flow,success:r.success,error:r.error_xdr,returned:r.return_value_xdr,events:r.contract_events_xdr,
 auth:xdr.TransactionEnvelope.fromXDR(r.unsigned_envelope_xdr,'base64').v1().tx().operations()[0].body().invokeHostFunctionOp().auth().map(x=>x.toXDR('base64')),
 before:r.qualification_full_logical_prestate_sha256,after:r.qualification_full_logical_poststate_sha256});
const rows=[];
for(let i=0;i<a.length;i++){
 const x=a[i],y=b[i];
 if(!x.resources||!y.resources)throw Error('Missing complete host evidence row '+i);
 if(x.test_name!==y.test_name||x.flow!==y.flow)throw Error('Row identity mismatch '+i);
 const at=new Map(x.qualification_ttl_changes.map(v=>[key(v.key_xdr),v])),bt=new Map(y.qualification_ttl_changes.map(v=>[key(v.key_xdr),v]));
 const ak=new Set([...x.footprint.read_only_xdr,...x.footprint.read_write_xdr].map(key)),bk=new Set([...y.footprint.read_only_xdr,...y.footprint.read_write_xdr].map(key));
 const ttlDifferences=[];
 for(const k of new Set([...at.keys(),...bt.keys()])){
  const av=at.get(k),bv=bt.get(k);
  if(!av||!bv||av.old_live_until!==bv.old_live_until||av.new_live_until!==bv.new_live_until)ttlDifferences.push({key:k,before:av,after:bv,removed_from_candidate_footprint:!bk.has(k)});
 }
 const bx=business(x),by=business(y),fields=Object.keys(bx).filter(k=>JSON.stringify(bx[k])!==JSON.stringify(by[k]));
 rows.push({row:i,flow:x.flow,test:x.test_name,baseline_success:x.success,candidate_success:y.success,business_equal:!fields.length,different_business_fields:fields,
  full_prestate_equal:bx.before===by.before,full_poststate_equal:bx.after===by.after,
  removed_keys:[...ak].filter(k=>!bk.has(k)),added_keys:[...bk].filter(k=>!ak.has(k)),ttl_differences:ttlDifferences,
  baseline:x.resources,candidate:y.resources,baseline_fees:x.fees,candidate_fees:y.fees,
  cpu_saved:x.resources.instructions-y.resources.instructions,writes_saved:x.resources.write_entries-y.resources.write_entries,
  baseline_restored:x.restored_rw_entry_indices.length,candidate_restored:y.restored_rw_entry_indices.length});
}
const result={schema:'cl-storage-differential-v1',release_approval:false,policy_change_implemented:false,
 scope:'Exact returns/events/external auth forest/full logical contract state; only receipt/controller/V3 executable identity normalized by exporter. TTL and eliminated keys explicitly differ and require maintenance review.',
 baseline:base,candidate,test_filter:testFilter||null,rows,business_equal:rows.every(r=>r.business_equal),maintenance_review_required:true};
fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({rows:rows.length,business_equal:result.business_equal,differences:rows.filter(r=>!r.business_equal).length,maintenance_changed_rows:rows.filter(r=>r.ttl_differences.length).length}));
if(!result.business_equal)process.exitCode=1;
