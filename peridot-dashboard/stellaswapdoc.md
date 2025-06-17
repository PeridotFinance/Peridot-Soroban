Overview of our SDK
Our public SDK provides functionality for integrating swaps on Moonbeam into any app or plugin. Moonbeam is the leading EVM parachain in Polkadot, and therefore taps on the best of Polkadot’s security and Ethereum’s convenience.

Developers and projects can integrate our our DEX for various applications that leverages on our liquidity. Our SDK features a range of useful documentation and APRs that entails technical specifications and code samples.

Description
The @stellaswap/swap-sdk provides functionality for integrating swap on Moonbeam into any app or plugin. StellaSwap SDK allows end-users to exchange tokens seamlessly on Moonbeam network.

Features
Uses state-of-the-art Hybrid Router

Utilizes Stable, V2, V3 AMMs

Error Handling

Installation
To install the package, use npm or yarn:

Copy
npm install @stellaswap/swap-sdk
# OR
yarn add @stellaswap/swap-sdk
Usage
Importing the SDK
First, import the SDK.

Copy
import stellaSwap from '@stellaswap/swap-sdk';
Allowance
This helps to check allowance of tokenAddress against spender, it will return allowed number if there is any.

Copy
const addresses = await stellaSwap.getAddresses();
const spender = addresses.permit2;
const allowance = await stellaSwap.checkAllowance(
  account,
  erc20Instance,
  spender
);
Copy
Response: 0
Approve

To perform approve pass desired value as amountIn and for unlimited approval use 0. In response, it returns transaction hash.

Copy
const addresses = await stellaSwap.getAddresses();
const spender = addresses.permit2;
const tx = await stellaSwap.approve(amountIn, erc20Instance, spender);
Get Quote
To get amountOut of a trade use getQuote. For account it can be null if user is not connected. For native asset pass ETH as token0Addr or token1Addr.

Copy
const quote = await stellaSwap.getQuote(
  token0Addr,
  token1Addr,
  amountIn,
  account,
  slippage
);
Response

To filter out amountOut use quote.result.amountOut. For the rest of the response, it includes;

Complete trade path.

Execution with commands and inputs

Swap
This can executes actual swap, for native asset pass ETH as token0Addr or token1Addr.

Copy
const tx = await stellaSwap.executeSwap(
  token0Addr,
  token1Addr,
  amountIn,
  signer,
  slippage
);
Swap Native to ERC20
To swap native to ERC20, pass token0Addr as ETH and token1Addr as erc20 address.

Copy
const encodedTxData = await stellaSwap.executeNativeSwap(
  token0Addr,
  token1Addr,
  amountIn,
  slippage,
  aggregatorContractInstance
);
const txParams = {
  from: ACCOUNT,
  value: amountIn,
  to: aggregatorContractInstance.address,
  data: encodedTxData,
  gasLimit: 1_500_000,
  gasPrice: await signer.getGasPrice(),
};

const txResponse = await signer.sendTransaction(txParams);
await txResponse.wait();
return txResponse.hash;
Swap ERC20 to ERC20
To swap native to ERC20, pass token0Addr as erc20 address and token1Addr as erc20 address.

Copy
const { signature, permit } = await permit2.getPermit2Signature(
  token0Addr,
  amountIn,
  signer
);

const encodedTxData = await stellaSwap.executeERC20Swap(
  token0Addr,
  token1Addr,
  amountIn,
  slippage,
  aggregatorContractInstance,
  permit,
  signature
);
const txParams = {
  to: aggregatorContractInstance.address,
  data: encodedTxData,
  gasLimit: 1_500_000,
  gasPrice: await signer.getGasPrice(),
};

const txResponse = await signer.sendTransaction(txParams);
await txResponse.wait();

console.log("Transaction hash:", txResponse.hash);
return txResponse.hash;
Example for getPermit2Signature
Copy
import {
  PermitTransferFrom,
  Witness,
  SignatureTransfer,
  MaxUint256,
} from "@uniswap/permit2-sdk";
import { joinSignature, splitSignature } from "@ethersproject/bytes";
import stellaSwap from "@stellaswap/swap-sdkÏ";

const permit2 = {
  async getPermit2Signature(token0Addr: string, amountIn: string, signer: any) {
    const addresses = await stellaSwap.getAddresses();
    const AGGREGATOR_ADDRESS = addresses.aggregator;
    const PERMIT2_ADDRESS = addresses.permit2;

    const spender = AGGREGATOR_ADDRESS;

    const permit: PermitTransferFrom = {
      permitted: {
        token: token0Addr,
        amount: amountIn,
      },
      spender: spender,
      nonce: await utils.calcNonces(signer),
      deadline: MaxUint256,
    };

    const witness: Witness = {
      witnessTypeName: "Witness",
      witnessType: { Witness: [{ name: "user", type: "address" }] },
      witness: { user: spender },
    };

    const { domain, types, values } = SignatureTransfer.getPermitData(
      permit,
      PERMIT2_ADDRESS,
      await signer.getChainId(),
      witness
    );

    const signature = await signer._signTypedData(domain, types, values);

    let { r, s, v } = splitSignature(signature);

    if (v == 0) v = 27;
    if (v == 1) v = 28;

    const joined = joinSignature({ r, s, v });

    return { signature: joined, permit, witness };
  },
};

export default permit2;
Dependencies
axios

@uniswap/permit2-sdk

Configuration
The SDK is pre-configured to be used with the Moonbeam mainnet and doesn't require an API key.

Error Handling
The SDK includes basic error handling. All methods return Promises, so you can use .catch() to handle errors as you see fit.

Copy
stellaSwap.checkAllowance(tokenAddress, signer, spender).catch((error) => {
  console.error("Check Allowance failed:", error.message);
});