import { NextRequest, NextResponse } from 'next/server';

const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;
const ALICE_ADDRESS = process.env.NEXT_PUBLIC_ALICE_ADDRESS!;
const ALICE_SECRET_KEY = process.env.ALICE_SECRET_KEY!;

export async function POST(request: NextRequest) {
  try {
    const { userAddress } = await request.json();

    if (!userAddress) {
      return NextResponse.json(
        { success: false, error: 'User address is required' },
        { status: 400 }
      );
    }

    if (!TOKEN_CONTRACT_ID || !ALICE_ADDRESS || !ALICE_SECRET_KEY) {
      return NextResponse.json(
        { success: false, error: 'Server configuration error: Missing contract ID, Alice address, or Alice secret key' },
        { status: 500 }
      );
    }

    console.log(`Minting and transferring PDOT tokens to ${userAddress}`);
    
    // Use Stellar SDK directly
    const StellarSdk = await import('@stellar/stellar-sdk');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Create Alice's keypair from secret key
    const aliceKeypair = StellarSdk.Keypair.fromSecret(ALICE_SECRET_KEY);
    
    // Get Alice's account
    const aliceAccount = await rpc.getAccount(ALICE_ADDRESS);
    
    // Build the token contract
    const tokenContract = new StellarSdk.Contract(TOKEN_CONTRACT_ID);
    
    console.log('Building transfer operation directly (skipping mint for now)...');
    
    // Let's try just doing a transfer from Alice to user directly
    // Amount: 1000000000000 (1,000 PDOT with 9 decimals)
    const transferOperation = tokenContract.call(
      'transfer',
      StellarSdk.Address.fromString(ALICE_ADDRESS).toScVal(), // from parameter
      StellarSdk.Address.fromString(userAddress).toScVal(), // to parameter
      StellarSdk.nativeToScVal(BigInt('1000000000000'), { type: 'i128' }) // amount parameter using i128
    );

    // Build the transfer transaction
    const transferTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: StellarSdk.Networks.TESTNET,
    })
      .addOperation(transferOperation)
      .setTimeout(30)
      .build();

    console.log('Simulating transfer transaction first...');
    
    // First simulate the transaction to check for errors
    const transferSimResult = await rpc.simulateTransaction(transferTransaction);
    console.log('Transfer simulation result:', transferSimResult);
    
    if (!StellarSdk.rpc.Api.isSimulationSuccess(transferSimResult)) {
      return NextResponse.json(
        { 
          success: false, 
          error: `Transfer simulation failed: ${JSON.stringify(transferSimResult)}`,
          details: { transferSimResult }
        },
        { status: 500 }
      );
    }

    console.log('Preparing transfer transaction...');
    
    // Prepare the transfer transaction for Soroban
    const preparedTransferTransaction = await rpc.prepareTransaction(transferTransaction);
    
    // Sign the transfer transaction with Alice's keypair
    preparedTransferTransaction.sign(aliceKeypair);
    
    console.log('Submitting transfer transaction...');
    
    // Submit the transfer transaction
    const transferResult = await rpc.sendTransaction(preparedTransferTransaction);
    console.log('Transfer transaction result:', transferResult);
    
    if (transferResult.status === 'ERROR') {
      return NextResponse.json(
        { 
          success: false, 
          error: `Transfer transaction failed: ${transferResult.errorResult || 'Unknown error'}`,
          details: { transferResult }
        },
        { status: 500 }
      );
    }

    // Wait for the transfer transaction to be confirmed
    await new Promise(resolve => setTimeout(resolve, 2000));

    return NextResponse.json({
      success: true,
      message: 'Successfully transferred 1,000 PDOT tokens (mint skipped due to contract limitations)',
      transactionHash: transferResult.hash,
      details: {
        transferResult: transferResult,
        note: 'Mint operation was skipped due to contract function not being available. Only transfer was performed.'
      }
    });

  } catch (error) {
    console.error('Transfer tokens error:', error);
    
    return NextResponse.json(
      { 
        success: false, 
        error: `Failed to transfer tokens: ${error instanceof Error ? error.message : 'Unknown error'}`,
        details: { error: error instanceof Error ? error.stack : error }
      },
      { status: 500 }
    );
  }
} 