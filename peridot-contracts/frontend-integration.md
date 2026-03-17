# Peridot Frontend Integration Guide

This document explains how the frontend can interact with the Peridot Soroban contracts in a Compound-style UI. It assumes you are comfortable with TypeScript/React (or Astro) and have the Stellar CLI installed.

## 1. High-Level Architecture

- **Controller (`SimplePeridottroller`)** – central risk engine. Contract ID: `CCBAEMMG4STILW6SYTNCIVG44OF4TQDDCYPU7GS3ZOEKLTC75ONTLCI2`
- **ReceiptVault (XLM market)** – contract ID: `CCHBN5RRP7KH4O7ICSIQTSYFFZBYFEBCF35UOQBGDI7GZZKKWXWVLLPX`
- **ReceiptVault (USDT market)** – contract ID: `CBP2U7FVTQ2EIAQ474CTYN74KCEU6YLCCGH6KRY2RAMQEDSKREKSAGSO`
- **Jump Rate Model** – contract ID: `CCIDO7HBNBPUKFWEI3PRA6O6QU2JXUKVIZAERCZWBNGGK7LO7MFBKKOA`
- **Peridot Reward Token (`P`)** – contract ID: `CBCA56UIBQA3WT2JUIIG2BHW325CMLNAC7CKL33T37GHN25RCGR6SXPB`
- **Mock USDT (open mint)** – contract ID: `CDBWTU527WNACRCET2NF6RZFQ3WAPJOQM3OQ5VLUNHJRDQ6ICVO2JTJP`
- **Reflector Oracle** – contract ID: `CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63`
- **Swap Adapter (Aquarius)** – contract ID: `CAGLARN3MUMRGCRNKXZ3SH7NVCZ3P3CDGHL2FQEEXIC4MPAGTQTACY6S`
- **Margin Controller (true margin trading)** – contract ID: `CAZQWGJDKG2JQYV66VV3ONBDLYAE77YVKSBUNWUY7MV6WVLLHT4URFX7`
- **Aquarius Router (AMM)** – `CBCFTQSPDBAIZ6R6PJQKSQWKNKWH2QIV3I4J72SHWBIK3ADRRAM5A6GD`

Your frontend will mainly call the controller and the vault contracts. The controller handles account liquidity checks, oracle pricing, and incentives; each vault exposes ERC20-like `deposit`, `withdraw`, `borrow`, `repay`, and `transfer` entrypoints.

## 2. Generated Clients (Recommended)

Use `stellar contract bindings` to generate TypeScript clients for each contract:

```bash
# From the repo root after building WASMs
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/receipt_vault.wasm \
  --out-dir web/src/contracts/receiptVault

stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/simple_peridottroller.wasm \
  --out-dir web/src/contracts/controller

stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/peridot_token.wasm \
  --out-dir web/src/contracts/peridotToken
```

Each generated package exports a class with strongly typed methods. Import them into your frontend and pass the contract ID plus RPC details.

## 2.1. Smart Accounts (Basic)

Peridot supports Basic Smart Accounts (contract accounts) that can sign and enforce policy via `__check_auth`. Use the factory to create a smart account for a user, then use that smart account address as the `user`/`borrower` in vault and margin calls.

Factory calls:
- `initialize(admin)`
- `set_wasm_hash(account_type=Basic, wasm_hash)`
- `create_account(config, salt)` → returns smart account address

Basic account constructor config:
- `owner` (user Address)
- `signer` (ed25519 public key BytesN<32>)
- `peridottroller` (SimplePeridottroller contract ID)
- `margin_controller` (MarginController contract ID)

Once created, the smart account intercepts `require_auth` and verifies signatures. The protocol does not require changes.

## 2.2. Passkeys + Smart Accounts (Recommended UX)

**Important:** the Basic Smart Account verifies **ed25519** signatures only. WebAuthn/passkeys produce **P‑256** (ES256) signatures, so passkeys cannot directly sign Soroban auth payloads. The recommended pattern is:

1) **Generate an ed25519 keypair in the frontend.**
2) **Protect the ed25519 private key using a passkey** (WebAuthn) by encrypting it with a passkey‑derived key.
3) Use the decrypted ed25519 private key to sign the Soroban auth payload when the user approves an action.

This gives you passkey UX + secure key storage while still satisfying the contract’s ed25519 verification.

### 2.2.1. Key Model

