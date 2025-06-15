import { NextRequest, NextResponse } from 'next/server';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);
const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;

export async function POST(request: NextRequest) {
  try {
    const { userAddress, amount } = await request.json();

    if (!userAddress || !amount) {
      return NextResponse.json(
        { success: false, error: 'User address and amount are required' },
        { status: 400 }
      );
    }

    if (!VAULT_CONTRACT_ID) {
      return NextResponse.json(
        { success: false, error: 'Vault contract not configured' },
        { status: 500 }
      );
    }

    // Build the transaction and return it for signing
    console.log(`Building deposit transaction: ${amount} PDOT tokens for ${userAddress}`);
    
    // Convert amount to contract units (9 decimals)
    const amountInUnits = Math.floor(parseFloat(amount) * 1000000000);
    
    // Build the deposit transaction
    const depositCommand = `stellar contract invoke \\
      --id ${VAULT_CONTRACT_ID} \\
      --source ${userAddress} \\
      --network testnet \\
      --build-only \\
      -- \\
      deposit \\
      --user ${userAddress} \\
      --amount ${amountInUnits}`;

    console.log('Building deposit transaction...');
    const result = await execAsync(depositCommand);
    console.log('Build result:', result.stdout);

    // Extract the transaction XDR from the output
    const xdrMatch = result.stdout.match(/([A-Za-z0-9+/=]+)$/m);
    if (!xdrMatch) {
      throw new Error('Failed to extract transaction XDR');
    }

    return NextResponse.json({
      success: true,
      transactionXdr: xdrMatch[1],
      amount: amount
    });

  } catch (error) {
    console.error('Deposit error:', error);
    
    // Check if it's a command execution error
    if (error instanceof Error && 'stdout' in error) {
      const execError = error as any;
      return NextResponse.json(
        { 
          success: false, 
          error: 'Failed to build deposit transaction',
          details: {
            stdout: execError.stdout,
            stderr: execError.stderr
          }
        },
        { status: 500 }
      );
    }

    return NextResponse.json(
      { success: false, error: 'Failed to process deposit' },
      { status: 500 }
    );
  }
} 