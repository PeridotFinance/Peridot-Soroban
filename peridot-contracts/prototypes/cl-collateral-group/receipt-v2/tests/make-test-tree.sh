#!/usr/bin/env bash
# Materialise the NATIVE TEST tree for receipt StgV2 + the group controller (test-only; never a build
# input for a WASM candidate).
#
#   1. the overlay sources (apply-stgv2.py, staged_v2.rs, d3_chain.rs, controller apply-patches.py,
#      generic controller src) are taken from git revision $SRC_REV (default HEAD) via `git archive`,
#      so uncommitted work in the shared worktree never leaks in; set SRC_REV=WORKTREE to use the
#      working tree instead;
#   2. apply-stgv2.py over the run-5 base (target/clg/ws/receipt-base) into a fresh tree;
#   3. the workspace's generic simple-peridottroller sources are REPLACED by the generated group
#      controller sources (apply-patches.py, G0-G10+) and a `group` feature is added; the receipt's
#      dev-dependency enables it. The controller is a DEPENDENCY of the receipt test crate, so
#      `cfg!(test)` is false inside it and its real 24 h admin timelock applies;
#   4. the test modules of THIS directory (working tree) are copied in and declared `#[cfg(test)]`.
#
# usage (from peridot-contracts/):
#   bash prototypes/cl-collateral-group/receipt-v2/tests/make-test-tree.sh [out]   (default target/clg/ws/stgv2-test)
set -euo pipefail
cd "$(dirname "$0")/../../../.."
PC=$(pwd)
HERE=$PC/prototypes/cl-collateral-group/receipt-v2/tests
OUT=$PC/${1:-target/clg/ws/stgv2-test}
REV=${SRC_REV:-HEAD}
if [[ $REV == WORKTREE ]]; then
  SRC=$PC
else
  SRC=$PC/target/clg/ws/stgv2-src
  rm -rf "$SRC"; mkdir -p "$SRC"
  (cd .. && git archive "$REV" peridot-contracts/prototypes/cl-collateral-group/controller \
      peridot-contracts/prototypes/cl-collateral-group/receipt-v2 peridot-contracts/prototypes/cl-collateral-group/d3-chain \
      peridot-contracts/contracts/simple-peridottroller/src) | tar -x -C "$SRC" --strip-components=1
  echo "overlay sources: git $REV ($(cd .. && git rev-parse --short "$REV"))"
fi
CTRL=$SRC/prototypes/cl-collateral-group/controller/src
[[ -f $CTRL/contract.rs && $REV == WORKTREE ]] || python3 -I "$SRC/prototypes/cl-collateral-group/controller/apply-patches.py" >/dev/null
# apply-stgv2.py requires its output under <its root>/target.
STAGE=$SRC/target/stgv2-test
python3 -I "$SRC/prototypes/cl-collateral-group/receipt-v2/apply-stgv2.py" --base "$PC/target/clg/ws/receipt-base" --out "$STAGE"
if [[ $STAGE != "$OUT" ]]; then rm -rf "$OUT"; mkdir -p "$(dirname "$OUT")"; mv "$STAGE" "$OUT"; fi
P=$OUT/peridot-contracts/contracts
rm -f "$P"/simple-peridottroller/src/*.rs
cp "$CTRL"/{lib,constants,contract,events,reflector,storage}.rs "$P/simple-peridottroller/src/"
python3 -I - "$P" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
c = p / 'simple-peridottroller/Cargo.toml'; s = c.read_text()
assert s.count('lp-zero-peri = []\n') == 1
c.write_text(s.replace('lp-zero-peri = []\n', 'lp-zero-peri = []\ngroup = []\n'))
r = p / 'receipt-vault/Cargo.toml'; s = r.read_text()
old = 'simple-peridottroller = { path = "../simple-peridottroller", features = ["test-default-admin"] }'
assert s.count(old) == 1
r.write_text(s.replace(old, old.replace('["test-default-admin"]', '["test-default-admin", "group"]')))
# TEST-TREE-ONLY rewrite (the one change to candidate logic sources): soroban-sdk 25.3.1 testutils
# cannot convert `Option<contracttype struct>` fields to ScVal, so `Stg2Status` does not compile
# under `cargo test`. The view becomes a crate-internal accessor (not exported) with an identical
# body; nothing else in staged_v2.rs changes. The exported view is covered by the compiled rows.
v = p / 'receipt-vault/src/staged_v2.rs'; s = v.read_text()
for old, new in [('#[contracttype]\n#[derive(Clone, Debug, Eq, PartialEq)]\npub struct Stg2Status', '#[derive(Clone, Debug, Eq, PartialEq)]\npub struct Stg2Status'),
                 ('    pub fn stage2_status(e: Env) -> Stg2Status {', '    pub(crate) fn stage2_status(e: Env) -> Stg2Status {')]:
    assert s.count(old) == 1, old
    s = s.replace(old, new)
v.write_text(s)
l = p / 'receipt-vault/src/lib.rs'
l.write_text(l.read_text() + '\n#[cfg(test)]\nmod stgv2_test;\n#[cfg(test)]\nmod group_ctl_test;\n')
PY
cp "$HERE"/stgv2_test.rs "$HERE"/group_ctl_test.rs "$P/receipt-vault/src/"
echo "test tree: $OUT"
