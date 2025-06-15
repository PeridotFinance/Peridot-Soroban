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
  try {
    // Add cache-busting parameter to ensure fresh data
    const timestamp = Date.now();
    const response = await fetch(`/api/token-balance?address=${address}&_t=${timestamp}`, {
      cache: 'no-store'
    });
    const data = await response.json();
    return data.balance || '0';
  } catch (error) {
    console.error('Error fetching token balance:', error);
    return '0';
  }
}

export async function getPTokenBalance(address: string): Promise<string> {
  try {
    // Add cache-busting parameter to ensure fresh data
    const timestamp = Date.now();
    const response = await fetch(`/api/ptoken-balance?address=${address}&_t=${timestamp}`, {
      cache: 'no-store'
    });
    const data = await response.json();
    console.log(`Fresh pToken balance for ${address}:`, data.balance);
    return data.balance || '0';
  } catch (error) {
    console.error('Error fetching pToken balance:', error);
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

export async function withdrawFromVault(userAddress: string, pTokenAmount: string): Promise<{ success: boolean; error?: string }> {
  try {
    const response = await fetch('/api/withdraw', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ userAddress, pTokenAmount }),
    });

    const data = await response.json();
    return data;
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