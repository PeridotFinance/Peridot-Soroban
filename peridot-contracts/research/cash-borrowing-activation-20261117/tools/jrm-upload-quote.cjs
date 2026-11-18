// Read-only fee quote for the OPTIONAL JumpRateModel path (not in the proposed plan): simulate uploadContractWasm of the
// reproduced 4c98bdd3 JRM build (sha256 4f57459d…, Ledger admin embedded). Unsigned, sequence-0 placeholder, never sent.
'use strict';
const fs = require('node:fs'); const crypto = require('node:crypto');
const {S, MAINNET, json, chain} = require('./lib.cjs');
const c = chain(MAINNET);
(async () => {
  const bytes = fs.readFileSync(process.argv[2]); const sha = crypto.createHash('sha256').update(bytes).digest('hex');
  const tx = new S.TransactionBuilder(new S.Account(MAINNET.admin, '0'), {fee: '100', networkPassphrase: MAINNET.network})
    .addOperation(S.Operation.uploadContractWasm({wasm: bytes})).setTimeout(300).build();
  const sim = await c.rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) throw Error(String(sim.error));
  console.log(json({observedAt: new Date().toISOString(), sha256: sha, bytes: bytes.length, minResourceFeeStroops: sim.minResourceFee,
    xlm: Number(sim.minResourceFee) / 1e7, note: 'upload only; instance creation + initialize + set_interest_model per market are extra (~0.05-0.5 XLM each, simulate before signing)'}));
})().catch(e => { console.error(e.message); process.exit(1); });
