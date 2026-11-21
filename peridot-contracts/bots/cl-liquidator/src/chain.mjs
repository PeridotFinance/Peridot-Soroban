// Read-only RPC helpers (simulateTransaction from a PUBLIC source address, getLedgerEntries) and the Testnet-only
// executor. The executor shells out to the Stellar CLI, which holds the identity's key; this process never sees it.
import {execFile as execFileCb} from 'node:child_process';
import {createRequire} from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export const TESTNET = 'Test SDF Network ; September 2015';
export const MAINNET = 'Public Global Stellar Network ; September 2015';
const here = path.dirname(fileURLToPath(import.meta.url));
// This package's own install (Droplet: `npm ci` from package-lock.json), the sibling bot (main checkout layout), then
// the main checkout seen from the .wt-cl-group worktree.
const SDK_DIRS = [path.join(here, '..'), path.join(here, '..', '..', 'cash-liquidator'), path.join(here, '..', '..', '..', '..', '..', 'peridot-contracts', 'bots', 'cash-liquidator')];

let sdk = null;
/** @stellar/stellar-sdk 16.3.1, resolved from STELLAR_SDK_DIR or a sibling install (no install in this package). */
export function loadSdk(dir = process.env.STELLAR_SDK_DIR) {
  if (sdk) return sdk;
  const d = dir || SDK_DIRS.find(x => fs.existsSync(path.join(x, 'node_modules', '@stellar', 'stellar-sdk')));
  if (!d) throw Error('set STELLAR_SDK_DIR to a directory whose node_modules holds @stellar/stellar-sdk 16.3.1');
  const req = createRequire(path.join(d, 'package.json'));
  const version = JSON.parse(fs.readFileSync(path.join(d, 'node_modules', '@stellar', 'stellar-sdk', 'package.json'), 'utf8')).version;
  if (version !== '16.3.1') throw Error(`@stellar/stellar-sdk ${version} found; 16.3.1 required`);
  sdk = req('@stellar/stellar-sdk');
  return sdk;
}

/** Hard live guard: Testnet passphrase only, exact match, and an identity NAME (never a key or seed phrase). */
export function assertLiveAllowed(cfg) {
  if (cfg.networkPassphrase === MAINNET || /Public Global/i.test(cfg.networkPassphrase ?? '')) throw Error('refused: --live is never allowed on Mainnet');
  if (cfg.networkPassphrase !== TESTNET) throw Error('refused: --live requires the exact Testnet passphrase');
  assertIdentityName(cfg.executorIdentity);
}
export function assertIdentityName(id) {
  if (typeof id !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/.test(id)) throw Error('refused: executorIdentity must be a CLI identity alias');
  if (/^[SGM][A-Z2-7]{55,}$/.test(id)) throw Error('refused: executorIdentity looks like a strkey; use an identity alias');
}

/** Child env without CLI overrides (STELLAR_ACCOUNT, _NETWORK, _RPC_URL, _SEND, _SIGN_WITH_*, fees, ...). Only
 *  STELLAR_CONFIG_HOME is kept so the CLI finds its identity store. No shell is used. */
export function cliEnv(src = process.env) {
  const env = {...src};
  for (const k of Object.keys(env)) if (/^(STELLAR|SOROBAN)_/.test(k) && k !== 'STELLAR_CONFIG_HOME') delete env[k];
  return env;
}

/** Executor: `stellar contract invoke --id <id> --source-account <alias> ... --send yes -- <fn> --arg value`. */
export function makeExecutor(cfg, {execFile = execFileCb} = {}) {
  assertLiveAllowed(cfg);
  return function invoke(contractId, fn, args = {}) {
    assertLiveAllowed(cfg);   // re-checked per call: config objects are mutable
    const argv = ['contract', 'invoke', '--id', contractId, '--source-account', cfg.executorIdentity,
      '--rpc-url', cfg.rpcUrl, '--network-passphrase', cfg.networkPassphrase, '--send', 'yes', '--', fn];
    for (const [k, v] of Object.entries(args)) argv.push(`--${k}`, String(v));
    return new Promise(resolve => {
      execFile('stellar', argv, {env: cliEnv(), timeout: 180_000}, (err, stdout, stderr) => resolve({ok: !err,
        // Timeout / signal: the transaction may still land. The caller leaves the journal intent unresolved.
        unknown: !!err && (err.killed || !!err.signal || typeof err.code !== 'number'),
        stdout: String(stdout ?? '').trim(), stderr: String(stderr ?? '').trim().slice(-2000), argv}));
    });
  };
}

