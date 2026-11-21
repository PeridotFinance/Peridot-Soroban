// Operator one-shot, run ON the CL liquidator host as the service user, after the liquidator account exists (G43):
// add the USDC and PYUSD trustlines it needs to receive its 200 USDC inventory (G44) and in-kind exits of seized units
// (PYUSD + USDC). Mirrors bots/cash-liquidator/src/trustlines.mjs.
// Default: dry run (prints the plan and the unsigned hash). CONFIRM_TRUSTLINES=yes signs with executorKeyFile and
// submits ONCE; check the printed hash on chain before any retry.
//   node src/trustlines.mjs --config config/mainnet.local.json [CONFIRM_TRUSTLINES=yes in the environment]
// The heartbeat account needs no trustline (fee-only).
import {fileURLToPath} from 'node:url';
import {MAINNET, loadSdk} from './chain.mjs';
import {loadConfig} from './config.mjs';
import {makeRpc, openBudget, isMainnet, CLASSIC, trustlineStatus} from './mainnet.mjs';
import {loadKeyFile} from './keyfile.mjs';

export {CLASSIC};

/** Offline: each classic asset's SAC id must equal the configured contract, or nothing is built. */
export function trustlineAssets(cfg, S = loadSdk()) {
  return Object.entries(CLASSIC).map(([n, a]) => {
    const asset = new S.Asset(a.code, a.issuer), id = asset.contractId(cfg.networkPassphrase);
    if (id !== cfg.contracts[a.contract]) throw Error(`${n}: classic asset ${a.code}:${a.issuer.slice(0, 6)}... is not the configured SAC ${String(cfg.contracts[a.contract]).slice(0, 6)}...`);
    return {name: n, asset};
  });
}

/** Only the missing trustlines (an existing one is never changed), fixed 10,000-stroop fee per operation. */
export function buildTrustlineTx(account, assets, network, existing = new Set(), S = loadSdk()) {
  const todo = assets.filter(a => !existing.has(`${a.asset.code}:${a.asset.issuer}`));
  if (!todo.length) return null;
  const b = new S.TransactionBuilder(account, {fee: '10000', networkPassphrase: network});
  for (const {asset} of todo) b.addOperation(S.Operation.changeTrust({asset}));
  return b.setTimeout(180).build();
}

async function horizonTrustlines(cfg, account) {
  const url = `${cfg.horizonUrl ?? 'https://horizon.stellar.org'}/accounts/${account}`;
  const r = await fetch(url, {headers: {accept: 'application/json'}, signal: AbortSignal.timeout(15_000)});
  if (!r.ok) throw Error(`horizon ${r.status}: account missing? (fund it first, G43)`);
  const j = await r.json();
  return new Set(j.balances.filter(b => b.asset_type !== 'native').map(b => `${b.asset_code}:${b.asset_issuer}`));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  (async () => {
    const i = process.argv.indexOf('--config'); if (i < 0) throw Error('usage: node src/trustlines.mjs --config <mainnet config>');
    const cfg = loadConfig(process.argv[i + 1]);
    if (!isMainnet(cfg)) throw Error('trustlines.mjs is for the Mainnet key-file signer (Testnet: use the Stellar CLI)');
    const S = loadSdk(), rpc = makeRpc(cfg, {S});
    const assets = trustlineAssets(cfg, S), existing = await horizonTrustlines(cfg, cfg.liquidator);
    const acc = await rpc.getAccount(cfg.liquidator);
    const tx = buildTrustlineTx(acc, assets, MAINNET, existing, S);
    console.log(JSON.stringify({account: cfg.liquidator, existing: [...existing], add: tx ? tx.operations.map(o => `${o.line.code}:${o.line.issuer}`) : [], unsignedHash: tx ? tx.hash().toString('hex') : null}));
    if (!tx) { console.log('nothing to do: both trustlines exist'); return; }
    if (process.env.CONFIRM_TRUSTLINES !== 'yes') { console.log('dry run: set CONFIRM_TRUSTLINES=yes to sign and submit once'); return; }
    openBudget(cfg.dataDir, cfg).reserve('liquidator', BigInt(tx.fee));
    tx.sign(loadKeyFile(cfg.executorKeyFile, {expectedPublicKey: cfg.liquidator}, S));
    const hash = tx.hash().toString('hex'); console.log(JSON.stringify({signed: true, hash}));
    const sent = await rpc.sendTransaction(tx);
    if (sent.status === 'ERROR') { console.log(JSON.stringify({hash, status: 'ERROR'})); process.exitCode = 1; return; }
    for (let k = 0; k < 45; k++) {
      await new Promise(res => setTimeout(res, 2000));
      const g = await rpc.getTransaction(hash).catch(() => ({status: 'NOT_FOUND'}));
      if (g.status !== 'NOT_FOUND') {
        const after = g.status === 'SUCCESS' ? await trustlineStatus(cfg, rpc, cfg.liquidator, S) : null;
        console.log(JSON.stringify({hash, status: g.status, ledger: g.ledger, trustlines: after}));
        if (g.status !== 'SUCCESS' || !after.USDC || !after.PYUSD) process.exitCode = 1;
        return;
      }
    }
    console.log(JSON.stringify({hash, status: 'UNKNOWN: check the hash before any retry'})); process.exitCode = 1;
  })().catch(e => { console.error('trustlines refused:', String(e.message).slice(0, 300)); process.exitCode = 1; });
}
