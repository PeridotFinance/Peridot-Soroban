import { NextRequest, NextResponse } from 'next/server';

const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;
const ALICE_ADDRESS = process.env.NEXT_PUBLIC_ALICE_ADDRESS!;
const ALICE_SECRET_KEY = process.env.ALICE_SECRET_KEY!;

export async function POST(request: NextRequest) {
  try {
    const { userAddress } = await request.json();

    console.log('=== MINT TOKENS REQUEST ===');
    console.log('User address:', userAddress);
    
    if (!userAddress) {
      return NextResponse.json(
        { success: false, error: 'User address is required' },
        { status: 400 }
      );
    }

    if (!TOKEN_CONTRACT_ID || !ALICE_ADDRESS || !ALICE_SECRET_KEY) {
      console.error('Missing environment variables');
      return NextResponse.json(
        { success: false, error: 'Server configuration error' },
        { status: 500 }
      );
    }

    // Import Stellar SDK
    const StellarSdk = await import('@stellar/stellar-sdk');
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Create Alice's keypair (Alice should be the admin who can mint)
    const aliceKeypair = StellarSdk.Keypair.fromSecret(ALICE_SECRET_KEY);
    
    // Amount to mint (1000 PDOT = 1000 * 10^9 contract units)
    const mintAmount = BigInt(1000) * BigInt(1000000000); // Use BigInt for precision
    
    console.log('Minting', mintAmount.toString(), 'contract units to', userAddress);
    
    // Get Alice's current account
    const aliceAccount = await rpc.getAccount(aliceKeypair.publicKey());
    
    // Create contract instance
    const contract = new StellarSdk.Contract(TOKEN_CONTRACT_ID);
    
    // Build mint operation: mint(to, amount)
    // This is safer than transfer since transfer has implementation issues
    const mintOperation = contract.call(
      'mint',
      StellarSdk.Address.fromString(userAddress).toScVal(),   // to (user)
      StellarSdk.nativeToScVal(mintAmount, { type: 'i128' }) // amount (use i128 for token amounts)
    );
    
    // Build transaction
    const transaction = new StellarSdk.TransactionBuilder(aliceAccount, {
      fee: (100_000).toString(), // Higher fee for contract calls
      networkPassphrase: StellarSdk.Networks.TESTNET,
    })
      .addOperation(mintOperation)
      .setTimeout(30)
      .build();
    
    // Prepare transaction for Soroban
    console.log('Preparing transaction...');
    const preparedTransaction = await rpc.prepareTransaction(transaction);
    
    // Sign with Alice's keypair
    console.log('Signing transaction...');
    preparedTransaction.sign(aliceKeypair);
    
    // Submit transaction
    console.log('Submitting transaction...');
    const result = await rpc.sendTransaction(preparedTransaction);
    
    if (result.status === 'ERROR') {
      console.error('Transaction failed:', result);
      return NextResponse.json(
        { success: false, error: `Mint failed: ${result.errorResult || 'Unknown error'}` },
        { status: 500 }
      );
    }
    
    console.log('✅ Mint successful! Hash:', result.hash);
    
    // Wait a moment for the transaction to be processed
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    return NextResponse.json({
      success: true,
      message: 'Successfully minted 1000 PDOT tokens',
      transactionHash: result.hash
    });
    
  } catch (error: any) {
    console.error('❌ Mint tokens error:', error);
    
    // Handle specific error patterns
    if (error.message?.includes('UnreachableCodeReached')) {
      return NextResponse.json(
        { success: false, error: 'Token contract has implementation issues - contact admin' },
        { status: 500 }
      );
    }
    
    if (error.message?.includes('0x7D82D5')) {
      return NextResponse.json(
        { success: false, error: 'Authorization failed - insufficient permissions' },
        { status: 500 }
      );
    }
    
    return NextResponse.json(
      { success: false, error: `Failed to mint tokens: ${error.message || error}` },
      { status: 500 }
    );
  }
} 