- `ed25519_public_key` → stored on-chain as `signer` in the smart account.
- `ed25519_private_key` → stored **encrypted** in local storage/IndexedDB.
- `passkey credential` → used to decrypt the private key (local user presence/biometric).

### 2.2.2. Registration Flow (Passkey + Ed25519)

**Step A: Create a passkey**

Use a WebAuthn helper (e.g., `@simplewebauthn/browser` or `webauthn-json`) to create a passkey credential:

```ts
// pseudo-code
const passkey = await webauthnCreate({
  rpId: location.hostname,
  userId: userAddress, // stable per user
  userName: userAddress,
  challenge: randomBytes(32),
});
// store passkey.id in local storage
```

**Step B: Create an ed25519 keypair and encrypt it**

```ts
// pseudo-code
const { publicKey, privateKey } = ed25519Generate();

// derive a symmetric key using the passkey (see below)
const kek = await deriveKeyFromPasskey(passkey);
const encryptedPrivateKey = await aesGcmEncrypt(kek, privateKey);

saveLocal({
  passkeyId: passkey.id,
  ed25519PublicKey: publicKey,
  ed25519PrivateKeyEncrypted: encryptedPrivateKey,
});
```

**Deriving a key from passkey**

There are two safe approaches:

1) **WebAuthn PRF extension** (preferred if supported by browser/OS).
2) **Server-assisted key wrapping** (passkey signs a challenge → server derives a KEK and returns an encrypted key wrapper).

If you don’t have PRF, use a server-assisted flow. The key point: **never store the ed25519 private key unencrypted**.

### 2.2.3. Smart Account Creation (Factory)

Use the ed25519 public key from the previous step as the `signer`.

```ts
const factory = new SmartAccountFactoryClient({
  contractId: FACTORY_ID,
  networkPassphrase,
  rpcUrl,
});

const salt = hash(ownerAddress); // must match factory requirements
const config = {
  account_type: { tag: "Basic" },
  owner: ownerAddress,
  signer: ed25519PublicKey, // BytesN<32>
  peridottroller: CONTROLLER_ID,
  margin_controller: MARGIN_CONTROLLER_ID,
};

const smartAccount = await factory.create_account({ config, salt }, { signer: freighterSigner });
```

### 2.2.4. Signing Flow (Passkey‑Protected Ed25519)

When a transaction requires the smart account to authorize:

1) Build the Soroban transaction normally.
2) Extract the **auth payload hash** that the smart account expects.
3) Use passkey to decrypt the ed25519 private key.
4) Sign the payload hash with ed25519.
5) Attach the signature(s) to the auth entries and submit.

```ts
// pseudo-code
const { tx, authEntries } = await buildSorobanTx(...);
const payloadHash = authEntries[0].signature_payload; // 32-byte hash

const kek = await deriveKeyFromPasskey(passkey);
const ed25519PrivateKey = await aesGcmDecrypt(kek, encryptedPrivateKey);

const sig = ed25519Sign(payloadHash, ed25519PrivateKey);

authEntries[0].signatures = [{
  public_key: ed25519PublicKey,     // BytesN<32>
  signature: sig,                   // BytesN<64>
}];

const sent = await submitSorobanTx(tx, authEntries);
```

### 2.2.5. Adding / Rotating Signers

Use passkey to decrypt the key, then call:

```ts
await smartAccount.add_signer({ owner: ownerAddress, signer: newEd25519Pub }, { signer: freighterSigner });
```

For rotation:
1) Add new signer.
2) Remove old signer.
3) Re-encrypt private key as needed.

### 2.2.6. UX Checklist

- **Lock/unlock**: require passkey authentication to sign any smart‑account auth payload.
- **Device loss**: provide a recovery flow (owner account can add/remove signers).
- **Multi‑device**: use multiple passkeys, each wrapping the same ed25519 key (or separate signers).

### 2.2.7. Security Notes

- Passkeys **do not sign Soroban payloads directly** (current contract supports ed25519 only).
- Treat passkeys as a **secure unlock mechanism** for the ed25519 private key.
- Never store ed25519 private keys in plaintext.
- Require user presence/biometrics for every smart‑account signature.
- Always clear decrypted private keys from memory after signing.

### 2.2.8. Soroban Auth Payload Extraction (Detailed)

