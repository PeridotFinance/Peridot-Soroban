import { Horizon } from '@stellar/stellar-sdk';
import { 
  isConnected, 
  getAddress, 
  signTransaction, 
  requestAccess, 
  getNetwork 
} from '@stellar/freighter-api';

// Constants
const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;
const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;
const ALICE_ADDRESS = process.env.NEXT_PUBLIC_ALICE_ADDRESS!;

// Network configuration
const server = new Horizon.Server('https://horizon-testnet.stellar.org');

export interface WalletInfo {
  isConnected: boolean;
  address: string;
  xlmBalance: string;
  testTokenBalance: string; // PDOT tokens in wallet (from token contract)
  pTokenBalance: string;    // pTokens earned from vault (from vault contract)
}

export interface VaultStats {
  totalDeposited: string;
  totalPTokens: string;
  exchangeRate: string;
  userShare: string;
}

// Wallet connection functions
export async function connectFreighter(): Promise<{ success: boolean; address?: string; error?: string }> {
  try {
    const connectionResult = await isConnected();
    
    if (!connectionResult.isConnected) {
      return { success: false, error: 'Freighter is not installed' };
    }

    const accessResult = await requestAccess();
    
    if (accessResult.error) {
      return { success: false, error: accessResult.error };
    }

    return { success: true, address: accessResult.address };
  } catch (error) {
    return { success: false, error: `Connection failed: ${error}` };
  }
}

export async function getWalletAddress(): Promise<string | null> {
  try {
    const result = await getAddress();
    return result.error ? null : result.address;
  } catch (error) {
    console.error('Error getting wallet address:', error);
    return null;
  }
}

// Balance functions
export async function getXLMBalance(address: string): Promise<string> {
  try {
    const account = await server.loadAccount(address);
    const xlmBalance = account.balances.find(
      (balance: any) => balance.asset_type === 'native'
    );
    return xlmBalance ? parseFloat(xlmBalance.balance).toFixed(2) : '0.00';
  } catch (error) {
    console.error('Error fetching XLM balance:', error);
    return '0.00';
  }
}

