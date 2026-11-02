// Compare complete measured transaction results, normalizing only the reviewed
// receipt executable change. No signing or transaction execution.
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const crypto = require('node:crypto');
const {xdr} = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const [baseline, candidate, output] = process.argv.slice(2);
if (!baseline || !candidate || !output || fs.existsSync(output)) throw Error('Need two runs and a fresh output');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function receiptHash(run) {
  const configuration = JSON.parse(fs.readFileSync(path.join(run, 'run-configuration.json')));
  const build = JSON.parse(fs.readFileSync(path.join(configuration.build, 'build-manifest.json')));
  return build.wasm['receipt_vault.wasm'].sha256;
}
const allowed = new Set([receiptHash(baseline), receiptHash(candidate)]);
if (allowed.size !== 2) throw Error('Expected precisely two distinct receipt executables');
async function project(run) {
  const results = [];
  for await (const line of readline.createInterface({input: fs.createReadStream(path.join(run, 'transactions.jsonl')), crlfDelay: Infinity})) {
    if (!line.trim()) continue;
    const r = JSON.parse(line);
    if (!r.resources) throw Error('Missing complete transaction evidence');
    const entries = [];
    for (const item of r.entry_size_checks) {
      const e = xdr.LedgerEntry.fromXDR(item.entry_xdr, 'base64');
      if (e.data().switch().name === 'contractCode' && allowed.has(e.data().contractCode().hash().toString('hex'))) continue;
      if (e.data().switch().name === 'contractData') {
        const data = e.data().contractData();
        if (data.val().switch().name === 'scvContractInstance') {
          const executable = data.val().instance().executable();
          // js-xdr union accessors are getters; replace the union through the
          // enclosing struct setter, rather than passing a value to wasm().
          if (executable.switch().name === 'contractExecutableWasm' && allowed.has(executable.wasmHash().toString('hex'))) data.val().instance().executable(xdr.ContractExecutable.contractExecutableWasm(Buffer.alloc(32)));
        }
      }
      // Includes original ledger header and all data values. No accounting,
      // amount, beneficiary, custody or reward field is normalized away.
      entries.push([item.phase, item.key_xdr, e.toXDR('base64')]);
    }
    entries.sort((a,b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
    const envelope = xdr.TransactionEnvelope.fromXDR(r.unsigned_envelope_xdr, 'base64');
    const auth = envelope.v1().tx().operations()[0].body().invokeHostFunctionOp().auth().map(a => a.toXDR('base64'));
    if (!r.qualification_full_logical_prestate_sha256 || !r.qualification_full_logical_poststate_sha256 || !r.qualification_ttl_changes) throw Error('Full logical state / TTL comparison evidence missing');
    const ttls = r.qualification_ttl_changes.map(t => {
      const key = xdr.LedgerKey.fromXDR(t.key_xdr,'base64');
      const normalized = key.switch().name === 'contractCode' && allowed.has(key.contractCode().hash().toString('hex')) ? 'reviewed-receipt-code' : t.key_xdr;
      return [normalized,t.old_live_until,t.new_live_until];
    }).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)));
    const projection = {
      test: r.test_name, flow: r.flow, success: r.success, error: r.error_xdr,
      returned: r.return_value_xdr, events: r.contract_events_xdr, auth,
      entryDataHash: hash(JSON.stringify(entries)), entries: entries.length,
      fullLogicalBefore:r.qualification_full_logical_prestate_sha256,
      fullLogicalAfter:r.qualification_full_logical_poststate_sha256,
      ttlHash:hash(JSON.stringify(ttls)),
    };
    results.push({projection, cpu:r.resources.instructions, memory:r.resources.memory_bytes, fee:r.fees.total});
  }
  return results;
}
(async () => {
  const a = await project(baseline), b = await project(candidate);
  if (a.length !== b.length || !a.length) throw Error('Transaction coverage differs or is empty');
  const differences = [], matched = [], savings = {};
  for (let i = 0; i < a.length; i++) {
    if (JSON.stringify(a[i].projection) !== JSON.stringify(b[i].projection)) {
      differences.push({row:i,baseline:a[i].projection,candidate:b[i].projection});
      continue;
    }
    matched.push(i);
    const flow = a[i].projection.flow;
    (savings[flow] ||= []).push({row:i,cpu_before:a[i].cpu,cpu_after:b[i].cpu,cpu_saved:a[i].cpu-b[i].cpu,memory_before:a[i].memory,memory_after:b[i].memory,fee_before:a[i].fee,fee_after:b[i].fee});
  }
  const result = {schema:'cl-atomic-differential-v1',release_approval:false,
    scope:'Same fixture, returns/events/authorization forest, touched before/after ledger data, full logical contract data before/after and all reported per-key TTL changes. Only pinned receipt executable identities normalized; code bytes separately pinned.',
    receipt_code_hashes:[...allowed],transactions:a.length,matched:matched.length,differences,savings};
  fs.writeFileSync(output, JSON.stringify(result,null,2)+'\n');
  process.stdout.write(JSON.stringify({transactions:a.length,matched:matched.length,differences:differences.length,release_approval:false})+'\n');
  if (differences.length) process.exitCode=1;
})().catch(e => {process.stderr.write(e.stack+'\n');process.exitCode=2;});