Soroban uses **auth entries** in the transaction to represent `require_auth` calls. You must sign the **auth payload hash** that corresponds to the smart account’s `__check_auth`.

High‑level steps:
1) Build the transaction with contract call(s).
2) Simulate to receive `auth` entries (the hash is provided per entry).
3) Sign each entry that targets the smart account.
4) Attach signatures and submit.

Pseudo‑code using `@stellar/stellar-sdk`:

```ts
import { SorobanRpc, TransactionBuilder, Networks } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server(rpcUrl);
const account = await server.getAccount(feePayer);

const tx = new TransactionBuilder(account, {
  fee: "100000",
  networkPassphrase: Networks.TESTNET,
})
  .addOperation(op)
  .setTimeout(120)
  .build();

const sim = await server.simulateTransaction(tx);
// sim.result.auth is an array of AuthorizationEntry XDRs
const authEntries = sim.result?.auth ?? [];

// find entries where the smart account is the authorizer
for (const entry of authEntries) {
  const payloadHash = entry.signaturePayload(); // 32‑byte hash
  const sig = ed25519Sign(payloadHash, ed25519PrivateKey);
  entry.signatures.push({
    publicKey: ed25519PublicKey,
    signature: sig,
  });
}

const prepared = SorobanRpc.assembleTransaction(tx, sim);
prepared.sign(feePayerKeypair);
const send = await server.sendTransaction(prepared);
```

If you use generated contract clients, they typically wrap the simulate/sign/submit steps. You still need to inject signatures for smart‑account auth entries before submission.

### 2.2.9. Passkey PRF Flow (If Supported)

If your WebAuthn stack supports the PRF extension, you can derive a **stable key‑encryption‑key (KEK)** without server assistance:

```ts
// 1) Create passkey with PRF enabled
const credential = await navigator.credentials.create({
  publicKey: {
    // ... standard WebAuthn fields ...
    extensions: { prf: { eval: { first: randomBytes(32) } } },
  },
});

// 2) Later, derive KEK on sign
const assertion = await navigator.credentials.get({
  publicKey: {
    // ... standard request fields ...
    extensions: { prf: { eval: { first: randomBytes(32) } } },
  },
});

// Use PRF output as KEK to decrypt ed25519 private key
const kek = assertion.getClientExtensionResults().prf?.results?.first;
```

If PRF is not available, use a server‑assisted wrapping flow:
- Client generates ed25519 keypair
- Client asks server to wrap the private key using a passkey‑validated challenge
- Server returns encrypted blob

### 2.2.10. Multi‑Signer / Recovery Pattern

Recommended policy:
- The EOA (wallet) is the **owner**.
- The smart account has **one or more ed25519 signers**.

Recovery options:
1) Owner adds a new signer, then removes the old signer.
2) Owner updates peridottroller/margin controller if needed.
3) Optional: a time‑locked recovery flow in the UI (off‑chain).

### 2.2.11. Salt Guidance (Factory)

The factory derives a salted address internally using `owner` + user‑provided salt.
Recommended client flow:

```ts
const userSalt = randomBytes(32);
// pass raw userSalt to factory; it derives final salt internally
await factory.create_account({ config, salt: userSalt });
```

Never use a fixed or predictable salt in the UI.

## 3. RPC Configuration

Use the public testnet RPC endpoint:

```ts
const rpcUrl = "https://soroban-testnet.stellar.org";
const networkPassphrase = "Test SDF Future Network ; October 2022";
```

When using Freighter, you can derive the user’s public key and sign transactions locally. The generated clients provide helpers to build and submit transactions; otherwise you can roll your own using `@stellar/stellar-sdk`.

## 4. Core Interaction Flows

### 4.1. Read Market Data (no signature)

- Controller

  - `get_price_usd(token: Address)` → current USD price scaled by `10^decimals`
  - `account_liquidity(account: Address)` → [liquidity, shortfall] in USD
  - `get_market_cf(market: Address)` → collateral factor (scaled 1e6)

- Vault
  - `get_exchange_rate()` → underlying per pToken (scaled 1e6)
  - `get_ptoken_balance(account: Address)`
  - `get_user_borrow_balance(account: Address)`
  - `get_available_liquidity()` → currently borrowable assets

Use these to build supply/borrow tables, account dashboards, and health meters.

### 4.2. Deposit + Enter Market