// For now, we'll use API routes for contract interactions
export async function getTokenBalance(address: string): Promise<string> {
  // First try the direct SDK approach with enhanced logging
  try {
    console.log(`Fetching PDOT token balance for address: ${address}`);
    
    // Check if TOKEN_CONTRACT_ID is available
    if (!TOKEN_CONTRACT_ID) {
      console.error('TOKEN_CONTRACT_ID is not defined in environment variables');
      throw new Error('TOKEN_CONTRACT_ID not available');
    }
    
    console.log(`Using TOKEN_CONTRACT_ID: ${TOKEN_CONTRACT_ID}`);
    
    // Use Stellar SDK directly instead of API route
    const StellarSdk = await import('@stellar/stellar-sdk');
    console.log('Stellar SDK imported successfully');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    console.log('RPC server created');
    
    // Validate address format
    try {
      StellarSdk.Address.fromString(address);
      console.log('Address format validated');
    } catch (addressError) {
      console.error('Invalid address format:', addressError);
      throw new Error(`Invalid address format: ${addressError}`);
    }
    
    // Build the contract call
    const contract = new StellarSdk.Contract(TOKEN_CONTRACT_ID);
    console.log('Contract instance created');
    
    // Build the operation to call balance function
    const operation = contract.call(
      'balance',
      StellarSdk.Address.fromString(address).toScVal() // id parameter
    );
    console.log('Contract operation created');

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
    
    console.log('Transaction built for simulation');

    // Simulate the transaction to get the result
    console.log('Starting transaction simulation...');
    const simResult = await rpc.simulateTransaction(transaction);
    console.log('Simulation completed:', simResult);
    
    // Check if simulation was successful
    if (StellarSdk.rpc.Api.isSimulationSuccess(simResult)) {
      console.log('Simulation was successful');
      
      // Parse the result - token balance should be returned as a ScVal
      const resultScVal = simResult.result?.retval;
      
      if (!resultScVal) {
        console.error('No result returned from token contract call');
        throw new Error('No result returned from contract');
      }
      
      console.log('Result ScVal:', resultScVal);
      
      // Convert ScVal to JavaScript value
      const balanceValue = StellarSdk.scValToNative(resultScVal);
      console.log('Balance value (raw):', balanceValue);
      
      // Convert from contract units (9 decimals) to display units
      const balanceInUnits = parseInt(balanceValue.toString()) / 1000000000; // 9 decimals
      
      console.log(`Direct PDOT token balance for ${address}:`, balanceInUnits.toString());
      return balanceInUnits.toString();
    } else {
      console.error('Simulation failed. Full result:', JSON.stringify(simResult, null, 2));
      
      // Check if it's a specific type of failure
      if ('error' in simResult) {
        console.error('Simulation error details:', simResult.error);
      }
      
      throw new Error('Contract simulation failed');
    }
  } catch (sdkError) {
    console.error('Direct SDK approach failed:', sdkError);
    
    // Log more details about the error
    if (sdkError instanceof Error) {
      console.error('SDK Error name:', sdkError.name);
      console.error('SDK Error message:', sdkError.message);
      console.error('SDK Error stack:', sdkError.stack);
    }
    
    // Fallback: Try to use Horizon API for token balance (if it's a Stellar Classic token)
    console.log('Attempting fallback approach using Horizon API...');
    try {
      const horizon = new Horizon.Server('https://horizon-testnet.stellar.org');
      const account = await horizon.loadAccount(address);
      
      // Look for the token in the account balances
      // This assumes the token contract ID corresponds to a classic token
      const tokenBalance = account.balances.find(
        (balance: any) => balance.asset_code === 'PDOT' || 
                         balance.asset_issuer === TOKEN_CONTRACT_ID ||
                         (balance.asset_type === 'credit_alphanum4' && balance.asset_code === 'PDOT')
      );
      
      if (tokenBalance) {
        console.log('Found token balance via Horizon:', tokenBalance.balance);
        return parseFloat(tokenBalance.balance).toString();
      } else {
        console.log('Token not found in Horizon balances, returning 0');
        return '0';
      }
    } catch (horizonError) {
      console.error('Horizon fallback also failed:', horizonError);
      
      // Final fallback: Use the fallback API route for server-side processing
      console.log('Attempting final fallback using API route...');
      try {
        const timestamp = Date.now();
        const response = await fetch(`/api/token-balance-fallback?address=${address}&_t=${timestamp}`, {
          cache: 'no-store'
        });
        
        if (!response.ok) {
          throw new Error(`API response not ok: ${response.status}`);
        }
        
        const data = await response.json();
        console.log('Fallback API response:', data);
        
        if (data.balance) {
          return data.balance;
        } else {
          console.error('No balance in API response:', data);
          return '0';
        }
      } catch (apiError) {
        console.error('Final API fallback also failed:', apiError);
        return '0';
      }
    }
  }
}

export async function getPTokenBalance(address: string): Promise<string> {
  try {
    // Use Stellar SDK directly instead of API route
    const StellarSdk = await import('@stellar/stellar-sdk');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Build the contract call
    const contract = new StellarSdk.Contract(VAULT_CONTRACT_ID);
    
    // Build the operation to call get_ptoken_balance
    const operation = contract.call(
      'get_ptoken_balance',
      StellarSdk.Address.fromString(address).toScVal() // user parameter
    );

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
      // Parse the result - pToken balance should be returned as a ScVal
      const resultScVal = simResult.result?.retval;
      
      if (!resultScVal) {
        console.error('No result returned from contract call');
        return '0';
      }
      
      // Convert ScVal to JavaScript value
      const balanceValue = StellarSdk.scValToNative(resultScVal);
      
      // Convert from contract units (9 decimals) to display units
      const balanceInUnits = parseInt(balanceValue.toString()) / 1000000000; // 9 decimals
      
      console.log(`Direct pToken balance for ${address}:`, balanceInUnits.toString());
      return balanceInUnits.toString();
    } else {
      console.error('Failed to get pToken balance:', simResult);
      return '0';
    }
  } catch (error) {
    console.error('Error fetching pToken balance directly:', error);
    return '0';
  }
}

