import { Keypair, rpc } from '@stellar/stellar-sdk';

import { loadConfig } from './config.js';
import { SorobanClient } from './contracts.js';
import { LiquidationBot } from './liquidationBot.js';
import { RebalanceBot } from './rebalanceBot.js';

async function main(): Promise<void> {
  const config = loadConfig();
  const server = new rpc.Server(config.rpcUrl, { allowHttp: config.rpcUrl.startsWith('http://') });
  const contracts = new SorobanClient(server, config.networkPassphrase);

  const liquidationBot = new LiquidationBot(config, server, contracts);

  const hasRebalanceConfig = Object.keys(config.underlyingTokens).length > 0;
  if (hasRebalanceConfig) {
    const signer = Keypair.fromSecret(config.liquidatorSecret);
    const rebalanceBot = new RebalanceBot(
      contracts,
      signer,
      config.markets,
      config.underlyingTokens,
      config.rebalancePollIntervalMs,
    );
    // Run both bots concurrently; a failure in either rejects the main promise.
    await Promise.all([liquidationBot.start(), rebalanceBot.run()]);
  } else {
    await liquidationBot.start();
  }
}

main().catch(error => {
  console.error(error);
  process.exit(1);
});
