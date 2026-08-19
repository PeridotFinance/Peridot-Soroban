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

    console.log(`Getting vault deposit balance for ${address}`);
    
    // Query the vault contract for the user's deposit balance
    const balanceCommand = `stellar contract invoke \\
      --id ${VAULT_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      get_user_balance \\
      --user ${address}`;

    const result = await execAsync(balanceCommand);
    console.log('Vault balance query result:', result.stdout);

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
    console.error('Vault balance error:', error);
    
    // Check if it's a command execution error
    if (error instanceof Error && 'stdout' in error) {
      const execError = error as any;
      return NextResponse.json(
        { 
          error: 'Failed to query vault balance',
          details: {
            stdout: execError.stdout,
            stderr: execError.stderr
          }
        },
        { status: 500 }
      );
    }

    return NextResponse.json(
      { error: 'Failed to fetch vault balance' },
      { status: 500 }
    );
  }
} 