/** Storage key Vec of parts: a valid G/C strkey is an Address, anything else a Symbol. A strkey can consist of letters
 *  only (e.g. CCWNPXMQ...UOVA), so the old "/^[A-Z][A-Za-z]*$/ = symbol" rule could encode an address as a symbol. */
export function dataKey(S, parts) {
  return S.xdr.ScVal.scvVec(parts.map(p => {
    const strkey = typeof p === 'string' && (S.StrKey.isValidEd25519PublicKey(p) || S.StrKey.isValidContract(p));
    if (strkey || typeof p !== 'string') return S.nativeToScVal(p, {type: 'address'});
    if (!/^[A-Za-z0-9_]{1,32}$/.test(p)) throw Error(`storage key part "${p.slice(0, 40)}" is neither a strkey nor a symbol`);
    return S.xdr.ScVal.scvSymbol(p);
  }));
}

export function makeReader(cfg) {
  const S = loadSdk();
  const rpc = new S.rpc.Server(cfg.rpcUrl, {allowHttp: cfg.rpcUrl.startsWith('http://')});
  const raw = async (method, params) => {
    const r = await (await fetch(cfg.rpcUrl, {method: 'POST', headers: {'content-type': 'application/json'},
      body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json();
    if (r.error) throw Error(JSON.stringify(r.error));
    return r.result;
  };
  const addr = a => S.nativeToScVal(a, {type: 'address'});
  const u64 = x => S.nativeToScVal(BigInt(x), {type: 'u64'});
  const u128 = x => S.nativeToScVal(BigInt(x), {type: 'u128'});
  const build = (contract, method, args) => new S.TransactionBuilder(new S.Account(cfg.liquidator, '0'), {fee: '100', networkPassphrase: cfg.networkPassphrase})
    .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(60).build();
  /** Simulate from the liquidator's PUBLIC address. Returns {ok, retval, error, code, metrics}. */
  async function simulate(contract, method, args = []) {
    const sim = await rpc.simulateTransaction(build(contract, method, args));
    if (!S.rpc.Api.isSimulationSuccess(sim)) {
      const error = String(sim.error).split('\n')[0].slice(0, 300), m = /Error\(Contract, #(\d+)\)/.exec(String(sim.error));
      return {ok: false, error, code: m ? Number(m[1]) : null};
    }
    const d = sim.transactionData.build(), r = d.resources(), fp = r.footprint();
    return {ok: true, retval: S.scValToNative(sim.result.retval), metrics: {cpu: Number(r.instructions()), readOnly: fp.readOnly().length,
      readWrite: fp.readWrite().length, auth: sim.result.auth?.length ?? 0, minResourceFee: String(sim.minResourceFee)}};
  }
  async function view(contract, method, args = []) {
    const s = await simulate(contract, method, args);
    if (!s.ok) throw Error(`${method}: ${s.error}`);
    return s.retval;
  }
  /** Raw persistent contract data, e.g. controller PriceCache(token); null when absent. */
  async function data(contract, keyParts) {
    const key = dataKey(S, keyParts);
    const lk = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(contract).toScAddress(), key,
      durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(lk);
    return r.entries.length ? S.scValToNative(r.entries[0].val.contractData().val()) : null;
  }
  /** Ledger close time (s): the clock the contracts use. */
  async function ledgerNow() { const l = await raw('getLatestLedger'); return {now: Number(l.closeTime), sequence: l.sequence}; }
  async function passphrase() { return (await raw('getNetwork')).passphrase; }
  return {simulate, view, data, ledgerNow, passphrase, addr, u64, u128};
}
