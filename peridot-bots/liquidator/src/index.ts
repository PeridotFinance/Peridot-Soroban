import { rpc } from '@stellar/stellar-sdk';

import { loadConfig } from './config.js';
import { SorobanClient } from './contracts.js';
import { LiquidationBot } from './liquidationBot.js';

async function main(): Promise<void> {
  const config = loadConfig();
  const server = new rpc.Server(config.rpcUrl, { allowHttp: config.rpcUrl.startsWith('http://') });
  const contracts = new SorobanClient(server, config.networkPassphrase);
  const bot = new LiquidationBot(config, server, contracts);
  await bot.start();
}

main().catch(error => {
  console.error(error);
  process.exit(1);
});
