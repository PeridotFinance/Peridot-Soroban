import { NextRequest, NextResponse } from 'next/server';

const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;
const ALICE_ADDRESS = process.env.NEXT_PUBLIC_ALICE_ADDRESS!;
const ALICE_SECRET_KEY = process.env.ALICE_SECRET_KEY!;

// Add health check for environment
export async function GET() {
  return NextResponse.json({
    status: 'ok',
    environment: process.env.NODE_ENV,
    hasTokenContract: !!TOKEN_CONTRACT_ID,
    hasAliceAddress: !!ALICE_ADDRESS,
    hasAliceSecret: !!ALICE_SECRET_KEY,
    timestamp: new Date().toISOString()
  });
}

export async function POST(request: NextRequest) {
  try {
    const { userAddress } = await request.json();

    // Comprehensive environment validation
    const envCheck = {
      TOKEN_CONTRACT_ID: !!TOKEN_CONTRACT_ID,
      ALICE_ADDRESS: !!ALICE_ADDRESS,
      ALICE_SECRET_KEY: !!ALICE_SECRET_KEY,
      NODE_ENV: process.env.NODE_ENV,
      VERCEL_ENV: process.env.VERCEL_ENV || 'not-vercel'
    };
    
    console.log('Environment check:', envCheck);

    if (!userAddress) {
      return NextResponse.json(
        { success: false, error: 'User address is required' },
        { status: 400 }
      );
    }

    if (!TOKEN_CONTRACT_ID || !ALICE_ADDRESS || !ALICE_SECRET_KEY) {
      return NextResponse.json(
        { 
          success: false, 
          error: 'Server configuration error: Missing environment variables',
          details: {
            hasTokenContract: !!TOKEN_CONTRACT_ID,
            hasAliceAddress: !!ALICE_ADDRESS,
            hasAliceSecret: !!ALICE_SECRET_KEY,
            environment: process.env.NODE_ENV
          }
        },
        { status: 500 }
      );
    }

    console.log(`[${new Date().toISOString()}] Starting token transfer to ${userAddress}`);
    console.log(`Environment: ${process.env.NODE_ENV}, Vercel: ${process.env.VERCEL_ENV}`);
    
    // Use Stellar SDK directly
    const StellarSdk = await import('@stellar/stellar-sdk');
    console.log('Stellar SDK loaded successfully');
    
    // Use SorobanRpc server for contract interactions with timeout
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org', {
      allowHttp: false,
      timeout: 30000, // 30 second timeout
    });
    console.log('RPC server initialized');
    
    // Create Alice's keypair from secret key
    let aliceKeypair;
    try {
      aliceKeypair = StellarSdk.Keypair.fromSecret(ALICE_SECRET_KEY);
      console.log('Alice keypair created successfully');
    } catch (error) {
      console.error('Failed to create Alice keypair:', error);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Invalid Alice secret key configuration',
          details: { error: error instanceof Error ? error.message : 'Unknown keypair error' }
        },
        { status: 500 }
      );
    }
    
    // Get Alice's account
    let aliceAccount;
    try {
      aliceAccount = await rpc.getAccount(ALICE_ADDRESS);
      console.log('Alice account loaded, sequence:', aliceAccount.sequenceNumber);
    } catch (error) {
      console.error('Failed to load Alice account:', error);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Failed to load Alice account from network',
          details: { 
            error: error instanceof Error ? error.message : 'Unknown account error',
            aliceAddress: ALICE_ADDRESS
          }
        },
        { status: 500 }
      );
    }
    
    // Build the token contract
    const tokenContract = new StellarSdk.Contract(TOKEN_CONTRACT_ID);
    console.log('Token contract initialized with ID:', TOKEN_CONTRACT_ID);
    
    console.log('Building transfer operation...');
    
    // Transfer 1,000 PDOT tokens from Alice to user
    // Amount: 1000000000000 (1,000 PDOT with 9 decimals)
    const transferOperation = tokenContract.call(
      'transfer',
      StellarSdk.Address.fromString(ALICE_ADDRESS).toScVal(), // from parameter
      StellarSdk.Address.fromString(userAddress).toScVal(), // to parameter
      StellarSdk.nativeToScVal(BigInt('1000000000000'), { type: 'i128' }) // amount parameter using i128
    );

    // Build the transfer transaction with higher timeout for deployment
    const transferTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: StellarSdk.Networks.TESTNET,
    })
      .addOperation(transferOperation)
      .setTimeout(60) // Increased timeout for deployment environments
      .build();

    console.log('Transfer transaction built');
    
    // First simulate the transaction to check for errors
    let transferSimResult;
    try {
      console.log('Simulating transfer transaction...');
      transferSimResult = await rpc.simulateTransaction(transferTransaction);
      console.log('Transfer simulation completed');
    } catch (error) {
      console.error('Simulation failed:', error);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Transaction simulation failed',
          details: { 
            error: error instanceof Error ? error.message : 'Unknown simulation error',
            phase: 'simulation'
          }
        },
        { status: 500 }
      );
    }
    
    if (!StellarSdk.rpc.Api.isSimulationSuccess(transferSimResult)) {
      console.error('Simulation unsuccessful:', transferSimResult);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Transfer simulation failed - contract call unsuccessful',
          details: { 
            simulationResult: transferSimResult,
            phase: 'simulation_validation'
          }
        },
        { status: 500 }
      );
    }

    console.log('Simulation successful, preparing transaction...');
    
    // Prepare the transfer transaction for Soroban
    let preparedTransferTransaction;
    try {
      preparedTransferTransaction = await rpc.prepareTransaction(transferTransaction);
      console.log('Transaction prepared successfully');
    } catch (error) {
      console.error('Transaction preparation failed:', error);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Transaction preparation failed',
          details: { 
            error: error instanceof Error ? error.message : 'Unknown preparation error',
            phase: 'preparation'
          }
        },
        { status: 500 }
      );
    }
    
    // Sign the transfer transaction with Alice's keypair
    try {
      preparedTransferTransaction.sign(aliceKeypair);
      console.log('Transaction signed successfully');
    } catch (error) {
      console.error('Transaction signing failed:', error);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Transaction signing failed',
          details: { 
            error: error instanceof Error ? error.message : 'Unknown signing error',
            phase: 'signing'
          }
        },
        { status: 500 }
      );
    }
    
    console.log('Submitting transfer transaction...');
    
    // Submit the transfer transaction
    let transferResult;
    try {
      transferResult = await rpc.sendTransaction(preparedTransferTransaction);
      console.log('Transfer transaction submitted, status:', transferResult.status);
    } catch (error) {
      console.error('Transaction submission failed:', error);
      return NextResponse.json(
        { 
          success: false, 
          error: 'Transaction submission failed',
          details: { 
            error: error instanceof Error ? error.message : 'Unknown submission error',
            phase: 'submission'
          }
        },
        { status: 500 }
      );
    }
    
    if (transferResult.status === 'ERROR') {
      console.error('Transaction failed on network:', transferResult);
      return NextResponse.json(
        { 
          success: false, 
          error: `Transfer transaction failed on network: ${transferResult.errorResult || 'Unknown network error'}`,
          details: { 
            transferResult,
            phase: 'network_execution'
          }
        },
        { status: 500 }
      );
    }

    console.log('Transaction successful, hash:', transferResult.hash);
    
    // Wait for the transfer transaction to be confirmed
    await new Promise(resolve => setTimeout(resolve, 2000));

    return NextResponse.json({
      success: true,
      message: 'Successfully transferred 1,000 PDOT tokens',
      transactionHash: transferResult.hash,
      details: {
        transferResult: transferResult,
        environment: process.env.NODE_ENV,
        timestamp: new Date().toISOString()
      }
    });

  } catch (error) {
    console.error('Unexpected error in mint-tokens API:', error);
    
    // Check if this is a network timeout or similar
    const errorMessage = error instanceof Error ? error.message : 'Unknown error';
    const errorStack = error instanceof Error ? error.stack : 'No stack trace';
    
    return NextResponse.json(
      { 
        success: false, 
        error: `Unexpected error: ${errorMessage}`,
        details: { 
          error: errorMessage,
          stack: errorStack,
          environment: process.env.NODE_ENV,
          timestamp: new Date().toISOString()
        }
      },
      { status: 500 }
    );
  }
} 