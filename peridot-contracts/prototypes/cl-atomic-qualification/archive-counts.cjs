// Evidence decoding only: configured expiry is not proof of touched restoration.
const fs=require('node:fs'),readline=require('node:readline');
const {xdr,StrKey}=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const [run,output]=process.argv.slice(2);
if(!run||!output||fs.existsSync(output))throw Error('Need run and fresh output');
async function* rows(file){for await(const line of readline.createInterface({input:fs.createReadStream(file),crlfDelay:Infinity}))if(line.trim())yield JSON.parse(line);}
(async()=>{
 const roles=new Map();
 for await(const row of rows(`${run}/calls.jsonl`))for(const c of row.calls)if(c.address&&c.role!=='unlabelled')roles.set(c.address,c.role);
 function role(encoded){
  const key=xdr.LedgerKey.fromXDR(encoded,'base64');
  if(key.switch().name!=='contractData')return key.switch().name;
  const a=key.contractData().contract();
  return a.switch().name==='scAddressTypeContract'?(roles.get(StrKey.encodeContract(a.contractId()))||'unlabelled-contract'):'account-contract-data';
 }
 function count(keys){const c={};for(const k of new Set(keys)){const r=role(k);c[r]=(c[r]||0)+1;}return c;}
 const records=[];let n=0;
 for await(const row of rows(`${run}/transactions.jsonl`)){
  if(!row.resources){records.push({row:n++,flow:row.flow,missing_quantities:true});continue;}
  const touched=[...row.footprint.read_only_xdr,...row.footprint.read_write_xdr];
  const restored=row.restored_rw_entry_indices.map(i=>row.footprint.read_write_xdr[i]);
  const expired=row.archive.modified_keys.map(k=>k.key_xdr);
  if(restored.some(k=>!expired.includes(k)))throw Error('Restored key outside explicit expired fixture');
  records.push({row:n++,test:row.test_name,flow:row.flow,success:row.success,
    expired_by_role:count(expired),touched_by_role:count(touched),restored_by_role:count(restored),
    restored_unique_keys:new Set(restored).size,write_entries:row.resources.write_entries,
    data_only:true,code_or_instance_expiry_measured:false});
 }
 const result={schema:'cl-combined-restoration-counts-v1',release_approval:false,records};
 fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n');
 process.stdout.write(JSON.stringify({records:records.length,restored_records:records.filter(r=>r.restored_unique_keys>0).length,code_or_instance_expiry_measured:false})+'\n');
})().catch(e=>{process.stderr.write(e.stack+'\n');process.exitCode=1;});