```ts
await vault.deposit({ user, amount }, { signer });
await controller.enter_market({ user, market: vaultId }, { signer });
```

### 4.3. Borrow

```ts
await vault.borrow({ user, amount }, { signer });
```

### 4.4. Repay

```ts
await vault.repay({ user, amount }, { signer });
```

### 4.5. Margin Open (Aquarius Swap Adapter)

Peridot uses **Aquarius** on testnet for swaps. You must supply `swaps_chain` from the Aquarius route API (or a known pool route) and enforce your own slippage.

```ts
await margin.open_position({
  user,
  collateral_asset: usdc,
  debt_asset: xlm,
  collateral_amount: "10000000",
  leverage: 2,
  swaps_chain,
  amount_out_min,
}, { signer });
```

### 4.6. Margin Close (Aquarius)

```ts
await margin.close_position({
  user,
  position_id,
  swaps_chain,
  amount_out_min,
}, { signer });
```

### 4.7. Liquidation (Aquarius)

```ts
await margin.liquidate_position({
  liquidator,
  position_id,
  swaps_chain,
  amount_out_min,
}, { signer });
```

### 4.8. Using Smart Accounts as `user`

If the user is a smart account, pass that address as `user` and attach the smart‑account auth signatures as described in 2.2.8.

```ts
await vault.deposit({ user: smartAccount, amount }, { signer: feePayer });
```

## 5. Full End‑to‑End Smart Account Flow (Passkeys + Ed25519)

This is an end‑to‑end example using **Soroban RPC** and manual auth signature injection.
It shows:
1) building the transaction
2) simulating to get auth entries
3) signing smart‑account auth with ed25519
4) submitting

```ts
import {
  SorobanRpc,
  TransactionBuilder,
  Networks,
  xdr,
} from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server(rpcUrl);

// fee payer is any funded account (could be the owner)
const feePayer = ownerAddress;
const feePayerAccount = await server.getAccount(feePayer);

// 1) Build the transaction
const tx = new TransactionBuilder(feePayerAccount, {
  fee: "100000",
  networkPassphrase: Networks.TESTNET,
})
  .addOperation(op) // e.g., margin.open_position
  .setTimeout(120)
  .build();

// 2) Simulate to obtain auth entries
const sim = await server.simulateTransaction(tx);
const authEntries = sim.result?.auth ?? [];

// 3) Sign smart‑account auth entries (ed25519)
for (const entry of authEntries) {
  // Filter to only the smart account we control
  const auth = xdr.SorobanAuthorizationEntry.fromXDR(entry, "base64");
  const addr = auth.switch().address();
  if (addr.toString() !== smartAccountAddress) continue;

  const payloadHash = auth.signaturePayload(); // 32 bytes
  const ed25519PrivateKey = await decryptWithPasskey();
  const sig = ed25519Sign(payloadHash, ed25519PrivateKey);

  auth.v0().signature().signatures().push(
    xdr.ScVal.scvMap([
      xdr.ScMapEntry({
        key: xdr.ScVal.scvSymbol("public_key"),
        val: xdr.ScVal.scvBytes(ed25519PublicKey),
      }),
      xdr.ScMapEntry({
        key: xdr.ScVal.scvSymbol("signature"),
        val: xdr.ScVal.scvBytes(sig),
      }),
    ])
  );
}

// 4) Assemble & submit
const prepared = SorobanRpc.assembleTransaction(tx, sim);
prepared.sign(feePayerKeypair);
const send = await server.sendTransaction(prepared);
```

Notes:
- Use the same ed25519 public key stored in the smart account signer list.
- Always require passkey user‑presence before decrypting the private key.
- Clear decrypted private keys from memory after signing.

## 6. Aquarius Swap Adapter (Routing + swaps_chain)

Peridot expects Aquarius `swap_chained` data:

```ts
type SwapHop = [string[], string /* pool_id */, string /* token_out */];
type SwapsChain = SwapHop[];
```

Recommended flow:
1) Fetch route from Aquarius **Find Path API** (off‑chain).
2) Build `swaps_chain` for the swap adapter call.
3) Provide `amount_out_min` from the quote minus slippage.

Example:

```ts
const swaps_chain: SwapsChain = [
  [
    [USDC, XLM],
    POOL_ID,   // BytesN<32> hex string
    XLM
  ],
];

await margin.open_position({
  user,
  collateral_asset: USDC,
  debt_asset: XLM,
  collateral_amount: "10000000",
  leverage: 2,
  swaps_chain,
  amount_out_min: "9800000",
}, { signer });
```

