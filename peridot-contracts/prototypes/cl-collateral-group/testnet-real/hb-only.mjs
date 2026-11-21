// TESTNET: run ONLY the bot's heartbeat module (no plan loop, so it relies on its self-observation) with an
// ordered RPC list whose first entry may be dead. usage: node hb-only.mjs <bot-config.json> <hb-alias> <hb-G> <durationSec> <rpc1> [rpc2...]
import {createRequire} from 'node:module';
import path from 'node:path';
import {makeHeartbeat, failoverRpc} from '../../../bots/cl-liquidator/src/heartbeat.mjs';
import {loadConfig} from '../../../bots/cl-liquidator/src/config.mjs';
import {openJournal} from '../../../bots/cl-liquidator/src/journal.mjs';
import {makeCliSigner} from '../../../bots/cl-liquidator/src/main.mjs';
import {loadSdk} from '../../../bots/cl-liquidator/src/chain.mjs';
const [cfgFile, alias, acct, dur, ...urls] = process.argv.slice(2);
const base = loadConfig(cfgFile);
const cfg = {...base, heartbeatIdentity: alias, heartbeatAccount: acct, heartbeatPeriodMs: 12000};
const S = loadSdk(), out = o => console.log(JSON.stringify({t: new Date().toISOString(), ...o}));
const rpc = failoverRpc(urls.map(u => new S.rpc.Server(u, {allowHttp: u.startsWith('http://')})), out);
const j = openJournal(path.join(base.dataDir, '..', 'hb-only'));
const hb = makeHeartbeat({S, rpc, cfg, journal: j, sign: makeCliSigner(cfg, alias), out}).start();
out({msg: 'hb-only start', acct, urls});
setTimeout(() => { hb.stop(); out({msg: 'hb-only stop', stats: hb.stats()}); process.exit(0); }, Number(dur) * 1000);
