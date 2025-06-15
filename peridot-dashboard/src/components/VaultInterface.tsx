'use client';

import { useState } from 'react';
import { ArrowDown, ArrowUp, Loader, CheckCircle, AlertCircle } from 'lucide-react';
import { depositToVault, withdrawFromVault, formatNumber } from '@/utils/stellar';

interface VaultInterfaceProps {
  walletInfo: any;
  onTransactionComplete: () => void;
}

export default function VaultInterface({ walletInfo, onTransactionComplete }: VaultInterfaceProps) {
  const [activeTab, setActiveTab] = useState<'deposit' | 'withdraw'>('deposit');
  const [depositAmount, setDepositAmount] = useState('');
  const [withdrawAmount, setWithdrawAmount] = useState('');
  const [isProcessing, setIsProcessing] = useState(false);
  const [transactionStatus, setTransactionStatus] = useState<'idle' | 'building' | 'signing' | 'submitting' | 'success' | 'error'>('idle');
  const [error, setError] = useState<string | null>(null);

  const handleDeposit = async () => {
    if (!walletInfo?.address || !depositAmount) return;

    const amount = parseFloat(depositAmount);
    const available = parseFloat(walletInfo.testTokenBalance);

    if (amount <= 0) {
      setError('Please enter a valid amount');
      return;
    }

    if (amount > available) {
      setError('Insufficient PDOT token balance');
      return;
    }

    setIsProcessing(true);
    setError(null);
    setTransactionStatus('building');

    try {
      // This will trigger the Freighter popup for signing
      const result = await depositToVault(
        walletInfo.address, 
        depositAmount,
        (status) => setTransactionStatus(status as any)
      );
      
      if (result.success) {
        setTransactionStatus('success');
        setDepositAmount('');
        if (result.transactionHash) {
          console.log('Transaction successful! Hash:', result.transactionHash);
        }
        onTransactionComplete();
        setTimeout(() => setTransactionStatus('idle'), 5000);
      } else {
        setTransactionStatus('error');
        setError(result.error || 'Deposit failed');
      }
    } catch (err) {
      setTransactionStatus('error');
      setError(`Deposit failed: ${err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleWithdraw = async () => {
    if (!walletInfo?.address || !withdrawAmount) return;

    const amount = parseFloat(withdrawAmount);
    const available = parseFloat(walletInfo.pTokenBalance);

    if (amount <= 0) {
      setError('Please enter a valid amount');
      return;
    }

    if (amount > available) {
      setError('Insufficient pToken balance');
      return;
    }

    setIsProcessing(true);
    setError(null);
    setTransactionStatus('idle');

    try {
      const result = await withdrawFromVault(walletInfo.address, withdrawAmount);
      
      if (result.success) {
        setTransactionStatus('success');
        setWithdrawAmount('');
        onTransactionComplete();
        setTimeout(() => setTransactionStatus('idle'), 3000);
      } else {
        setTransactionStatus('error');
        setError(result.error || 'Withdrawal failed');
      }
    } catch (err) {
      setTransactionStatus('error');
      setError(`Withdrawal failed: ${err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const setMaxAmount = (type: 'deposit' | 'withdraw') => {
    if (type === 'deposit') {
      setDepositAmount(walletInfo.testTokenBalance);
    } else {
      setWithdrawAmount(walletInfo.pTokenBalance);
    }
  };

  if (!walletInfo?.isConnected) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="text-center py-8">
          <div className="w-16 h-16 bg-gray-100 rounded-full flex items-center justify-center mx-auto mb-4">
            <ArrowDown className="w-8 h-8 text-gray-400" />
          </div>
          <h3 className="text-lg font-semibold text-gray-900 mb-2">
            Vault Operations
          </h3>
          <p className="text-gray-600">
            Connect your wallet to deposit and withdraw from the vault
          </p>
        </div>
      </div>
    );
  }

  const hasTestTokens = parseFloat(walletInfo.testTokenBalance) > 0;
  const hasPTokens = parseFloat(walletInfo.pTokenBalance) > 0;

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="mb-6">
        <h3 className="text-lg font-semibold text-gray-900 mb-2">
          Vault Operations
        </h3>
        <p className="text-sm text-gray-600">
          Deposit PDOT tokens to receive pTokens, or withdraw pTokens to get PDOT tokens back
        </p>
      </div>

      {/* Tab Navigation */}
      <div className="flex space-x-1 mb-6 bg-gray-100 p-1 rounded-lg">
        <button
          onClick={() => setActiveTab('deposit')}
          className={`flex-1 flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md transition-colors ${
            activeTab === 'deposit'
              ? 'bg-white text-green-700 shadow-sm'
              : 'text-gray-500 hover:text-gray-700'
          }`}
        >
          <ArrowDown className="w-4 h-4 mr-2" />
          Deposit
        </button>
        <button
          onClick={() => setActiveTab('withdraw')}
          className={`flex-1 flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md transition-colors ${
            activeTab === 'withdraw'
              ? 'bg-white text-green-700 shadow-sm'
              : 'text-gray-500 hover:text-gray-700'
          }`}
        >
          <ArrowUp className="w-4 h-4 mr-2" />
          Withdraw
        </button>
      </div>

      {/* Status Messages */}
      {transactionStatus === 'building' && (
        <div className="mb-4 p-3 bg-blue-50 border border-blue-200 rounded-md">
          <div className="flex items-center">
            <Loader className="w-4 h-4 text-blue-500 mr-2 animate-spin" />
            <p className="text-sm text-blue-700">
              Building transaction...
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'signing' && (
        <div className="mb-4 p-3 bg-yellow-50 border border-yellow-200 rounded-md">
          <div className="flex items-center">
            <AlertCircle className="w-4 h-4 text-yellow-500 mr-2" />
            <p className="text-sm text-yellow-700">
              Please sign the transaction in your Freighter wallet
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'submitting' && (
        <div className="mb-4 p-3 bg-blue-50 border border-blue-200 rounded-md">
          <div className="flex items-center">
            <Loader className="w-4 h-4 text-blue-500 mr-2 animate-spin" />
            <p className="text-sm text-blue-700">
              Submitting transaction to Stellar network...
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'success' && (
        <div className="mb-4 p-3 bg-green-50 border border-green-200 rounded-md">
          <div className="flex items-center">
            <CheckCircle className="w-4 h-4 text-green-500 mr-2" />
            <p className="text-sm text-green-700">
              Transaction completed successfully! You received pTokens.
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'error' && error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md">
          <div className="flex items-center">
            <AlertCircle className="w-4 h-4 text-red-500 mr-2" />
            <p className="text-sm text-red-700">{error}</p>
          </div>
        </div>
      )}

      {/* Deposit Tab */}
      {activeTab === 'deposit' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-sm text-gray-600">
            <span>Available PDOT tokens:</span>
            <span className="font-medium">{formatNumber(walletInfo.testTokenBalance)}</span>
          </div>

          <div>
            <label htmlFor="deposit-amount" className="block text-sm font-medium text-gray-700 mb-1">
              Deposit Amount
            </label>
            <div className="flex space-x-2">
              <input
                id="deposit-amount"
                type="number"
                value={depositAmount}
                onChange={(e) => setDepositAmount(e.target.value)}
                placeholder="0.00"
                min="0"
                step="0.01"
                className="flex-1 px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-green-500 focus:border-green-500"
                disabled={isProcessing}
              />
              <button
                onClick={() => setMaxAmount('deposit')}
                className="px-3 py-2 text-sm font-medium text-green-700 bg-green-50 border border-green-200 rounded-md hover:bg-green-100 focus:outline-none focus:ring-2 focus:ring-green-500"
                disabled={isProcessing || !hasTestTokens}
              >
                Max
              </button>
            </div>
            <p className="mt-1 text-xs text-gray-500">
              You will receive {depositAmount || '0'} pTokens (1:1 ratio)
            </p>
          </div>

          <button
            onClick={handleDeposit}
            disabled={isProcessing || !hasTestTokens || !depositAmount}
            className="w-full flex items-center justify-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-green-600 hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isProcessing ? (
              <>
                <Loader className="w-4 h-4 mr-2 animate-spin" />
                {transactionStatus === 'building' && 'Building Transaction...'}
                {transactionStatus === 'signing' && 'Sign in Freighter...'}
                {transactionStatus === 'submitting' && 'Submitting...'}
                {(transactionStatus === 'idle' || !transactionStatus) && 'Processing...'}
              </>
            ) : (
              <>
                <ArrowDown className="w-4 h-4 mr-2" />
                Deposit PDOT Tokens
              </>
            )}
          </button>

          {!hasTestTokens && (
            <p className="text-xs text-center text-red-600">
              You need PDOT tokens to make a deposit. Use the Token Manager above to get some.
            </p>
          )}
        </div>
      )}

      {/* Withdraw Tab */}
      {activeTab === 'withdraw' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-sm text-gray-600">
            <span>Available pTokens:</span>
            <span className="font-medium">{formatNumber(walletInfo.pTokenBalance)}</span>
          </div>

          <div>
            <label htmlFor="withdraw-amount" className="block text-sm font-medium text-gray-700 mb-1">
              Withdraw Amount (pTokens)
            </label>
            <div className="flex space-x-2">
              <input
                id="withdraw-amount"
                type="number"
                value={withdrawAmount}
                onChange={(e) => setWithdrawAmount(e.target.value)}
                placeholder="0.00"
                min="0"
                step="0.01"
                className="flex-1 px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-green-500 focus:border-green-500"
                disabled={isProcessing}
              />
              <button
                onClick={() => setMaxAmount('withdraw')}
                className="px-3 py-2 text-sm font-medium text-green-700 bg-green-50 border border-green-200 rounded-md hover:bg-green-100 focus:outline-none focus:ring-2 focus:ring-green-500"
                disabled={isProcessing || !hasPTokens}
              >
                Max
              </button>
            </div>
            <p className="mt-1 text-xs text-gray-500">
              You will receive {withdrawAmount || '0'} PDOT tokens (1:1 ratio)
            </p>
          </div>

          <button
            onClick={handleWithdraw}
            disabled={isProcessing || !hasPTokens || !withdrawAmount}
            className="w-full flex items-center justify-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-green-600 hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isProcessing ? (
              <>
                <Loader className="w-4 h-4 mr-2 animate-spin" />
                Processing Withdrawal...
              </>
            ) : (
              <>
                <ArrowUp className="w-4 h-4 mr-2" />
                Withdraw pTokens
              </>
            )}
          </button>

          {!hasPTokens && (
            <p className="text-xs text-center text-orange-600">
              You need pTokens to make a withdrawal. Deposit PDOT tokens first to receive pTokens.
            </p>
          )}
        </div>
      )}
    </div>
  );
} 