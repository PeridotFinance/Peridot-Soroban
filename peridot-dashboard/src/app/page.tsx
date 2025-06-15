'use client';

import { useState, useCallback } from 'react';
import { Shield, Github, ExternalLink } from 'lucide-react';
import ConnectWallet from '@/components/ConnectWallet';
import TokenManager from '@/components/TokenManager';
import VaultInterface from '@/components/VaultInterface';
import VaultStats from '@/components/VaultStats';
import { WalletInfo } from '@/utils/stellar';
import Image from 'next/image';

export default function Dashboard() {
  const [walletInfo, setWalletInfo] = useState<WalletInfo | null>(null);
  const [refreshTrigger, setRefreshTrigger] = useState(0);

  const handleWalletChange = useCallback((info: WalletInfo | null) => {
    setWalletInfo(info);
    if (info) {
      // Trigger a refresh of stats when wallet connects
      setRefreshTrigger(prev => prev + 1);
    }
  }, []);

  const handleTokensMinted = useCallback(async () => {
    if (walletInfo?.address) {
      // Refresh wallet balances after minting
      const { getBalances } = await import('@/utils/stellar');
      const updatedBalances = await getBalances(walletInfo.address);
      setWalletInfo(updatedBalances);
      setRefreshTrigger(prev => prev + 1);
    }
  }, [walletInfo?.address]);

  const handleTransactionComplete = useCallback(async () => {
    if (walletInfo?.address) {
      // Add a small delay to ensure ledger state has updated
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Refresh wallet balances after transactions
      const { getBalances } = await import('@/utils/stellar');
      const updatedBalances = await getBalances(walletInfo.address);
      setWalletInfo(updatedBalances);
      setRefreshTrigger(prev => prev + 1);
    }
  }, [walletInfo?.address]);

  return (
    <div className="min-h-screen bg-gradient-to-br from-green-50 via-white to-green-50">
      {/* Header */}
      <header className="bg-white border-b border-green-200 shadow-sm">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            <div className="flex items-center space-x-3">
              <div className="w-10 h-10 flex items-center justify-center">
                <Image
                  src="/logo-optimized.svg"
                  alt="Peridot Logo"
                  width={40}
                  height={40}
                  className="w-full h-full"
                />
              </div>
              <div>
                <h1 className="text-xl font-bold text-gray-900">
                  Peridot Vault Dashboard
                </h1>
                <p className="text-xs text-gray-600">
                  Testnet • DeFi Vault Protocol
                </p>
              </div>
            </div>
            
            {/* Links */}
            <div className="flex items-center space-x-4">
              <a
                href="https://github.com/PeridotFinance/Peridot-Soroban/tree/main#"
                target="_blank"
                rel="noopener noreferrer"
                className="text-gray-500 hover:text-gray-700 transition-colors"
                title="View on GitHub"
              >
                <Github className="w-5 h-5" />
              </a>
              <a
                href="https://peridot-finance.gitbook.io/"
                target="_blank"
                rel="noopener noreferrer"
                className="text-gray-500 hover:text-gray-700 transition-colors"
                title="Learn about Soroban"
              >
                <ExternalLink className="w-5 h-5" />
              </a>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Introduction */}
        <div className="text-center mb-8">
          <h2 className="text-3xl font-bold text-gray-900 mb-2">
            Peridot DeFi Vault
          </h2>
          <p className="text-lg text-gray-600">
            Deposit PDOT tokens and receive pTokens representing your share of the vault
          </p>
          <div className="mt-4 inline-flex items-center px-3 py-1 bg-green-100 border border-green-200 rounded-full text-sm text-green-800">
            <div className="w-2 h-2 bg-green-500 rounded-full mr-2"></div>
            Connected to Stellar Testnet
          </div>
        </div>

        {/* Dashboard Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {/* Left Column */}
          <div className="space-y-6">
            {/* Wallet Connection */}
            <ConnectWallet 
              walletInfo={walletInfo} 
              onWalletChange={handleWalletChange} 
            />

            {/* Token Manager */}
            {walletInfo?.isConnected && (
              <TokenManager 
                walletInfo={walletInfo} 
                onTokensMinted={handleTokensMinted} 
              />
            )}
          </div>

          {/* Right Column */}
          <div className="space-y-6">
            {/* Vault Interface */}
            <VaultInterface 
              walletInfo={walletInfo} 
              onTransactionComplete={handleTransactionComplete} 
            />
          </div>
        </div>

        {/* Vault Stats - Full Width */}
        <div className="mt-8">
          <VaultStats 
            walletInfo={walletInfo} 
            refreshTrigger={refreshTrigger} 
          />
        </div>

        {/* Contract Information */}
        <div className="mt-8 bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
          <h3 className="text-lg font-semibold text-gray-900 mb-4">
            Contract Information
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-gray-600">Vault Contract:</span>
              <p className="font-mono text-xs text-gray-900 break-all">
                {process.env.NEXT_PUBLIC_VAULT_CONTRACT}
              </p>
            </div>
            <div>
              <span className="text-gray-600">Token Contract:</span>
              <p className="font-mono text-xs text-gray-900 break-all">
                {process.env.NEXT_PUBLIC_TOKEN_CONTRACT}
              </p>
            </div>
            <div>
              <span className="text-gray-600">Network:</span>
              <p className="text-gray-900">Stellar Testnet</p>
            </div>
            <div>
              <span className="text-gray-600">Protocol:</span>
              <p className="text-gray-900">Soroban Smart Contracts</p>
            </div>
          </div>
        </div>

        {/* User Guide */}
        <div className="mt-8 bg-blue-50 border border-blue-200 rounded-lg p-6">
          <h3 className="text-lg font-semibold text-blue-900 mb-3">
            How to Use the Vault
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
            <div className="flex items-start space-x-3">
              <div className="w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-xs font-bold">
                1
              </div>
              <div>
                <h4 className="font-medium text-blue-900">Connect Wallet</h4>
                <p className="text-blue-700">
                  Connect your Freighter wallet to get started
                </p>
              </div>
            </div>
            <div className="flex items-start space-x-3">
              <div className="w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-xs font-bold">
                2
              </div>
              <div>
                <h4 className="font-medium text-blue-900">Get PDOT Tokens</h4>
                <p className="text-blue-700">
                  Mint free PDOT tokens for the testnet
                </p>
              </div>
            </div>
            <div className="flex items-start space-x-3">
              <div className="w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-xs font-bold">
                3
              </div>
              <div>
                <h4 className="font-medium text-blue-900">Deposit & Earn</h4>
                <p className="text-blue-700">
                  Deposit PDOT tokens to receive pTokens
                </p>
              </div>
            </div>
          </div>
        </div>
      </main>

      {/* Footer */}
      <footer className="bg-gray-50 border-t border-gray-200 mt-12">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">
          <div className="text-center text-sm text-gray-600">
            <p>
              Built in Berlin by{' '}
              <a
                href="https://github.com/PeridotFinance/Peridot-Soroban/tree/main#"
                target="_blank"
                rel="noopener noreferrer"
                className="text-green-600 hover:text-green-700 underline font-medium"
              >
                Peridot
              </a>{' '}
              with{' '}
              <a
                href="https://stellar.org/soroban"
                target="_blank"
                rel="noopener noreferrer"
                className="text-green-600 hover:text-green-700 underline"
              >
                Soroban
              </a>{' '}
              on{' '}
              <a
                href="https://stellar.org"
                target="_blank"
                rel="noopener noreferrer"
                className="text-green-600 hover:text-green-700 underline"
              >
                Stellar
              </a>
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}
