import { NextRequest, NextResponse } from 'next/server';

const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;

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

    if (!TOKEN_CONTRACT_ID) {
      return NextResponse.json(
        { error: 'Token contract not configured' },
        { status: 500 }
      );
    }

    console.log(`[Fallback API] Getting PDOT token balance for ${address}`);
    console.log(`[Fallback API] Using TOKEN_CONTRACT_ID: ${TOKEN_CONTRACT_ID}`);
    
    // Use Stellar SDK directly for contract interaction
    const StellarSdk = await import('@stellar/stellar-sdk');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Build the contract call
    const contract = new StellarSdk.Contract(TOKEN_CONTRACT_ID);
    
    // Build the operation to call balance function
    const operation = contract.call(
      'balance',
      StellarSdk.Address.fromString(address).toScVal() // id parameter
    );

    // Build transaction for simulation
    const dummyAccount = new StellarSdk.Account(
      'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      '0'
    );

    const transaction = new StellarSdk.TransactionBuilder(dummyAccount, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: StellarSdk.Networks.TESTNET,
    })
      .addOperation(operation)
      .setTimeout(30)
      .build();

    // Simulate the transaction
    const simResult = await rpc.simulateTransaction(transaction);
    
    console.log(`[Fallback API] Simulation result:`, simResult);
    
    if (StellarSdk.rpc.Api.isSimulationSuccess(simResult)) {
      const resultScVal = simResult.result?.retval;
      
      if (!resultScVal) {
        return NextResponse.json({
          balance: '0',
          address: address,
          error: 'No result from contract',
          debug: { simResult }
        });
      }
      
      const balanceValue = StellarSdk.scValToNative(resultScVal);
      const balanceInUnits = parseInt(balanceValue.toString()) / 1000000000;
      
      return NextResponse.json({
        balance: balanceInUnits.toString(),
        address: address,
        debug: {
          rawValue: balanceValue.toString(),
          contractId: TOKEN_CONTRACT_ID,
          success: true
        }
      });
    } else {
      return NextResponse.json({
        balance: '0',
        address: address,
        error: 'Contract simulation failed',
        debug: { simResult }
      });
    }

  } catch (error) {
    console.error('[Fallback API] Token balance error:', error);
    
    return NextResponse.json({
      balance: '0',
      error: `Failed to fetch token balance: ${error}`,
      debug: {
        errorName: error instanceof Error ? error.name : 'Unknown',
        errorMessage: error instanceof Error ? error.message : String(error),
        contractId: TOKEN_CONTRACT_ID
      }
    }, { status: 500 });
  }
} 