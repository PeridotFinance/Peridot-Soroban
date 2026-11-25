// SKELETON. Job table for the Sentora group keeper. Each job is read-only unless it returns a `tx` intent; intents
// are signed and submitted only when DRY_RUN=false (signer, fee guard and confirmation polling are not implemented in
// the skeleton: see DESIGN.md section 14 for the contract each one must meet).
import {SENTORA} from './config.mjs';

export const JOBS = [
  {name: 'refresh_boosted_underlying', every: 'refreshMs', signs: true,
   what: 'per receipt with nonzero boosted shares: refresh_boosted_underlying() (11 entries, ~2.8 M instr). Health fails closed after 300 s.'},
  {name: 'peg', every: 'pegMs', signs: true,
   what: 'simulate peg_oracle.peg_status(); if at_peg differs from the controller cache or a mark-down started/ended, call controller.cache_price(PYUSD) so borrow gating and liquidation price follow at once; alert on missing sources or mark-down; alert on an UPWARD pool deviation (> +100 bps) for a guardian pause_borrow decision (the oracle never prices above $1).'},
  {name: 'sentora_monitor', every: 'monitorMs', signs: true,
   what: `read vault/escrow instances (getLedgerEntries): wasm hash must equal ${SENTORA.vaultWasm.slice(0, 8)}../${SENTORA.escrowWasm.slice(0, 8)}..; Paused must be false; TotalPrincipal must equal the vault SAC balance; admin ${SENTORA.admin.slice(0, 6)}.. signers/thresholds unchanged; watch vault/escrow upgrade, STATUS and update_admin events. On ANY deviation: adapter.emergency_exit(keeper) on every adapter (repeat until remaining_principal == 0), alert, and recommend supply caps to 0. If a balance drain is seen, exit_if_drained is also permissionless.`},
  {name: 'claim_rewards', every: 'claimMs', signs: true,
   what: 'for each new escrow epoch of each vault: fetch the adapter leaves + proofs (Sentora API, pending confirmation), simulate adapter.claim_rewards(epoch, params), submit if the simulated delta > fee. Rewards vest 7 days into supplier NAV.'},
  {name: 'ttl', every: 'ttlMs', signs: true,
   what: 'ExtendFootprintTTL for Sentora Deposit(adapter), vault instance+code, escrow instance+code, peg-oracle bump_ttl, adapter bump_ttl, controller/receipt bump_ttl; keep every entry > 30 days.'},
];

export function describe(cfg) {
  return JOBS.map(j => ({job: j.name, everyMs: cfg[j.every], signs: j.signs && !cfg.dryRun, what: j.what}));
}
