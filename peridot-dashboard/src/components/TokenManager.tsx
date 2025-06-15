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
    return null;
  }

  const hasTestTokens = parseFloat(walletInfo.testTokenBalance) > 0;

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center space-x-3">
          <div className="w-10 h-10 bg-green-100 rounded-full flex items-center justify-center">
            <Coins className="w-5 h-5 text-green-600" />
          </div>
          <div>
            <h3 className="font-semibold text-gray-900">PDOT Token Manager</h3>
            <p className="text-sm text-gray-600">
              Get PDOT tokens to start using the vault
            </p>
          </div>
        </div>
      </div>

      {/* Current Balance Display */}
      <div className="mb-4 p-4 bg-gray-50 rounded-lg">
        <div className="flex items-center justify-between">
          <span className="text-sm text-gray-600">Your PDOT Token Balance:</span>
          <span className="text-lg font-semibold text-gray-900">
            {formatNumber(walletInfo.testTokenBalance)} PDOT
          </span>
        </div>
      </div>

      {/* Status Messages */}
      {mintStatus === 'success' && (
        <div className="mb-4 p-3 bg-green-50 border border-green-200 rounded-md">
          <div className="flex items-center">
            <CheckCircle className="w-4 h-4 text-green-500 mr-2" />
            <p className="text-sm text-green-700">
              Successfully minted 1,000 PDOT tokens!
            </p>
          </div>
        </div>
      )}

      {mintStatus === 'error' && error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md">
          <div className="flex items-center">
            <AlertCircle className="w-4 h-4 text-red-500 mr-2" />
            <p className="text-sm text-red-700">{error}</p>
          </div>
        </div>
      )}

      {/* Mint Button */}
      <div className="space-y-3">
        <button
          onClick={handleMintTokens}
          disabled={isMinting}
          className="w-full inline-flex items-center justify-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-green-600 hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {isMinting ? (
            <>
              <Loader className="w-4 h-4 mr-2 animate-spin" />
              Minting Tokens...
            </>
          ) : (
            <>
              <Coins className="w-4 h-4 mr-2" />
              Get 1,000 PDOT Tokens
            </>
          )}
        </button>

        {hasTestTokens && (
          <p className="text-xs text-center text-gray-500">
            You already have PDOT tokens! You can mint more if needed.
          </p>
        )}

        {!hasTestTokens && (
          <p className="text-xs text-center text-gray-500">
            Free PDOT tokens for Stellar Testnet. No real value.
          </p>
        )}
      </div>

      {/* Information Box */}
      <div className="mt-4 p-3 bg-blue-50 border border-blue-200 rounded-md">
        <h4 className="text-sm font-medium text-blue-900 mb-1">
          About PDOT Tokens
        </h4>
        <p className="text-xs text-blue-700">
          PDOT tokens are used for demonstration purposes on the Stellar Testnet. 
          They have no real value and are used to interact with the vault contract.
        </p>
      </div>
    </div>
  );
} 