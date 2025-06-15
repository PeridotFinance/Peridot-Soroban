import { NextRequest, NextResponse } from 'next/server';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);
const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const address = searchParams.get('address');

    if (!address) {
      return NextResponse.json(
        { error: 'Address parameter is required' },
        { status: 400 }
      );
    }

    if (!VAULT_CONTRACT_ID) {
      return NextResponse.json(
        { error: 'Vault contract not configured' },
        { status: 500 }
      );
    }

    console.log(`Getting pToken balance for ${address}`);
    
    // Query the vault contract for the user's pToken balance
    // Add a timestamp to ensure we get fresh data
    const timestamp = Date.now();
    const balanceCommand = `stellar contract invoke \\
      --id ${VAULT_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      get_ptoken_balance \\
      --user ${address}`;

    console.log(`Executing balance query at ${timestamp}:`, balanceCommand);
    const result = await execAsync(balanceCommand);
    console.log(`pToken balance query result at ${timestamp}:`, result.stdout);

    // Parse the balance from the output
    // The output is a simple string like "0"
    const balanceMatch = result.stdout.match(/^"?(\d+)"?\s*$/);
    let balance = '0';
    
    if (balanceMatch && balanceMatch[1]) {
      // Convert from contract units (9 decimals) to display units
      const balanceInUnits = parseInt(balanceMatch[1]) / 1000000000; // 9 decimals
      balance = balanceInUnits.toString();
    }
    
    return NextResponse.json({
      balance: balance,
      address: address,
      rawOutput: result.stdout
    });

  } catch (error) {
    console.error('pToken balance error:', error);
    
    // Check if it's a command execution error
    if (error instanceof Error && 'stdout' in error) {
      const execError = error as any;
      return NextResponse.json(
        { 
          error: 'Failed to query pToken balance',
          details: {
            stdout: execError.stdout,
            stderr: execError.stderr
          }
        },
        { status: 500 }
      );
    }

    return NextResponse.json(
      { error: 'Failed to fetch pToken balance' },
      { status: 500 }
    );
  }
} 