### 6.1. Off‑chain Find‑Path API (Recommended)

Aquarius provides an HTTP API to compute the **best route** (path + expected output).
You call it **off‑chain**, then pass the resulting `swaps_chain` into the on‑chain adapter.

Example request (pseudo‑code):

```ts
const res = await fetch("https://api.aqua.network/api/external/v1/find-path/", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    tokenIn: USDC,
    tokenOut: XLM,
    amountIn: "10000000",
    network: "testnet",
    // optional: slippageBps, maxHops, etc
  }),
});

const route = await res.json();
const swaps_chain = route.swaps_chain;
const amount_out = route.amount_out;

const amount_out_min = applySlippage(amount_out, 50); // 0.50% slippage
```

Use this for:
- **Open/close** positions
- **Liquidations** (best price reduces bad debt risk)

**Response shape tip:** Aquarius returns a JSON object that includes the swap route and a quoted output.
Capture and log the full response once in dev, then map to:
- `swaps_chain`: array of hops
- `amount_out`: expected output
- optionally `path` or `pools` if the API exposes them

### 6.1.1. Route → swaps_chain conversion helper

If the API returns `path` or `pools`, you may need to convert it to `swaps_chain`.
Use this helper pattern (replace field names to match the actual response):

```ts
type SwapHop = [string[], string, string]; // [path, poolId, tokenOut]
type SwapsChain = SwapHop[];

function routeToSwapsChain(route: any): SwapsChain {
  // Example: route.hops = [{ path: [USDC, XLM], pool_id, token_out }]
  return route.hops.map((h: any) => [h.path, h.pool_id, h.token_out]);
}
```

### 6.1.2. Slippage helper

```ts
function applySlippage(amountOut: string, bps: number): string {
  const n = BigInt(amountOut);
  const min = n - (n * BigInt(bps) / BigInt(10_000));
  return min.toString();
}
```

### 6.1.3. Minimal route cache (recommended)

Cache a good route per pair (USDC↔XLM) for 10–30 seconds to reduce API calls:

```ts
const cache = new Map<string, { ts: number; route: any }>();

async function getRoute(tokenIn: string, tokenOut: string, amountIn: string) {
  const key = `${tokenIn}-${tokenOut}`;
  const now = Date.now();
  const cached = cache.get(key);
  if (cached && now - cached.ts < 15_000) return cached.route;
  const route = await fetchRoute(tokenIn, tokenOut, amountIn);
  cache.set(key, { ts: now, route });
  return route;
}
```

### 6.2. Hardcoded Fallback Route (No API)

If the API is unavailable, you can use **fixed routes** for high‑liquidity pairs.
Example: USDC ↔ XLM direct pool.

```ts
const swaps_chain: SwapsChain = [
  [[USDC, XLM], POOL_ID_USDC_XLM, XLM],
];

// You must set a conservative amount_out_min, or you risk swap failure.
```

Guidelines:
- Keep the fallback list small (USDC/XLM only).
- Update pool IDs when liquidity migrates.
- Only use fallback when API is down.

## 7. Example: Margin Open With Smart Account

```ts
// fee payer can be owner or relayer
const feePayer = ownerAddress;

// build op: open_position with user=smartAccountAddress
const op = margin.open_positionOp({
  user: smartAccountAddress,
  collateral_asset: USDC,
  debt_asset: XLM,
  collateral_amount: "10000000",
  leverage: 2,
  swaps_chain,
  amount_out_min: "9800000",
});

// build tx, simulate, inject smart‑account auth, submit
```

## 8. Example: Margin Close With Smart Account

```ts
const op = margin.close_positionOp({
  user: smartAccountAddress,
  position_id,
  swaps_chain,
  amount_out_min: "9900000",
});
```

## 9. Example: Liquidation With Smart Account as Borrower

Liquidator signs as themselves; borrower is the smart account:

```ts
const op = margin.liquidate_positionOp({
  liquidator: liquidatorAddress,
  position_id,
  swaps_chain,
  amount_out_min: "9500000",
});
```

### 4.2. Supply / Mint pTokens

1. User approves vault to withdraw underlying
2. Call `deposit(user: Address, amount: u128)` on the vault

Example (TypeScript using generated client):

