import { NextRequest, NextResponse } from 'next/server';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;
const ALICE_ADDRESS = process.env.NEXT_PUBLIC_ALICE_ADDRESS!;

export async function POST(request: NextRequest) {
  try {
    const { userAddress } = await request.json();

    if (!userAddress) {
      return NextResponse.json(
        { success: false, error: 'User address is required' },
        { status: 400 }
      );
    }

    if (!TOKEN_CONTRACT_ID || !ALICE_ADDRESS) {
      return NextResponse.json(
        { success: false, error: 'Server configuration error: Missing contract or Alice address' },
        { status: 500 }
      );
    }

    console.log(`Minting PDOT tokens for ${userAddress}`);
    
    // Step 1: Mint tokens to Alice's account
    // Amount: 1000000000000000 (1,000,000 PDOT with 9 decimals)
    const mintCommand = `stellar contract invoke \\
      --id ${TOKEN_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      mint \\
      --to ${ALICE_ADDRESS} \\
      --amount 1000000000000000`;

    console.log('Executing mint command...');
    const mintResult = await execAsync(mintCommand);
    console.log('Mint result:', mintResult.stdout);

    // Step 2: Transfer 1,000 PDOT tokens from Alice to user
    // Amount: 1000000000000 (1,000 PDOT with 9 decimals)
    const transferCommand = `stellar contract invoke \\
      --id ${TOKEN_CONTRACT_ID} \\
      --source alice \\
      --network testnet \\
      -- \\
      transfer \\
      --from ${ALICE_ADDRESS} \\
      --to ${userAddress} \\
      --amount 1000000000000`;

    console.log('Executing transfer command...');
    const transferResult = await execAsync(transferCommand);
    console.log('Transfer result:', transferResult.stdout);

    // Extract transaction hash from the output if available
    const transactionHashMatch = transferResult.stdout.match(/transaction: ([a-f0-9]+)/i);
    const transactionHash = transactionHashMatch ? transactionHashMatch[1] : 'completed';

    return NextResponse.json({
      success: true,
      message: 'Successfully minted and transferred 1,000 PDOT tokens',
      transactionHash: transactionHash,
      details: {
        mintOutput: mintResult.stdout,
        transferOutput: transferResult.stdout
      }
    });

  } catch (error) {
    console.error('Mint tokens error:', error);
    
    // Check if it's a command execution error
    if (error instanceof Error && 'stdout' in error) {
      const execError = error as any;
      return NextResponse.json(
        { 
          success: false, 
          error: 'Failed to execute Stellar commands',
          details: {
            stdout: execError.stdout,
            stderr: execError.stderr
          }
        },
        { status: 500 }
      );
    }

    return NextResponse.json(
      { success: false, error: 'Failed to mint tokens' },
      { status: 500 }
    );
  }
} 