export async function getBalances(address: string): Promise<WalletInfo> {
  const xlmBalance = await getXLMBalance(address);
  const testTokenBalance = await getTokenBalance(address);
  const pTokenBalance = await getPTokenBalance(address);

  return {
    isConnected: true,
    address,
    xlmBalance,
    testTokenBalance,
    pTokenBalance
  };
}

// Vault operations with Freighter integration
export async function depositToVault(
  userAddress: string, 
  amount: string, 
  statusCallback?: (status: string) => void
): Promise<{ success: boolean; error?: string; transactionHash?: string }> {
  try {
    statusCallback?.('building');
    
    // Build the transaction using SorobanRpc (not Horizon for contract calls)
    const StellarSdk = await import('@stellar/stellar-sdk');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Convert amount to contract units (9 decimals)
    const amountInUnits = Math.floor(parseFloat(amount) * 1000000000).toString();
    
    // Get user account
    const account = await rpc.getAccount(userAddress);
    
    // Contract addresses
    const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;
    
    // Build the contract invocation operation
    const contract = new StellarSdk.Contract(VAULT_CONTRACT_ID);
    
    const operation = contract.call(
      'deposit',
      StellarSdk.Address.fromString(userAddress).toScVal(), // user parameter
      StellarSdk.nativeToScVal(amountInUnits, { type: 'u128' }) // amount parameter
    );

    // Build the transaction
    const transaction = new StellarSdk.TransactionBuilder(account, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: StellarSdk.Networks.TESTNET,
    })
      .addOperation(operation)
      .setTimeout(30) // Shorter timeout is usually better for contract calls
      .build();

    statusCallback?.('preparing');
    
    // CRUCIAL: Prepare the transaction for Soroban
    const preparedTransaction = await rpc.prepareTransaction(transaction);
    
    const transactionXdr = preparedTransaction.toXDR();
    console.log('Prepared transaction XDR:', transactionXdr);

    statusCallback?.('signing');
    
    // Sign the prepared transaction with Freighter
    const { signTransaction } = await import('@stellar/freighter-api');
    
    const signedResult = await signTransaction(transactionXdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: userAddress,
    });

    if (signedResult.error) {
      return { success: false, error: `Transaction signing failed: ${signedResult.error}` };
    }

    statusCallback?.('submitting');

    // Submit the signed transaction using SorobanRpc
    try {
      console.log('Signed transaction XDR:', signedResult.signedTxXdr);
      
      // Reconstruct the signed transaction from XDR
      const signedTransaction = StellarSdk.TransactionBuilder.fromXDR(
        signedResult.signedTxXdr, 
        StellarSdk.Networks.TESTNET
      );
      
      // Submit via SorobanRpc
      const txResult = await rpc.sendTransaction(signedTransaction);
      console.log('Transaction result:', txResult);
      
      if (txResult.status === 'ERROR') {
        return { 
          success: false, 
          error: `Transaction failed: ${txResult.errorResult || 'Unknown error'}` 
        };
      }
      
      // Wait for the transaction to be confirmed before returning
      // This gives time for the ledger state to update
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      return { 
        success: true, 
        transactionHash: txResult.hash 
      };
    } catch (submitError: any) {
      console.error('Transaction submission error:', submitError);
      return { 
        success: false, 
        error: `Transaction submission failed: ${submitError.message || submitError}` 
      };
    }
  } catch (error) {
    console.error('Deposit error:', error);
    return { success: false, error: `Deposit failed: ${error}` };
  }
}