```ts
const vault = new ReceiptVaultClient({
  contractId: "CCBRKJ5ZZZ...",
  networkPassphrase,
  rpcUrl,
});

await vault.deposit(
  { user: userAddress, amount: BigInt(1_000_000) },
  { signer: freighterSigner }
);
```

### 4.3. Redeem / Withdraw

Call `withdraw(user, ptoken_amount)` on the same vault. The method handles collateral checks via the controller (if the user has borrow positions).

### 4.4. Borrow

Call `borrow(user, amount)` on the vault. The controller’s `hypothetical_liquidity` is invoked internally to ensure the resulting position is safe. Display the user’s max borrow by reading `preview_borrow_max`.

### 4.5. Repay

Call `repay(user, amount)` with the underlying asset amount. For “max repay,” pass a high number or call `preview_repay_cap` on the controller first.

### 4.6. Rewards

To show accrued rewards and allow claiming:

```ts
const controller = new SimplePeridottrollerClient({
  contractId: "CAWEZ...",
  networkPassphrase,
  rpcUrl,
});

const accrued = await controller.get_accrued({ user: userAddress });
await controller.claim_self({ user: userAddress }, { signer: freighterSigner });
```

`claim_self` mints `P` tokens to the user via the controller→token hook.

## 5. Supported Assets

Store the known asset addresses in your config file:

```ts
export const markets = [
  {
    symbol: "XLM",
    vaultId: "CCPQYPFNAGQPQTMPAEBGNPNSQJ4FAJYPX6WLYBKE5SO5ZONXANCUEYE7",
    underlying: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
    decimals: 7,
  },
  {
    symbol: "USDT",
    vaultId: "CDM37TMZO2QQQP6CIMU7E6OIBR6IQMM46P5PCSQ5D7AX6GMEFQX7NTKL",
    underlying: "CBX3DOZH4HUR3EJS6LAKHXN6RARXKMUT33OUMVVSUW5HCXEIECD4WT75",
    decimals: 6,
  },
];
```

Use these IDs for price queries and UI labels.

## 6. Health Factor Calculation

To display the Compound-style health factor:

1. Fetch collateral USD and borrow USD via `account_liquidity`
2. Compute `healthFactor = collateralUSD / borrowUSD`
3. Highlight positions where `borrowUSD > collateralUSD` (shortfall > 0).

For per-market views, call `portfolio(user)` on the controller to get `[market, ptoken_balance, borrow_balance, collateral_usd, borrow_usd]` for each entered market.

## 7. Oracle Considerations

The controller calls the Reflector oracle synchronously. If `get_price_usd` returns `None`, the UI should warn that the oracle is stale or missing. Poll once per page load and cache responses for a few seconds; the oracle updates roughly every 5 minutes.

## 8. Leveraged Margin (True Borrow/Swap)

Margin trading uses real vault borrows + AMM swaps (Aquarius) coordinated by `margin-controller` and `swap-adapter`.

Core calls:
- `open_position(user, collateral_asset, base_asset, collateral_amount, leverage, side, swaps_chain, amount_with_slippage)`
- `open_position_no_swap(user, collateral_asset, debt_asset, collateral_amount, borrow_amount, leverage, side)`
- `open_position_no_swap_short(user, collateral_asset, debt_asset, collateral_amount, borrow_amount, leverage)`
- `close_position(user, position_id, swaps_chain, amount_with_slippage)`
- `liquidate_position(liquidator, position_id)` (liquidation uses peridottroller liquidation + vaults)

Key notes for frontend engineers:
- **`swaps_chain`** is the Aquarius route payload from find‑path (strict‑send).
- **`amount_with_slippage`** is the quoted output after applying slippage tolerance.
- **`side`** is `Long` or `Short`.

Recommended UX flow for open/close (Aquarius):
1. Use Aquarius find‑path off‑chain to get `swaps_chain`.
2. Compute `amount_with_slippage`.
3. Call `open_position`/`close_position` with `swaps_chain` and `amount_with_slippage`.

Budget‑safe open flow (recommended for testnet limits):
1. User swaps USDC→XLM directly via Aquarius (outside MarginController).
2. Call `open_position_no_swap` to deposit XLM collateral and borrow USDC (no router call).

Budget‑safe short flow:
1. Call `open_position_no_swap_short` to deposit USDC collateral and borrow XLM.
2. Swap XLM→USDC via Aquarius in a separate tx.

