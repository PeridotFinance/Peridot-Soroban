// Offline decoding only: no RPC, wallet, secret, signing or submission API.
const fs = require('node:fs');
const crypto = require('node:crypto');
const {xdr} = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
function decode(type, encoded) {
  if (typeof encoded !== 'string' || !encoded) throw Error('missing serialized XDR');
  const value = type.fromXDR(encoded, 'base64');
  if (value.toXDR('base64') !== encoded) throw Error('non-canonical XDR');
  return value;
}
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
try {
  const row = JSON.parse(fs.readFileSync(0, 'utf8'));
  const env = decode(xdr.TransactionEnvelope, row.unsigned_envelope_xdr);
  if (env.switch().name !== 'envelopeTypeTx') throw Error('expected V1 transaction envelope');
  const v1 = env.v1(), tx = v1.tx();
  if (v1.signatures().length) throw Error('signed envelope forbidden');
  if (tx.operations().length !== 1) throw Error('exactly one restore operation required');
  const op = tx.operations()[0].body();
  if (op.switch().name !== 'restoreFootprint') throw Error('expected RestoreFootprintOp');
  if (op.restoreFootprintOp().ext().switch() !== 0) throw Error('unsupported restore extension');
  if (tx.ext().switch() !== 1) throw Error('missing Soroban resources');
  const helper = decode(xdr.SorobanTransactionData, row.helper_transaction_data_xdr);
  const data = tx.ext().sorobanData(), resources = data.resources();
  if (data.ext().switch() !== 0 || helper.ext().switch() !== 0) throw Error('restore requires resource extension V0');
  const footprint = resources.footprint();
  if (footprint.readOnly().length) throw Error('restore read-only footprint must be empty');
  if (resources.toXDR('base64') !== helper.resources().toXDR('base64')) throw Error('helper and envelope resources differ');
  const keys = footprint.readWrite().map(k=>k.toXDR('base64'));
  if (!keys.length || new Set(keys).size !== keys.length) throw Error('empty or duplicate restore footprint');
  if (!Array.isArray(row.keys) || row.keys.length !== keys.length) throw Error('missing entry for restore key');
  const entries = row.keys.map((item,i)=>{
    if (item.key_xdr !== keys[i]) throw Error('entry order or footprint mismatch');
    const key = decode(xdr.LedgerKey,item.key_xdr), entry = decode(xdr.LedgerEntry,item.entry_xdr);
    const afterEncoded = item.predicted_entry_xdr ?? item.after_entry_xdr;
    const after = decode(xdr.LedgerEntry,afterEncoded);
    if (entry.data().toXDR('base64') !== after.data().toXDR('base64')) throw Error('restore changed contract entry data');
    const kind = key.switch().name;
    if (!['contractCode','contractData'].includes(kind) || entry.data().switch().name !== kind) throw Error('nonpersistent or mismatched key type');
    let codeMemoryInputs=[];
    if (kind === 'contractData') {
      const d = entry.data().contractData();
      if (d.durability().name !== 'persistent' || key.contractData().durability().name !== 'persistent') throw Error('temporary data cannot be restored');
      const derived = xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({contract:d.contract(),key:d.key(),durability:d.durability()}));
      if (derived.toXDR('base64') !== item.key_xdr) throw Error('entry does not match key');
    } else {
      const code = entry.data().contractCode();
      if (!key.contractCode().hash().equals(code.hash()) || hash(code.code()) !== code.hash().toString('hex')) throw Error('code bytes/hash/key mismatch');
      if (code.ext().switch()===0) codeMemoryInputs=[{cost_type:xdr.ContractCostType.vmInstantiation().value,input:code.code().length}];
      else if (code.ext().switch()===1) {
        const inputs=code.ext().v1().costInputs();
        for(const [cost,field] of [['parseWasmInstructions','nInstructions'],['parseWasmFunctions','nFunctions'],['parseWasmGlobals','nGlobals'],['parseWasmTableEntries','nTableEntries'],['parseWasmTypes','nTypes'],['parseWasmDataSegments','nDataSegments'],['parseWasmElemSegments','nElemSegments'],['parseWasmImports','nImports'],['parseWasmExports','nExports'],['parseWasmDataSegmentBytes','nDataSegmentBytes']])codeMemoryInputs.push({cost_type:xdr.ContractCostType[cost]().value,input:inputs[field]()});
      } else throw Error('unsupported code cost-input version');
    }
    return {key_xdr:item.key_xdr,kind,code_memory_inputs:codeMemoryInputs,key_bytes:key.toXDR().length,entry_bytes:entry.toXDR().length,
      entry_data_sha256:hash(entry.data().toXDR()),predicted_entry_data_sha256:hash(after.data().toXDR()),
      entry_sha256:hash(entry.toXDR()),predicted_entry_sha256:hash(after.toXDR())};
  });
  process.stdout.write(JSON.stringify({envelope_size_bytes:env.toXDR().length,envelope_sha256:hash(env.toXDR()),
    tx_fee:tx.fee().toString(),resource_fee:data.resourceFee().toString(),helper_resource_fee:helper.resourceFee().toString(),
    instructions:resources.instructions(),disk_read_bytes:resources.diskReadBytes(),write_bytes:resources.writeBytes(),
    keys,entries,signatures:0,operation:'RestoreFootprintOp'}));
} catch (error) {console.error(error.message);process.exit(1);}