export async function withdrawFromVault(
  userAddress: string, 
  pTokenAmount: string, 
  statusCallback?: (status: string) => void
): Promise<{ success: boolean; error?: string; transactionHash?: string }> {
  try {
    statusCallback?.('building');
    
    // Build the transaction using SorobanRpc (not Horizon for contract calls)
    const StellarSdk = await import('@stellar/stellar-sdk');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    
    // Convert pToken amount to contract units (9 decimals)
    const pTokenAmountInUnits = Math.floor(parseFloat(pTokenAmount) * 1000000000).toString();
    
    // Get user account
    const account = await rpc.getAccount(userAddress);
    
    // Contract addresses
    const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;
    
    // Build the contract invocation operation
    const contract = new StellarSdk.Contract(VAULT_CONTRACT_ID);
    
    const operation = contract.call(
      'withdraw',
      StellarSdk.Address.fromString(userAddress).toScVal(), // user parameter
      StellarSdk.nativeToScVal(pTokenAmountInUnits, { type: 'u128' }) // ptoken_amount parameter
    );

    // Build the transaction
    const transaction = new StellarSdk.TransactionBuilder(account, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: StellarSdk.Networks.TESTNET,
    })
      .addOperation(operation)
      .setTimeout(30) // Shorter timeout for contract calls
      .build();

    statusCallback?.('preparing');
    
    // CRUCIAL: Prepare the transaction for Soroban
    const preparedTransaction = await rpc.prepareTransaction(transaction);
    
    const transactionXdr = preparedTransaction.toXDR();
    console.log('Prepared withdraw transaction XDR:', transactionXdr);

    statusCallback?.('signing');
    
    // Sign the prepared transaction with Freighter
    const { signTransaction } = await import('@stellar/freighter-api');
    
    const signedResult = await signTransaction(transactionXdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: userAddress,
    });

    if (signedResult.error) {
      return { success: false, error: `Transaction signing failed: ${signedResult.error}` };
    }

    statusCallback?.('submitting');

    // Submit the signed transaction using SorobanRpc
    try {
      console.log('Signed withdraw transaction XDR:', signedResult.signedTxXdr);
      
      // Reconstruct the signed transaction from XDR
      const signedTransaction = StellarSdk.TransactionBuilder.fromXDR(
        signedResult.signedTxXdr, 
        StellarSdk.Networks.TESTNET
      );
      
      // Submit via SorobanRpc
      const txResult = await rpc.sendTransaction(signedTransaction);
      console.log('Withdraw transaction result:', txResult);
      
      if (txResult.status === 'ERROR') {
        return { 
          success: false, 
          error: `Transaction failed: ${txResult.errorResult || 'Unknown error'}` 
        };
      }
      
      // Wait for the transaction to be confirmed before returning
      // This gives time for the ledger state to update
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      return { 
        success: true, 
        transactionHash: txResult.hash 
      };
    } catch (submitError: any) {
      console.error('Withdraw transaction submission error:', submitError);
      return { 
        success: false, 
        error: `Transaction submission failed: ${submitError.message || submitError}` 
      };
    }
  } catch (error) {
    console.error('Withdraw error:', error);
    return { success: false, error: `Withdraw failed: ${error}` };
  }
}

export async function getVaultStats(): Promise<VaultStats> {
  try {
    const response = await fetch('/api/vault-stats');
    const data = await response.json();
    return data;
  } catch (error) {
    console.error('Error fetching vault stats:', error);
    return {
      totalDeposited: '0',
      totalPTokens: '0',
      exchangeRate: '1',
      userShare: '0'
    };
  }
}

// Helper function to format numbers
export function formatNumber(value: string, decimals: number = 2): string {
  const num = parseFloat(value);
  if (isNaN(num)) return '0.00';
  return num.toFixed(decimals);
}

// Mint test tokens function
export async function mintTestTokens(userAddress: string): Promise<{ success: boolean; error?: string }> {
  try {
    const response = await fetch('/api/mint-tokens', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ userAddress }),
    });

    const data = await response.json();
    return data;
  } catch (error) {
    console.error('Mint tokens error:', error);
    return { success: false, error: `Failed to mint tokens: ${error}` };
  }
} 