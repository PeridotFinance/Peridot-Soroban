import { NextRequest, NextResponse } from 'next/server';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);
const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;

export async function GET(request: NextRequest) {
  try {
    if (!VAULT_CONTRACT_ID) {
      return NextResponse.json(
        { error: 'Vault contract not configured' },
        { status: 500 }
      );
    }

    console.log('Getting vault statistics from contract');
    
    // Query total deposited
    const totalDepositedCommand = `stellar contract invoke \\
      --id ${VAULT_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      get_total_deposited`;

    // Query total pTokens
    const totalPTokensCommand = `stellar contract invoke \\
      --id ${VAULT_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      get_total_ptokens`;

    // Query exchange rate
    const exchangeRateCommand = `stellar contract invoke \\
      --id ${VAULT_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      get_exchange_rate`;

    // Execute all commands in parallel
    const [totalDepositedResult, totalPTokensResult, exchangeRateResult] = await Promise.all([
      execAsync(totalDepositedCommand),
      execAsync(totalPTokensCommand),
      execAsync(exchangeRateCommand)
    ]);

    console.log('Total deposited result:', totalDepositedResult.stdout);
    console.log('Total pTokens result:', totalPTokensResult.stdout);
    console.log('Exchange rate result:', exchangeRateResult.stdout);

    // Parse results
    const parseBalance = (output: string) => {
      const match = output.match(/^"?(\d+)"?\s*$/);
      if (match && match[1]) {
        return (parseInt(match[1]) / 1000000000).toString(); // Convert from 9 decimals
      }
      return '0';
    };

    const parseExchangeRate = (output: string) => {
      const match = output.match(/^"?(\d+)"?\s*$/);
      if (match && match[1]) {
        return (parseInt(match[1]) / 1000000).toString(); // Convert from 6 decimals (per contract)
      }
      return '1.00';
    };

    const stats = {
      totalDeposited: parseBalance(totalDepositedResult.stdout),
      totalPTokens: parseBalance(totalPTokensResult.stdout),
      exchangeRate: parseExchangeRate(exchangeRateResult.stdout),
      userShare: '0' // Will be calculated on frontend
    };
    
    return NextResponse.json(stats);

  } catch (error) {
    console.error('Vault stats error:', error);
    
    // Check if it's a command execution error
    if (error instanceof Error && 'stdout' in error) {
      const execError = error as any;
      return NextResponse.json(
        { 
          error: 'Failed to query vault statistics',
          details: {
            stdout: execError.stdout,
            stderr: execError.stderr
          }
        },
        { status: 500 }
      );
    }

    return NextResponse.json(
      { error: 'Failed to fetch vault statistics' },
      { status: 500 }
    );
  }
} 