// Regression: storage keys of `data()` (chain.mjs). An all-letter strkey used to match /^[A-Z][A-Za-z]*$/ and was
// encoded as a Symbol (XDR write error: 56 bytes > 32), so PriceCache(<such a token>) could never be read.
import test from 'node:test';
import assert from 'node:assert/strict';
import {loadSdk, dataKey} from '../src/chain.mjs';

const S = loadSdk();
const LETTERS_ONLY = 'CCWNPXMQTZVYSLQACDUUZTTOECLQRVSNTHWUETNKPDMUXVHZBEZIUOVA';   // valid contract strkey, no digits
test('dataKey: every valid strkey is an Address (also an all-letter one); names are Symbols', () => {
  assert(S.StrKey.isValidContract(LETTERS_ONLY) && /^[A-Z]+$/.test(LETTERS_ONLY));
  const k = dataKey(S, ['PriceCache', LETTERS_ONLY]).vec();
  assert.equal(k[0].switch().name, 'scvSymbol'); assert.equal(k[0].sym().toString(), 'PriceCache');
  assert.equal(k[1].switch().name, 'scvAddress'); assert.equal(S.Address.fromScVal(k[1]).toString(), LETTERS_ONLY);
  assert.doesNotThrow(() => dataKey(S, ['PriceCache', LETTERS_ONLY]).toXDR());
  const g = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 7)), usdc = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
  for (const a of [g, usdc]) assert.equal(dataKey(S, ['X', a]).vec()[1].switch().name, 'scvAddress');
  assert.equal(dataKey(S, ['OracleMaxAgeMultiplier']).vec()[0].switch().name, 'scvSymbol');
  assert.throws(() => dataKey(S, ['not a symbol!']), /neither a strkey nor a symbol/);
  assert.throws(() => dataKey(S, ['A'.repeat(33)]), /neither/);
});
