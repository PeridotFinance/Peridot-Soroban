// Offline XDR validation only. No RPC client, identity, key or signing API.
const fs = require('node:fs');
const crypto = require('node:crypto');
const { xdr, Address } = require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function decode(type, encoded) {
  if (typeof encoded !== 'string') throw Error('missing serialized XDR');
  const value = type.fromXDR(encoded, 'base64');
  if (value.toXDR('base64') !== encoded) throw Error('non-canonical XDR');
  return value;
}
try {
  const evidence = JSON.parse(fs.readFileSync(0, 'utf8'));
  const envelope = decode(xdr.TransactionEnvelope, evidence.unsigned_envelope_xdr);
  if (envelope.switch().name !== 'envelopeTypeTx') throw Error('expected V1 transaction envelope');
  const v1 = envelope.v1();
  if (v1.signatures().length !== 0) throw Error('signed envelope forbidden');
  const tx = v1.tx();
  if (tx.operations().length !== 1) throw Error('expected exactly one operation');
  const op = tx.operations()[0].body();
  if (op.switch().name !== 'invokeHostFunction') throw Error('expected invokeHostFunction');
  const invoke = op.invokeHostFunctionOp();
  if (invoke.hostFunction().switch().name !== 'hostFunctionTypeInvokeContract') throw Error('expected contract function invocation');
  const functionCall = invoke.hostFunction().invokeContract();
  const auth = invoke.auth();
  const authorizers = [];
  let mismatchedArgumentIndices = [];
  for (const entry of auth) {
    const credential = entry.credentials();
    const kind = credential.switch().name;
    if (kind === 'sorobanCredentialsSourceAccount') {
      authorizers.push('SOURCE_ACCOUNT');
      continue;
    }
    if (!['sorobanCredentialsAddress', 'sorobanCredentialsAddressV2'].includes(kind)) {
      throw Error('unsupported recording authorization credentials');
    }
    const address = credential.value();
    if (address.signature().switch().name !== 'scvVoid') {
      throw Error('recording-only authorization must not contain signatures');
    }
    authorizers.push(Address.fromScAddress(address.address()).toString());
  }
  if (evidence.account_authorization_mode === 'enforcing-mismatched-root') {
    if (auth.length !== 1 || auth[0].credentials().switch().name !== 'sorobanCredentialsAddress') {
      throw Error('mismatched-root proof requires exactly one external address authorization');
    }
    const credential = auth[0].credentials().address();
    if (credential.nonce().toString() !== '0' || credential.signatureExpirationLedger() !== evidence.current_ledger + 100) {
      throw Error('unexpected mismatched-root credential nonce/expiry');
    }
    const root = auth[0].rootInvocation();
    if (root.subInvocations().length !== 0 || root.function().switch().name !== 'sorobanAuthorizedFunctionTypeContractFn') {
      throw Error('mismatched-root proof requires a single contract root without children');
    }
    const wrong = root.function().contractFn();
    if (wrong.contractAddress().toXDR('base64') !== functionCall.contractAddress().toXDR('base64') ||
        wrong.functionName().toString() !== functionCall.functionName().toString() ||
        wrong.args().length !== functionCall.args().length) throw Error('mismatched root changed target/function/arity');
    mismatchedArgumentIndices = wrong.args().flatMap((arg, i) =>
      arg.toXDR('base64') === functionCall.args()[i].toXDR('base64') ? [] : [i]);
    if (mismatchedArgumentIndices.length === 0) throw Error('mismatched-root proof contains matching arguments');
  }
  if (tx.ext().switch() !== 1) throw Error('Soroban transaction resources missing');
  const data = tx.ext().sorobanData();
  const resources = data.resources();
  const footprint = resources.footprint();
  const restored = data.ext().switch() === 1 ? data.ext().resourceExt().archivedSorobanEntries() : [];
  const readOnly = footprint.readOnly().map(v => v.toXDR('base64'));
  const readWrite = footprint.readWrite().map(v => v.toXDR('base64'));
  if (new Set([...readOnly, ...readWrite]).size !== readOnly.length + readWrite.length) throw Error('duplicate footprint key');
  const rejected = evidence.outcome === 'rejected';
  let ret = null, errorType = null, errorCode = null;
  if (rejected) {
    if (evidence.return_value_xdr !== null) throw Error('rejected invocation returned a value');
    const error = decode(xdr.ScError, evidence.error_xdr);
    errorType = error.switch().name;
    errorCode = errorType === 'sceContract' ? error.value() : error.value().name;
  } else {
    ret = decode(xdr.ScVal, evidence.return_value_xdr);
  }
  const events = evidence.contract_events_xdr.map(v => decode(xdr.ContractEvent, v));
  const maxima = {max_contract_data_key_size_bytes: 0,
    max_contract_data_entry_size_bytes: 0, max_contract_code_entry_size_bytes: 0};
  const postState = {};
  const keyNames = {};
  const phases = new Set();
  for (const item of evidence.entry_size_checks) {
    if (!['before', 'after'].includes(item.phase)) throw Error('entry phase missing');
    if (rejected && item.phase === 'after') throw Error('rejected invocation exported a changed entry');
    const phaseKey = item.key_xdr + ':' + item.phase;
    if (phases.has(phaseKey)) throw Error('duplicate entry phase');
    phases.add(phaseKey);
    const key = decode(xdr.LedgerKey, item.key_xdr);
    if (![...readOnly, ...readWrite].includes(item.key_xdr)) throw Error('entry size check outside footprint');
    const entry = decode(xdr.LedgerEntry, item.entry_xdr);
    const encoded = entry.toXDR();
    if (item.entry_size_bytes !== encoded.length || item.entry_sha256 !== hash(encoded)) throw Error('entry size/hash mismatch');
    if (key.switch().name === 'contractData') {
      maxima.max_contract_data_key_size_bytes = Math.max(maxima.max_contract_data_key_size_bytes, key.toXDR().length);
      if (entry.data().switch().name !== 'contractData') throw Error('contract data entry/key type mismatch');
      const d = entry.data().contractData();
      const derived = xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({contract: d.contract(), key: d.key(), durability: d.durability()}));
      if (derived.toXDR('base64') !== item.key_xdr) throw Error('contract data entry/key mismatch');
      maxima.max_contract_data_entry_size_bytes = Math.max(maxima.max_contract_data_entry_size_bytes, encoded.length);
    } else if (key.switch().name === 'contractCode') {
      if (entry.data().switch().name !== 'contractCode' ||
          !key.contractCode().hash().equals(entry.data().contractCode().hash())) throw Error('contract code entry/key mismatch');
      maxima.max_contract_code_entry_size_bytes = Math.max(maxima.max_contract_code_entry_size_bytes, encoded.length);
    }
    // On host rejection ledger_changes is empty, including read-only changes.
    // The change-derived hash is therefore {}, not a rollback proof by itself;
    // full rollback is separately attested and bound to completed test assertions.
    if (!rejected && ['contractData', 'contractCode'].includes(key.switch().name) &&
        (item.phase === 'after' || (readOnly.includes(item.key_xdr) && !(item.key_xdr in postState)))) {
      postState[item.key_xdr] = entry.data().toXDR('base64');
    }
  }
  // A deleted entry may have no output bytes but its key still consumes space.
  for (const encoded of [...readOnly, ...readWrite]) {
    const key = decode(xdr.LedgerKey, encoded);
    if (key.switch().name === 'contractData') {
      maxima.max_contract_data_key_size_bytes = Math.max(maxima.max_contract_data_key_size_bytes, key.toXDR().length);
      const val = key.contractData().key();
      const head = val.switch().name === 'scvVec' ? val.vec()?.[0] : val;
      if (head?.switch().name === 'scvSymbol') keyNames[encoded] = head.sym().toString();
    }
  }
  const bytes = envelope.toXDR();
  console.log(JSON.stringify({
    envelope_sha256: hash(bytes), envelope_size_bytes: bytes.length,
    transaction_size_bytes: tx.toXDR().length,
    authorization_entries: auth.length,
    authorizers,
    mismatched_argument_indices: mismatchedArgumentIndices,
    authorization_size_bytes: 4 + auth.reduce((n, v) => n + v.toXDR().length, 0),
    host_function_sha256: hash(invoke.hostFunction().toXDR()),
    function_name: functionCall.functionName().toString(),
    contract_address: Address.fromScAddress(functionCall.contractAddress()).toString(),
    read_only_xdr: readOnly, read_write_xdr: readWrite,
    restored_rw_entry_indices: restored, footprint_key_names: keyNames,
    instructions: resources.instructions(), disk_read_bytes: resources.diskReadBytes(),
    write_bytes: resources.writeBytes(), resource_fee: data.resourceFee().toString(),
    transaction_fee: tx.fee(),
    return_value_size_bytes: ret === null ? 0 : ret.toXDR().length,
    error_type: errorType, error_code: errorCode,
    contract_events_size_bytes: events.reduce((n, v) => n + v.toXDR().length, 0),
    ...maxima,
    post_state_data_sha256: hash(Buffer.from(JSON.stringify(Object.fromEntries(Object.keys(postState).sort().map(k => [k, postState[k]]))))),
  }));
} catch (error) {
  console.error(String(error.message));
  process.exitCode = 1;
}
