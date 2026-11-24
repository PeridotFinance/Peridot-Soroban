// Read-only Mainnet simulations. Never signs or submits.
const {S, MAINNET, chain, addr, u128, json} = require('/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/research/cash-borrowing-activation-20261117/tools/lib.cjs');
const fs = require('fs'); const crypto = require('crypto');
const POOL = 'CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV';
(async () => {
  const c = chain(MAINNET); const rpc = c.rpc;
  const acct = await rpc.getAccount(MAINNET.admin);
  const wasm = fs.readFileSync(process.argv[2]);
  const hash = crypto.createHash('sha256').update(wasm).digest();
  const out = {observedAt: new Date().toISOString(), wasmSha256: hash.toString('hex'), wasmBytes: wasm.length};
  const sim = async (label, op) => {
    const tx = new S.TransactionBuilder(acct, {fee: '100', networkPassphrase: MAINNET.network}).addOperation(op).setTimeout(0).build();
    const r = await rpc.simulateTransaction(tx);
    if (!S.rpc.Api.isSimulationSuccess(r)) { out[label] = {error: String(r.error).split('\n').slice(0, 3).join(' | ').slice(0, 400)}; return; }
    const d = r.transactionData.build(); const res = d.resources(); const fp = res.footprint();
    out[label] = {minResourceFeeStroops: r.minResourceFee, minResourceFeeXLM: Number(r.minResourceFee) / 1e7, cpuInsns: r.cost?.cpuInsns ?? res.instructions(),
      instructions: res.instructions(), diskReadBytes: res.diskReadBytes ? res.diskReadBytes() : undefined, writeBytes: res.writeBytes(),
      readOnly: fp.readOnly().length, readWrite: fp.readWrite().length, footprintEntries: fp.readOnly().length + fp.readWrite().length, latestLedger: r.latestLedger,
      auth: (r.result?.auth || []).length};
  };
  await sim('upload', S.Operation.uploadContractWasm({wasm}));
  await sim('create', S.Operation.createCustomContract({address: new S.Address(MAINNET.admin), wasmHash: hash, salt: crypto.randomBytes(32),
    constructorArgs: [addr(MAINNET.admin), addr(MAINNET.controller), addr(MAINNET.admin)]}));
  const pool = new S.Contract(POOL);
  const u32 = n => S.nativeToScVal(n, {type: 'u32'});
  await sim('pool_swap_100XLM_to_USDC', pool.call('swap', addr(MAINNET.admin), u32(0), u32(1), u128(1000000000n), u128(1n)));
  await sim('pool_swap_15USDC_to_XLM', pool.call('swap', addr(MAINNET.admin), u32(1), u32(0), u128(150000000n), u128(1n)));
  out.poolTokens = await c.tryView(POOL, 'get_tokens');
  out.estimate_100XLM = await c.tryView(POOL, 'estimate_swap', [u32(0), u32(1), u128(1000000000n)]);
  console.log(json(out));
})().catch(e => { console.error(e); process.exit(1); });
