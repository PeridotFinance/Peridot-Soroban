import 'dotenv/config';

export interface MarketConfig {
  symbol: string;
  vaultId: string;
  decimals: number;
}

export interface BotConfig {
  rpcUrl: string;
  networkPassphrase: string;
  peridottrollerId: string;
  liquidatorSecret: string;
  markets: MarketConfig[];
  pollIntervalMs: number;
  borrowerRefreshMs: number;
  minShortfall: bigint;
  eventBacklog: number;
  eventPageSize: number;
}

const DEFAULT_MARKETS: MarketConfig[] = [
  {
    symbol: 'XLM',
    vaultId: 'CCBRKJ5ZZZB6A7GSAPVPDWFEOJXZZ43F65RL6NJGJX7AQJ2JS64DGU7G',
    decimals: 7,
  },
  {
    symbol: 'USDC',
    vaultId: 'CDNSMCOHX4NJTIYEILEVEBAS5LKPJRDH6CPLWJ4SQ2YUB4LVNQWPXG3L',
    decimals: 6,
  },
];

function parseMarkets(json?: string | null): MarketConfig[] {
  if (!json) {
    return DEFAULT_MARKETS;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch (error) {
    throw new Error(`MARKETS_JSON is not valid JSON: ${(error as Error).message}`);
  }

  if (!Array.isArray(parsed)) {
    throw new Error('MARKETS_JSON must be an array');
  }

  const markets: MarketConfig[] = [];
  for (const entry of parsed) {
    if (
      typeof entry !== 'object' ||
      entry === null ||
      typeof (entry as any).symbol !== 'string' ||
      typeof (entry as any).vaultId !== 'string'
    ) {
      throw new Error('MARKETS_JSON entries must include symbol and vaultId');
    }
    const decimals = Number((entry as any).decimals ?? 0);
    if (!Number.isInteger(decimals) || decimals < 0) {
      throw new Error(`Invalid decimals for market ${(entry as any).symbol}`);
    }
    markets.push({
      symbol: (entry as any).symbol,
      vaultId: (entry as any).vaultId,
      decimals,
    });
  }
  return markets;
}

export function loadConfig(): BotConfig {
  const networkPassphrase =
    process.env.NETWORK_PASSPHRASE ?? 'Test SDF Future Network ; October 2022';
  const rpcUrl = process.env.RPC_URL ?? 'https://soroban-testnet.stellar.org';
  const peridottrollerId =
    process.env.PERIDOTTROLLER_ID ??
    'CAWEZM3CRRMBUAGYMCCFHXI6ZKCLVMQTVE4LPXQCH7MM3ZU2PMQTKUXM';
  const liquidatorSecret = process.env.LIQUIDATOR_SECRET;

  if (!liquidatorSecret) {
    throw new Error('LIQUIDATOR_SECRET is required');
  }

  const markets = parseMarkets(process.env.MARKETS_JSON);

  const pollIntervalMs = Number(process.env.POLL_INTERVAL_MS ?? 5000);
  const borrowerRefreshMs = Number(process.env.BORROWER_REFRESH_MS ?? 15000);
  const minShortfallRaw = process.env.MIN_SHORTFALL ?? '0';
  const eventBacklog = Number(process.env.EVENT_BACKLOG ?? 50);
  const eventPageSize = Number(process.env.EVENT_PAGE_SIZE ?? 50);

  if (
    !Number.isInteger(pollIntervalMs) ||
    !Number.isInteger(borrowerRefreshMs) ||
    pollIntervalMs <= 0 ||
    borrowerRefreshMs <= 0
  ) {
    throw new Error('Invalid poll interval configuration');
  }
  if (!Number.isInteger(eventBacklog) || eventBacklog < 0) {
    throw new Error('EVENT_BACKLOG must be a non-negative integer');
  }
  if (!Number.isInteger(eventPageSize) || eventPageSize <= 0) {
    throw new Error('EVENT_PAGE_SIZE must be a positive integer');
  }

  let minShortfall: bigint;
  try {
    minShortfall = BigInt(minShortfallRaw);
  } catch (error) {
    throw new Error(`MIN_SHORTFALL must be an integer: ${(error as Error).message}`);
  }
  if (minShortfall < 0n) {
    throw new Error('MIN_SHORTFALL cannot be negative');
  }

  return {
    rpcUrl,
    networkPassphrase,
    peridottrollerId,
    liquidatorSecret,
    markets,
    pollIntervalMs,
    borrowerRefreshMs,
    minShortfall,
    eventBacklog,
    eventPageSize,
  };
}
