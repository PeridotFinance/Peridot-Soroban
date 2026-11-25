import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as S from '@stellar/stellar-sdk';
import {trustlineAssets, buildTrustlineTx} from '../src/trustlines.mjs';

const pins = JSON.parse(fs.readFileSync(new URL('../config/mainnet.json', import.meta.url), 'utf8'));

test('trustlines: USDC and EURC classic assets map to the pinned SACs; a wrong issuer is refused', () => {
  const a = trustlineAssets(pins, pins.network);
  assert.deepEqual(a.map(x => x.market), ['USDC', 'EURC']);
  const tx = buildTrustlineTx(new S.Account(S.Keypair.random().publicKey(), '1'), a, pins.network);
  assert.deepEqual(tx.operations.map(o => [o.type, o.line.code]), [['changeTrust', 'USDC'], ['changeTrust', 'EURC']]);
  const bad = {...pins, markets: {...pins.markets, USDC: {...pins.markets.USDC, underlying: pins.markets.EURC.underlying}}};
  assert.throws(() => trustlineAssets(bad, pins.network), /USDC: classic asset does not match/);
});
