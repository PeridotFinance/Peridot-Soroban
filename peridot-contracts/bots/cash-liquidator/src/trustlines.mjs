// Operator one-shot, run ON the liquidator host as the service user, after the account is funded with XLM:
// add the USDC and EURC trustlines the liquidator needs to receive inventory and seized collateral.
// Default is a dry run (prints the plan and the unsigned hash). CONFIRM_TRUSTLINES=yes signs with LIQUIDATOR_KEY_FILE
// and submits once. Usage: LIQUIDATOR_PUBLIC_KEY=G... LIQUIDATOR_KEY_FILE=/abs/key [CONFIRM_TRUSTLINES=yes] node src/trustlines.mjs
import * as S from '@stellar/stellar-sdk';
import {fileURLToPath} from 'node:url';
import {loadConfig} from './config.mjs';
import {makeChain} from './chain.mjs';

export const CLASSIC = {
  USDC: {code: 'USDC', issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'},
  EURC: {code: 'EURC', issuer: 'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2'},
};

/** Offline: each classic asset's SAC id must equal the pinned market underlying, or nothing is built. */
export function trustlineAssets(pins, network) {
  return Object.entries(CLASSIC).map(([n, a]) => {
    const asset = new S.Asset(a.code, a.issuer), id = asset.contractId(network);
    if (id !== pins.markets[n].underlying) throw Error(`${n}: classic asset does not match the pinned SAC`);
    return {market: n, asset};
  });
}

export function buildTrustlineTx(account, assets, network, fee = '10000') {
  const b = new S.TransactionBuilder(account, {fee, networkPassphrase: network});
  for (const {asset} of assets) b.addOperation(S.Operation.changeTrust({asset}));
  return b.setTimeout(180).build();
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const cfg = loadConfig();
  if (!cfg.keyFile) throw Error('LIQUIDATOR_KEY_FILE required');
  const c = makeChain(cfg);
  const acc = await c.rpc.getAccount(cfg.publicKey);
  const assets = trustlineAssets(cfg.pins, cfg.network);
  const tx = buildTrustlineTx(acc, assets, cfg.network);
  console.log(JSON.stringify({account: cfg.publicKey, trustlines: assets.map(a => `${a.asset.code}:${a.asset.issuer}`), unsignedHash: tx.hash().toString('hex')}));
  if (process.env.CONFIRM_TRUSTLINES !== 'yes') { console.log('dry run: set CONFIRM_TRUSTLINES=yes to sign and submit once'); process.exit(0); }
  const res = await c.submit(c.sign(tx));
  console.log(JSON.stringify(res));
  if (res.status !== 'SUCCESS') process.exit(1);
}