## 10. Boosted Markets (DeFindex Vaults)

Peridot vaults can be configured to forward deposits into a DeFindex vault to earn external yield. This is opt‑in per ReceiptVault via admin.

Admin calls:
- `set_boosted_vault(admin, defindex_vault_address)`
- `get_boosted_vault()` (view)

Behavior:
- On deposit, underlying is forwarded to the DeFindex vault (single‑asset) and the ReceiptVault keeps DeFindex shares.
- On withdraw, if local cash is insufficient, the ReceiptVault withdraws from DeFindex first.
- `get_total_underlying` includes the DeFindex‑managed portion for correct exchange‑rate math.

Note: DeFindex vault addresses are in `addresses.md` (BLEND USDC vault for USDC, XLM vault for XLM).

Non‑boosted markets:
- Do nothing. If `set_boosted_vault` is never called, the ReceiptVault behaves as a normal market.

CLI example (two‑step, USDC → XLM → open):
```bash
# Step 1: swap USDC -> XLM via Aquarius off‑chain find‑path + swap_chained
# (frontend/bot fetches swaps_chain from Aquarius find‑path API)
stellar contract invoke --id "$SWAP_ADAPTER" --source-account dev --network testnet -- \
  swap_chained \
  --user "USER_ADDRESS" \
  --swaps_chain "$SWAPS_CHAIN" \
  --token_in "USDC_CONTRACT" \
  --in_amount 10000000 \
  --out_min 1

# Step 2: open position without swap
stellar contract invoke --id "MARGIN_CONTROLLER_ID" --source-account dev --network testnet -- \
  open_position_no_swap \
  --user "USER_ADDRESS" \
  --collateral_asset "XLM_CONTRACT" \
  --debt_asset "USDC_CONTRACT" \
  --collateral_amount 15123603 \
  --borrow_amount 2000000 \
  --leverage 2 --side Long
```

Notes:
- Set `amount_out_min` based on your slippage tolerance.
- Use the swap output as `collateral_amount`.

Liquidation helper (peridottroller):
- `repay_on_behalf_for_liquidator(borrower, repay_market, repay_amount, liquidator)`
  - Use this for liquidation bots that want to repay a borrow without seizing collateral.
  - The peridottroller authorizes the vault call via contract auth, so no vault allowlist is required.

Example CLI (repay-on-behalf helper):
```bash
stellar contract invoke \
  --id "$PERIDOTTROLLER" \
  --source-account "$LIQUIDATOR" \
  $NETWORK -- \
  repay_on_behalf_for_liquidator \
  --borrower "$BORROWER" \
  --repay_market "$REPAY_VAULT" \
  --repay_amount 40000000 \
  --liquidator "$LIQUIDATOR"
```
Notes:
- `repay_amount` is in underlying token base units (e.g., 6 decimals for USDT).
- The liquidator must hold sufficient underlying balance in the repay market token.

When wiring the frontend, store:
- `marginControllerId` (from `scripts/deploy_margin_controller_testnet.sh` output)
- `swapAdapterId`
- `aquariusRouterId`
- `peridottrollerId`
- `oracleId` (Reflector)
- vault IDs and underlying token IDs (as above)

## 9. Testing

- Use the scripts (`build_wasm.sh`, `deploy_testnet.sh`, `verify_testnet.sh`) to keep the contracts in sync with the frontend environment.
- For local development, you can target the sandbox (`deploy_sandbox.sh`) and point the frontend RPC to `http://localhost:8000`.
- Consider adding mock data layers for unit testing UI components without hitting the network.

## 10. Checklist

- [ ] Configure RPC + network passphrase
- [ ] Generate TypeScript bindings for controller and vaults
- [ ] Copy contract IDs and asset metadata into the frontend config
- [ ] Implement wallet connection (Freighter recommended)
- [ ] Create reusable hooks/services for `deposit`, `withdraw`, `borrow`, `repay`, `claim`
- [ ] Render health factor and liquidity data via controller reads
- [ ] Handle oracle stale/missing states gracefully
- [ ] If using margin, add multi-op transaction builder for swaps
- [ ] Test flows on testnet before mainnet deployment

With these pieces, you can recreate a Compound-like experience on Stellar Soroban, showing supply/borrow balances, USD valuations, and liquidation status in real time.
