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
  const [transactionStatus, setTransactionStatus] = useState<'idle' | 'building' | 'preparing' | 'signing' | 'submitting' | 'success' | 'error'>('idle');
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
    setTransactionStatus('building');

    try {
      // This will trigger the Freighter popup for signing
      const result = await withdrawFromVault(
        walletInfo.address, 
        withdrawAmount,
        (status) => setTransactionStatus(status as any)
      );
      
      if (result.success) {
        setTransactionStatus('success');
        setWithdrawAmount('');
        if (result.transactionHash) {
          console.log('Withdraw transaction successful! Hash:', result.transactionHash);
        }
        onTransactionComplete();
        setTimeout(() => setTransactionStatus('idle'), 5000);
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
      <div className="text-center py-8">
        <div className="w-16 h-16 bg-slate-100 dark:bg-slate-800 rounded-full flex items-center justify-center mx-auto mb-4">
          <ArrowDown className="w-8 h-8 text-slate-500 dark:text-slate-400" />
        </div>
        <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-2">
          Vault Operations
        </h3>
        <p className="text-slate-700 dark:text-slate-300">
          Connect your wallet to deposit and withdraw from the vault
        </p>
      </div>
    );
  }

  const hasTestTokens = parseFloat(walletInfo.testTokenBalance) > 0;
  const hasPTokens = parseFloat(walletInfo.pTokenBalance) > 0;

  return (
    <>
      <div className="mb-6">
        <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-2">
          Vault Operations
        </h3>
        <p className="text-sm text-slate-700 dark:text-slate-300">
          Deposit PDOT tokens to receive pTokens, or withdraw pTokens to get PDOT tokens back
        </p>
      </div>

      {/* Tab Navigation */}
      <div className="flex space-x-1 mb-6 bg-slate-100 dark:bg-slate-800 p-1 rounded-lg">
        <button
          onClick={() => setActiveTab('deposit')}
          className={`flex-1 flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md transition-colors ${
            activeTab === 'deposit'
              ? 'bg-white dark:bg-slate-700 text-emerald-700 dark:text-emerald-300 shadow-sm'
              : 'text-slate-600 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'
          }`}
        >
          <ArrowDown className="w-4 h-4 mr-2" />
          Deposit
        </button>
        <button
          onClick={() => setActiveTab('withdraw')}
          className={`flex-1 flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md transition-colors ${
            activeTab === 'withdraw'
              ? 'bg-white dark:bg-slate-700 text-emerald-700 dark:text-emerald-300 shadow-sm'
              : 'text-slate-600 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'
          }`}
        >
          <ArrowUp className="w-4 h-4 mr-2" />
          Withdraw
        </button>
      </div>

      {/* Cyber Status Messages */}
      {transactionStatus === 'building' && (
        <div className="mb-6 relative overflow-hidden rounded-xl bg-gradient-to-r from-blue-500/2 via-cyan-500/1 to-purple-500/2 dark:from-blue-400/3 dark:via-cyan-400/2 dark:to-purple-400/3 border border-blue-400/15 dark:border-blue-400/25 shadow-lg shadow-blue-500/5 backdrop-blur-2xl">
          <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-blue-400/40 via-cyan-400/40 to-purple-400/40"></div>
          <div className="relative p-6 flex flex-col items-center">
            <svg className="pl mb-4" width="240" height="240" viewBox="0 0 240 240">
              <circle className="pl__ring pl__ring--a" cx="120" cy="120" r="105" fill="none" strokeWidth="20" strokeDasharray="0 660" strokeDashoffset="-330" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--b" cx="120" cy="120" r="35" fill="none" strokeWidth="20" strokeDasharray="0 220" strokeDashoffset="-110" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--c" cx="85" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--d" cx="155" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
            </svg>
            <p className="text-lg font-bold text-blue-700 dark:text-blue-300 font-mono uppercase tracking-wide">
              BUILDING_TRANSACTION...
            </p>
            <p className="text-sm text-blue-600 dark:text-blue-400 font-mono opacity-80 mt-1">
              Preparing smart contract call
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'preparing' && (
        <div className="mb-6 relative overflow-hidden rounded-xl bg-gradient-to-r from-indigo-500/2 via-purple-500/1 to-pink-500/2 dark:from-indigo-400/3 dark:via-purple-400/2 dark:to-pink-400/3 border border-indigo-400/15 dark:border-indigo-400/25 shadow-lg shadow-indigo-500/5 backdrop-blur-2xl">
          <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-indigo-400/40 via-purple-400/40 to-pink-400/40"></div>
          <div className="relative p-6 flex flex-col items-center">
            <svg className="pl mb-4" width="240" height="240" viewBox="0 0 240 240">
              <circle className="pl__ring pl__ring--a" cx="120" cy="120" r="105" fill="none" strokeWidth="20" strokeDasharray="0 660" strokeDashoffset="-330" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--b" cx="120" cy="120" r="35" fill="none" strokeWidth="20" strokeDasharray="0 220" strokeDashoffset="-110" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--c" cx="85" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--d" cx="155" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
            </svg>
            <p className="text-lg font-bold text-indigo-700 dark:text-indigo-300 font-mono uppercase tracking-wide">
              PREPARING_TRANSACTION...
            </p>
            <p className="text-sm text-indigo-600 dark:text-indigo-400 font-mono opacity-80 mt-1">
              Simulating and optimizing contract call
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'signing' && (
        <div className="mb-6 relative overflow-hidden rounded-xl bg-gradient-to-r from-yellow-500/2 via-orange-500/1 to-red-500/2 dark:from-yellow-400/3 dark:via-orange-400/2 dark:to-red-400/3 border border-yellow-400/15 dark:border-yellow-400/25 shadow-lg shadow-yellow-500/5 backdrop-blur-2xl">
          <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-yellow-400/40 via-orange-400/40 to-red-400/40"></div>
          <div className="relative p-4 flex items-center space-x-4">
            <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-yellow-500/40 to-orange-500/40 backdrop-blur-md flex items-center justify-center shadow-lg">
              <AlertCircle className="w-5 h-5 text-white" />
            </div>
            <div className="flex-1">
              <p className="text-sm font-bold text-yellow-700 dark:text-yellow-300 font-mono uppercase tracking-wide">
                SIGNATURE_REQUIRED
              </p>
              <p className="text-xs text-yellow-600 dark:text-yellow-400 font-mono opacity-80">
                Please sign the transaction in your Freighter wallet
              </p>
            </div>
          </div>
        </div>
      )}

      {transactionStatus === 'submitting' && (
        <div className="mb-6 relative overflow-hidden rounded-xl bg-gradient-to-r from-emerald-500/2 via-teal-500/1 to-cyan-500/2 dark:from-emerald-400/3 dark:via-teal-400/2 dark:to-cyan-400/3 border border-emerald-400/15 dark:border-emerald-400/25 shadow-lg shadow-emerald-500/5 backdrop-blur-2xl">
          <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-emerald-400/40 via-teal-400/40 to-cyan-400/40"></div>
          <div className="relative p-6 flex flex-col items-center">
            <svg className="pl mb-4" width="240" height="240" viewBox="0 0 240 240">
              <circle className="pl__ring pl__ring--a" cx="120" cy="120" r="105" fill="none" strokeWidth="20" strokeDasharray="0 660" strokeDashoffset="-330" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--b" cx="120" cy="120" r="35" fill="none" strokeWidth="20" strokeDasharray="0 220" strokeDashoffset="-110" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--c" cx="85" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
              <circle className="pl__ring pl__ring--d" cx="155" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
            </svg>
            <p className="text-lg font-bold text-emerald-700 dark:text-emerald-300 font-mono uppercase tracking-wide">
              SUBMITTING_TO_NETWORK...
            </p>
            <p className="text-sm text-emerald-600 dark:text-emerald-400 font-mono opacity-80 mt-1">
              Broadcasting to Stellar Testnet
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'success' && (
        <div className="mb-4 p-3 bg-emerald-50 dark:bg-emerald-900/20 border border-emerald-200 dark:border-emerald-800 rounded-md">
          <div className="flex items-center">
            <CheckCircle className="w-4 h-4 text-emerald-600 dark:text-emerald-400 mr-2" />
            <p className="text-sm text-emerald-800 dark:text-emerald-300">
              {activeTab === 'deposit' 
                ? 'Deposit completed successfully! You received pTokens.'
                : 'Withdrawal completed successfully! You received PDOT tokens.'
              }
            </p>
          </div>
        </div>
      )}

      {transactionStatus === 'error' && error && (
        <div className="mb-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <div className="flex items-center">
            <AlertCircle className="w-4 h-4 text-red-600 dark:text-red-400 mr-2" />
            <p className="text-sm text-red-800 dark:text-red-400">{error}</p>
          </div>
        </div>
      )}

      {/* Deposit Tab */}
      {activeTab === 'deposit' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-sm text-slate-700 dark:text-slate-300">
            <span>Available PDOT tokens:</span>
            <span className="font-medium">{formatNumber(walletInfo.testTokenBalance)}</span>
          </div>

          <div className="mb-4 sm:mb-6">
            <div className="inputbox">
              <input
                id="deposit-amount"
                type="number"
                value={depositAmount}
                onChange={(e) => setDepositAmount(e.target.value)}
                disabled={isProcessing}
                required
              />
              <span>Enter PDOT amount</span>
              <i></i>
              <div className="max-button">
                <button
                  onClick={() => setMaxAmount('deposit')}
                  className="px-2 py-1 bg-gradient-to-r from-emerald-500/20 to-teal-500/20 text-emerald-600 dark:text-emerald-400 text-xs font-bold rounded border border-emerald-400/30 hover:from-emerald-500/30 hover:to-teal-500/30 hover:border-emerald-400/50 transition-all duration-300"
                  disabled={isProcessing || !hasTestTokens}
                >
                  MAX
                </button>
              </div>
            </div>
            <p className="mt-1 text-xs text-slate-600 dark:text-slate-400">
              You will receive {depositAmount || '0'} pTokens (1:1 ratio)
            </p>
          </div>

          <button
            onClick={handleDeposit}
            disabled={isProcessing || !hasTestTokens || !depositAmount}
            className="w-full group relative overflow-hidden px-6 py-4 bg-gradient-to-r from-emerald-600/70 via-teal-600/70 to-cyan-600/70 hover:from-emerald-500/80 hover:via-teal-500/80 hover:to-cyan-500/80 rounded-xl border-2 border-emerald-400/40 hover:border-emerald-300/60 focus:outline-none focus:ring-2 focus:ring-emerald-400/50 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-xl hover:shadow-emerald-500/20 backdrop-blur-lg"
          >
            <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
            <div className="relative flex items-center justify-center space-x-3">
              {isProcessing ? (
                <>
                  <svg className="w-6 h-6" viewBox="0 0 60 60">
                    <circle className="pl__ring pl__ring--a" cx="30" cy="30" r="26.25" fill="none" strokeWidth="5" strokeDasharray="0 165" strokeDashoffset="-82.5" strokeLinecap="round"></circle>
                    <circle className="pl__ring pl__ring--b" cx="30" cy="30" r="8.75" fill="none" strokeWidth="5" strokeDasharray="0 55" strokeDashoffset="-27.5" strokeLinecap="round"></circle>
                    <circle className="pl__ring pl__ring--c" cx="21.25" cy="30" r="17.5" fill="none" strokeWidth="5" strokeDasharray="0 110" strokeLinecap="round"></circle>
                    <circle className="pl__ring pl__ring--d" cx="38.75" cy="30" r="17.5" fill="none" strokeWidth="5" strokeDasharray="0 110" strokeLinecap="round"></circle>
                  </svg>
                  <span className="text-lg font-bold text-white font-mono uppercase tracking-wide">
                    {transactionStatus === 'building' && 'BUILDING...'}
                    {transactionStatus === 'preparing' && 'PREPARING...'}
                    {transactionStatus === 'signing' && 'SIGNING...'}
                    {transactionStatus === 'submitting' && 'SUBMITTING...'}
                    {(transactionStatus === 'idle' || !transactionStatus) && 'PROCESSING...'}
                  </span>
                </>
              ) : (
                <>
                  <ArrowDown className="w-5 h-5 text-white group-hover:text-emerald-100 transition-colors duration-300" />
                  <span className="text-lg font-bold text-white group-hover:text-emerald-100 font-mono uppercase tracking-wide transition-colors duration-300">
                    DEPOSIT_PDOT
                  </span>
                </>
              )}
            </div>
          </button>

          {!hasTestTokens && (
            <p className="text-xs text-center text-red-700 dark:text-red-400">
              You need PDOT tokens to make a deposit. Use the Token Manager above to get some.
            </p>
          )}
        </div>
      )}

      {/* Withdraw Tab */}
      {activeTab === 'withdraw' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-sm text-slate-700 dark:text-slate-300">
            <span>Available pTokens:</span>
            <span className="font-medium">{formatNumber(walletInfo.pTokenBalance)}</span>
          </div>

          <div className="mb-4 sm:mb-6">
            <div className="inputbox">
              <input
                id="withdraw-amount"
                type="number"
                value={withdrawAmount}
                onChange={(e) => setWithdrawAmount(e.target.value)}
                disabled={isProcessing}
                required
              />
              <span>Enter pToken amount</span>
              <i></i>
              <div className="max-button">
                <button
                  onClick={() => setMaxAmount('withdraw')}
                  className="px-2 py-1 bg-gradient-to-r from-emerald-500/20 to-teal-500/20 text-emerald-600 dark:text-emerald-400 text-xs font-bold rounded border border-emerald-400/30 hover:from-emerald-500/30 hover:to-teal-500/30 hover:border-emerald-400/50 transition-all duration-300"
                  disabled={isProcessing || !hasPTokens}
                >
                  MAX
                </button>
              </div>
            </div>
            <p className="mt-1 text-xs text-slate-600 dark:text-slate-400">
              You will receive {withdrawAmount || '0'} PDOT tokens (1:1 ratio)
            </p>
          </div>

          <button
            onClick={handleWithdraw}
            disabled={isProcessing || !hasPTokens || !withdrawAmount}
            className="w-full group relative overflow-hidden px-6 py-4 bg-gradient-to-r from-purple-600/70 via-violet-600/70 to-pink-600/70 hover:from-purple-500/80 hover:via-violet-500/80 hover:to-pink-500/80 rounded-xl border-2 border-purple-400/40 hover:border-purple-300/60 focus:outline-none focus:ring-2 focus:ring-purple-400/50 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-xl hover:shadow-purple-500/20 backdrop-blur-lg"
          >
            <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
            <div className="relative flex items-center justify-center space-x-3">
              {isProcessing ? (
                <>
                  <svg className="w-6 h-6" viewBox="0 0 60 60">
                    <circle className="pl__ring pl__ring--a" cx="30" cy="30" r="26.25" fill="none" strokeWidth="5" strokeDasharray="0 165" strokeDashoffset="-82.5" strokeLinecap="round"></circle>
                    <circle className="pl__ring pl__ring--b" cx="30" cy="30" r="8.75" fill="none" strokeWidth="5" strokeDasharray="0 55" strokeDashoffset="-27.5" strokeLinecap="round"></circle>
                    <circle className="pl__ring pl__ring--c" cx="21.25" cy="30" r="17.5" fill="none" strokeWidth="5" strokeDasharray="0 110" strokeLinecap="round"></circle>
                    <circle className="pl__ring pl__ring--d" cx="38.75" cy="30" r="17.5" fill="none" strokeWidth="5" strokeDasharray="0 110" strokeLinecap="round"></circle>
                  </svg>
                  <span className="text-lg font-bold text-white font-mono uppercase tracking-wide">
                    {transactionStatus === 'building' && 'BUILDING...'}
                    {transactionStatus === 'preparing' && 'PREPARING...'}
                    {transactionStatus === 'signing' && 'SIGNING...'}
                    {transactionStatus === 'submitting' && 'SUBMITTING...'}
                    {(transactionStatus === 'idle' || !transactionStatus) && 'PROCESSING...'}
                  </span>
                </>
              ) : (
                <>
                  <ArrowUp className="w-5 h-5 text-white group-hover:text-purple-100 transition-colors duration-300" />
                  <span className="text-lg font-bold text-white group-hover:text-purple-100 font-mono uppercase tracking-wide transition-colors duration-300">
                    WITHDRAW_PTOKENS
                  </span>
                </>
              )}
            </div>
          </button>

          {!hasPTokens && (
            <p className="text-xs text-center text-orange-700 dark:text-orange-400">
              You need pTokens to make a withdrawal. Deposit PDOT tokens first to receive pTokens.
            </p>
          )}
        </div>
      )}
    </>
  );
} 