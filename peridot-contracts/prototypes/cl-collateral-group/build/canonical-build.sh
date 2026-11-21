#!/usr/bin/env bash
# Canonical release build of the CL collateral group (vault v2, receipt StgV2, group controller).
# usage (from peridot-contracts/): bash prototypes/cl-collateral-group/build/canonical-build.sh <commit> <dest dir>
#
# * Sources come ONLY from `git archive <commit>` (never the shared worktree), extracted under <dest>/src.
# * The receipt base is the frozen run-5 staged tree of the main checkout (pinned below by a whole-tree
#   digest of its receipt-vault sources; apply-stgv2.py also pins staged.rs).
# * OpenZeppelin is always the canonical absolute path below (the receipt bytes depend on it).
# * Each contract gets its own fresh CARGO_TARGET_DIR under <dest>, `--locked`, offline, Mainnet admin.
# No network, signing or deployment. Prints one line per artifact: <bytes> <sha256> <name>.
set -euo pipefail
commit=$1; dest=$2
OZ=/Users/joshua/Peridot/Peridot-Soroban/openzeppelin-stellar-contracts
BASE=/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-staged-market-integration/run-5/staged
BASE_DIGEST=${BASE_DIGEST:-}
A=GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP
here="$(cd "$(dirname "$0")/../../.." && pwd)"
[[ ! -e "$dest" ]] || { echo "refusing: $dest exists" >&2; exit 1; }
mkdir -p "$dest/src/peridot-contracts" "$dest/art"
dest="$(cd "$dest" && pwd)"
git -C "$here" archive "$commit" prototypes/cl-collateral-group contracts/simple-peridottroller | tar -x -C "$dest/src/peridot-contracts"
P="$dest/src/peridot-contracts/prototypes/cl-collateral-group"
digest=$(cd "$BASE/peridot-contracts/contracts/receipt-vault" && find src Cargo.toml -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -c1-64)
[[ -z "$BASE_DIGEST" || "$digest" == "$BASE_DIGEST" ]] || { echo "receipt base drifted: $digest" >&2; exit 1; }
E=(env CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true RECEIPT_VAULT_INIT_ADMIN=$A SIMPLE_PERIDOTTROLLER_INIT_ADMIN=$A MARGIN_CONTROLLER_INIT_ADMIN=$A
   SWAP_ADAPTER_INIT_ADMIN=$A JUMP_RATE_MODEL_INIT_ADMIN=$A AQUARIUS_LP_VAULT_INIT_ADMIN=$A)
# vault v2
(cd "$P/vault-v2" && "${E[@]}" CARGO_TARGET_DIR="$dest/t-vault" stellar contract build --manifest-path Cargo.toml --package cl-share-vault \
  --locked --features cl-pyusd-vault-b --out-dir "$dest/art/vault-v2" >/dev/null 2>&1)
# group controller (patch script writes into the archived copy only)
python3 -I "$P/controller/apply-patches.py" >/dev/null
(cd "$P/controller" && "${E[@]}" CARGO_TARGET_DIR="$dest/t-ctrl" stellar contract build --manifest-path Cargo.toml --locked --features group \
  --out-dir "$dest/art/ctrl-group" >/dev/null 2>&1)
# receipt StgV2 (Mainnet features), OZ rewritten to the canonical absolute path
python3 -I "$P/receipt-v2/apply-stgv2.py" --base "$BASE" --out "$dest/src/peridot-contracts/target/rv2" >/dev/null
R="$dest/src/peridot-contracts/target/rv2"
sed -i '' "s#path = \"../openzeppelin-stellar-contracts/#path = \"$OZ/#" "$R/peridot-contracts/Cargo.toml"
grep -q "$OZ/packages/tokens" "$R/peridot-contracts/Cargo.toml"
rm -rf "$R/openzeppelin-stellar-contracts"
(cd "$R/peridot-contracts" && "${E[@]}" CARGO_TARGET_DIR="$dest/t-receipt" stellar contract build --package receipt-vault --locked \
  --features stg-mainnet-accepted,stg2-mainnet-accepted --out-dir "$dest/art/receipt-v2" >/dev/null 2>&1)
echo "commit $(git -C "$here" rev-parse "$commit") receipt-base-digest $digest oz $(git -C "$OZ" rev-parse HEAD)"
for f in vault-v2/cl_share_vault.wasm receipt-v2/receipt_vault.wasm ctrl-group/simple_peridottroller.wasm; do
  echo "$(stat -f %z "$dest/art/$f") $(shasum -a 256 "$dest/art/$f" | cut -c1-64) $f"
done
