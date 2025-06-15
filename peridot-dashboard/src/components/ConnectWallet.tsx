'use client';

import { useState, useEffect } from 'react';
import { Wallet, X, AlertCircle } from 'lucide-react';
import { connectFreighter, getWalletAddress, WalletInfo } from '@/utils/stellar';

interface ConnectWalletProps {
  onWalletChange: (walletInfo: WalletInfo | null) => void;
  walletInfo: WalletInfo | null;
}

export default function ConnectWallet({ onWalletChange, walletInfo }: ConnectWalletProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Check if wallet is already connected on component mount
  useEffect(() => {
    checkWalletConnection();
  }, []);

  const checkWalletConnection = async () => {
    const address = await getWalletAddress();
    if (address && !walletInfo) {
      // Wallet is connected, fetch balance info
      const { getBalances } = await import('@/utils/stellar');
      const balances = await getBalances(address);
      onWalletChange(balances);
    }
  };

  const handleConnect = async () => {
    setIsConnecting(true);
    setError(null);

    try {
      const result = await connectFreighter();
      
      if (result.success && result.address) {
        // Fetch wallet balances
        const { getBalances } = await import('@/utils/stellar');
        const balances = await getBalances(result.address);
        onWalletChange(balances);
      } else {
        setError(result.error || 'Failed to connect wallet');
      }
    } catch (err) {
      setError(`Connection failed: ${err}`);
    } finally {
      setIsConnecting(false);
    }
  };

  const handleDisconnect = () => {
    onWalletChange(null);
    setError(null);
  };

  const formatAddress = (address: string) => {
    return `${address.slice(0, 4)}...${address.slice(-4)}`;
  };

  if (walletInfo?.isConnected) {
    return (
      <div className="bg-white rounded-lg border border-green-200 p-6 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <div className="w-10 h-10 bg-green-100 rounded-full flex items-center justify-center">
              <Wallet className="w-5 h-5 text-green-600" />
            </div>
            <div>
              <h3 className="font-semibold text-gray-900">Wallet Connected</h3>
              <p className="text-sm text-gray-600">
                {formatAddress(walletInfo.address)}
              </p>
            </div>
          </div>
          <button
            onClick={handleDisconnect}
            className="inline-flex items-center px-3 py-2 border border-red-300 text-sm font-medium rounded-md text-red-700 bg-red-50 hover:bg-red-100 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-red-500 transition-colors"
          >
            <X className="w-4 h-4 mr-2" />
            Disconnect
          </button>
        </div>

        {/* Wallet Balances */}
        <div className="mt-4 grid grid-cols-3 gap-4">
          <div className="text-center p-3 bg-gray-50 rounded-lg">
            <p className="text-sm text-gray-600">XLM Balance</p>
            <p className="text-lg font-semibold text-gray-900">
              {walletInfo.xlmBalance}
            </p>
          </div>
          <div className="text-center p-3 bg-green-50 rounded-lg">
            <p className="text-sm text-green-600">PDOT Tokens</p>
            <p className="text-lg font-semibold text-green-700">
              {walletInfo.testTokenBalance}
            </p>
          </div>
          <div className="text-center p-3 bg-green-50 rounded-lg">
            <p className="text-sm text-green-600">pTokens</p>
            <p className="text-lg font-semibold text-green-700">
              {walletInfo.pTokenBalance}
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="text-center">
        <div className="w-16 h-16 bg-gray-100 rounded-full flex items-center justify-center mx-auto mb-4">
          <Wallet className="w-8 h-8 text-gray-400" />
        </div>
        <h3 className="text-lg font-semibold text-gray-900 mb-2">
          Connect Your Wallet
        </h3>
        <p className="text-gray-600 mb-6">
          Connect your Freighter wallet to start using the vault
        </p>

        {error && (
          <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md">
            <div className="flex items-center">
              <AlertCircle className="w-4 h-4 text-red-500 mr-2" />
              <p className="text-sm text-red-700">{error}</p>
            </div>
          </div>
        )}

        <button
          onClick={handleConnect}
          disabled={isConnecting}
          className="inline-flex items-center px-6 py-3 border border-transparent text-base font-medium rounded-md text-white bg-green-600 hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          <Wallet className="w-5 h-5 mr-2" />
          {isConnecting ? 'Connecting...' : 'Connect Freighter'}
        </button>

        <p className="mt-4 text-xs text-gray-500">
          Don't have Freighter?{' '}
          <a
            href="https://freighter.app/"
            target="_blank"
            rel="noopener noreferrer"
            className="text-green-600 hover:text-green-700 underline"
          >
            Install here
          </a>
        </p>
      </div>
    </div>
  );
} 