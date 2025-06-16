'use client';

import { useState } from 'react';
import { Coins, Loader, CheckCircle, AlertCircle } from 'lucide-react';
import { mintTestTokens, formatNumber } from '@/utils/stellar';

interface TokenManagerProps {
  walletInfo: any;
  onTokensMinted: () => void;
}

export default function TokenManager({ walletInfo, onTokensMinted }: TokenManagerProps) {
  const [isMinting, setIsMinting] = useState(false);
  const [mintStatus, setMintStatus] = useState<'idle' | 'success' | 'error'>('idle');
  const [error, setError] = useState<string | null>(null);

  const handleMintTokens = async () => {
    if (!walletInfo?.address) return;

    setIsMinting(true);
    setError(null);
    setMintStatus('idle');

    try {
      const result = await mintTestTokens(walletInfo.address);
      
      if (result.success) {
        setMintStatus('success');
        onTokensMinted(); // Refresh balances
        setTimeout(() => setMintStatus('idle'), 3000); // Reset status after 3 seconds
      } else {
        setMintStatus('error');
        setError(result.error || 'Failed to mint tokens');
      }
    } catch (err) {
      setMintStatus('error');
      setError(`Minting failed: ${err}`);
    } finally {
      setIsMinting(false);
    }
  };

  if (!walletInfo?.isConnected) {
    return (
      <div className="glass-card">
        <div className="text-center py-8">
          <div className="w-16 h-16 bg-slate-100 dark:bg-slate-800 rounded-full flex items-center justify-center mx-auto mb-4">
            <Coins className="w-8 h-8 text-slate-400" />
          </div>
          <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-2">
            Token Manager
          </h3>
          <p className="text-slate-600 dark:text-slate-400">
            Connect your wallet to mint test PDOT tokens
          </p>
        </div>
      </div>
    );
  }

  const hasTestTokens = parseFloat(walletInfo.testTokenBalance) > 0;

  return (
    <div className="glass-card">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center space-x-3">
          <div className="w-10 h-10 bg-gradient-to-br from-blue-500 to-purple-600 rounded-xl flex items-center justify-center shadow-lg">
            <Coins className="w-5 h-5 text-white" />
          </div>
          <div>
            <h3 className="font-semibold text-slate-900 dark:text-slate-100">PDOT Token Manager</h3>
            <p className="text-sm text-slate-600 dark:text-slate-400">
              Get PDOT tokens to start using the vault
            </p>
          </div>
        </div>
      </div>

      {/* Current Balance Display */}
      <div className="mb-4 p-4 bg-slate-50 dark:bg-slate-800 rounded-lg">
        <div className="flex items-center justify-between">
          <span className="text-sm text-slate-600 dark:text-slate-400">Your PDOT Token Balance:</span>
          <span className="text-lg font-semibold text-slate-900 dark:text-slate-100">
            {formatNumber(walletInfo.testTokenBalance)} PDOT
          </span>
        </div>
      </div>

      {/* Status Messages */}
      {mintStatus === 'success' && (
        <div className="mb-4 p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-md">
          <div className="flex items-center">
            <CheckCircle className="w-4 h-4 text-green-500 mr-2" />
            <p className="text-sm text-green-700 dark:text-green-300">
              Successfully minted 1,000 PDOT tokens!
            </p>
          </div>
        </div>
      )}

      {mintStatus === 'error' && error && (
        <div className="mb-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <div className="flex items-center">
            <AlertCircle className="w-4 h-4 text-red-500 mr-2" />
            <p className="text-sm text-red-700 dark:text-red-400">{error}</p>
          </div>
        </div>
      )}

      {/* Mint Button */}
      <button
        onClick={handleMintTokens}
        disabled={isMinting}
        className="w-full group relative overflow-hidden px-6 py-4 bg-gradient-to-r from-blue-600/70 via-indigo-600/70 to-purple-600/70 hover:from-blue-500/80 hover:via-indigo-500/80 hover:to-purple-500/80 rounded-xl border-2 border-blue-400/40 hover:border-blue-300/60 focus:outline-none focus:ring-2 focus:ring-blue-400/50 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-xl hover:shadow-blue-500/20 backdrop-blur-lg"
      >
        <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
        <div className="relative flex items-center justify-center space-x-3">
          {isMinting ? (
            <>
              <Loader className="w-5 h-5 text-white animate-spin" />
              <span className="text-lg font-bold text-white font-mono uppercase tracking-wide">
                MINTING...
              </span>
            </>
          ) : (
            <>
              <Coins className="w-5 h-5 text-white group-hover:text-blue-100 transition-colors duration-300" />
              <span className="text-lg font-bold text-white group-hover:text-blue-100 font-mono uppercase tracking-wide transition-colors duration-300">
                MINT_1000_PDOT
              </span>
            </>
          )}
        </div>
      </button>

      {hasTestTokens && (
        <p className="text-xs text-center text-slate-600 dark:text-slate-400 mt-3">
          You already have PDOT tokens. You can mint more if needed for testing.
        </p>
      )}

      {!hasTestTokens && (
        <p className="text-xs text-center text-blue-700 dark:text-blue-400 mt-3">
          Click above to mint 1,000 free PDOT tokens for testing the vault.
        </p>
      )}
    </div>
  );
} 