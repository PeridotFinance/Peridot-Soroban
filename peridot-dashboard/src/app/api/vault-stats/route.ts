import { NextRequest, NextResponse } from 'next/server';

const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;
const ALICE_SECRET_KEY = process.env.ALICE_SECRET_KEY!;

export async function GET(request: NextRequest) {
  try {
    if (!VAULT_CONTRACT_ID) {
      return NextResponse.json(
        { error: 'Vault contract not configured' },
        { status: 500 }
      );
    }

    if (!ALICE_SECRET_KEY) {
      return NextResponse.json(
        { error: 'Alice secret key not configured' },
        { status: 500 }
      );
    }

    console.log('Getting vault statistics from contract using SDK');

    // Use Stellar SDK directly instead of exec
    const StellarSdk = await import('@stellar/stellar-sdk');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Create Alice's keypair from secret key
    const aliceKeypair = StellarSdk.Keypair.fromSecret(ALICE_SECRET_KEY);
    
    // Build the contract calls
    const contract = new StellarSdk.Contract(VAULT_CONTRACT_ID);
    
    // Helper function to simulate contract call
    const simulateContractCall = async (functionName: string): Promise<string> => {
      const operation = contract.call(functionName);

      // We need to build a transaction to simulate the contract call
      // For read-only operations, we can use a dummy account
      const dummyAccount = new StellarSdk.Account(
        'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF', // Dummy account
        '0'
      );

      // Build the transaction for simulation
      const transaction = new StellarSdk.TransactionBuilder(dummyAccount, {
        fee: StellarSdk.BASE_FEE,
        networkPassphrase: StellarSdk.Networks.TESTNET,
      })
        .addOperation(operation)
        .setTimeout(30)
        .build();

      // Simulate the transaction to get the result
      const simResult = await rpc.simulateTransaction(transaction);
      
      // Check if simulation was successful
      if (StellarSdk.rpc.Api.isSimulationSuccess(simResult)) {
        const resultScVal = simResult.result?.retval;
        
        if (!resultScVal) {
          console.error(`No result returned from ${functionName} contract call`);
          return '0';
        }
        
        // Convert ScVal to JavaScript value
        const value = StellarSdk.scValToNative(resultScVal);
        return value.toString();
      } else {
        console.error(`Failed to get ${functionName}:`, simResult);
        return '0';
      }
    };

    // Execute all contract calls in parallel
    const [totalDepositedRaw, totalPTokensRaw, exchangeRateRaw] = await Promise.all([
      simulateContractCall('get_total_deposited'),
      simulateContractCall('get_total_ptokens'),
      simulateContractCall('get_exchange_rate')
    ]);

    console.log('Total deposited raw:', totalDepositedRaw);
    console.log('Total pTokens raw:', totalPTokensRaw);
    console.log('Exchange rate raw:', exchangeRateRaw);

    // Parse results - convert from contract units to display units
    const parseBalance = (value: string) => {
      const num = parseInt(value);
      if (isNaN(num)) return '0';
      return (num / 1000000000).toString(); // Convert from 9 decimals
    };

    const parseExchangeRate = (value: string) => {
      const num = parseInt(value);
      if (isNaN(num)) return '1.00';
      return (num / 1000000).toString(); // Convert from 6 decimals (per contract)
    };

    const stats = {
      totalDeposited: parseBalance(totalDepositedRaw),
      totalPTokens: parseBalance(totalPTokensRaw),
      exchangeRate: parseExchangeRate(exchangeRateRaw),
      userShare: '0' // Will be calculated on frontend
    };
    
    console.log('Vault stats computed:', stats);
    return NextResponse.json(stats);

  } catch (error) {
    console.error('Vault stats error:', error);
    
    return NextResponse.json(
      { error: 'Failed to fetch vault statistics' },
      { status: 500 }
    );
  }
} 