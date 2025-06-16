'use client';

import { useState, useCallback } from 'react';
import { Shield, Github, ExternalLink } from 'lucide-react';
import ConnectWallet from '@/components/ConnectWallet';
import VaultInterface from '@/components/VaultInterface';
import VaultStats from '@/components/VaultStats';
import ThemeToggle from '@/components/ThemeToggle';
import DashboardWithCarousel from '@/components/DashboardWithCarousel';
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
    <div className="min-h-screen theme-bg">
      {/* Floating Header */}
      <header className="floating-header sticky top-4 z-50">
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
                <p className="text-xs text-slate-900 dark:text-slate-300">
                Peridot • Testnet
                </p>
              </div>
            </div>
            
            {/* Links and Theme Toggle */}
            <div className="flex items-center space-x-4">
              <ThemeToggle />
              <div className="w-px h-6 bg-slate-400 dark:bg-slate-500"></div>
              <a
                href="https://github.com/PeridotFinance/Peridot-Soroban/tree/main#"
                target="_blank"
                rel="noopener noreferrer"
                className="text-slate-700 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200 transition-colors duration-200"
                title="View on GitHub"
              >
                <Github className="w-5 h-5" />
              </a>
              <a
                href="https://peridot-finance.gitbook.io/"
                target="_blank"
                rel="noopener noreferrer"
                className="text-slate-700 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200 transition-colors duration-200"
                title="Learn about Soroban"
              >
                <ExternalLink className="w-5 h-5" />
              </a>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8 pt-4">
        {/* Introduction */}
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-slate-900 dark:text-slate-100 mb-2">
            Peridot DeFi Vault
          </h1>
          <p className="text-lg text-slate-800 dark:text-slate-300">
            Deposit PDOT tokens and receive pTokens representing your share of the vault
          </p>
          <div className="mt-4 space-y-3">
            <div className="inline-flex items-center px-4 py-2 glass border-emerald-200/50 dark:border-emerald-400/20 text-sm text-emerald-800 dark:text-emerald-200 rounded-full">
              <div className="w-2 h-2 bg-emerald-500 rounded-full mr-2 animate-pulse"></div>
              Connected to Stellar Testnet
            </div>
            <div className="block">
              <a 
                href="/carousel" 
                className="inline-flex items-center px-6 py-3 bg-gradient-to-r from-purple-600 to-blue-600 hover:from-purple-700 hover:to-blue-700 text-white font-semibold rounded-xl transition-all duration-300 shadow-lg hover:shadow-xl transform hover:scale-105"
              >
                <span className="mr-2">🎠</span>
                View 3D Carousel Dashboard
              </a>
            </div>
          </div>
        </div>

        {/* Dashboard Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {/* Left Column */}
          <div className="space-y-6">
            {/* Wallet Connection */}
            <div className="glass-card">
              <ConnectWallet 
                walletInfo={walletInfo} 
                onWalletChange={handleWalletChange} 
              />
            </div>


          </div>

          {/* Right Column */}
          <div className="space-y-6">
            {/* Vault Interface */}
            <div className="glass-card">
              <VaultInterface 
                walletInfo={walletInfo} 
                onTransactionComplete={handleTransactionComplete} 
              />
            </div>
          </div>
        </div>

        {/* Vault Stats - Full Width */}
        <div className="mt-8 glass-card">
          <VaultStats 
            walletInfo={walletInfo} 
            refreshTrigger={refreshTrigger} 
          />
        </div>

        {/* Contract Information */}
        <div className="mt-8 glass-card">
          <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-4">
            Contract Information
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-slate-800 dark:text-slate-300">Vault Contract:</span>
              <p className="font-mono text-xs text-slate-900 dark:text-slate-100 break-all mt-1 p-2 bg-slate-100 dark:bg-slate-800 rounded-lg">
                {process.env.NEXT_PUBLIC_VAULT_CONTRACT}
              </p>
            </div>
            <div>
              <span className="text-slate-800 dark:text-slate-300">Token Contract:</span>
              <p className="font-mono text-xs text-slate-900 dark:text-slate-100 break-all mt-1 p-2 bg-slate-100 dark:bg-slate-800 rounded-lg">
                {process.env.NEXT_PUBLIC_TOKEN_CONTRACT}
              </p>
            </div>
            <div>
              <span className="text-slate-800 dark:text-slate-300">Network:</span>
              <p className="text-slate-900 dark:text-slate-100 font-medium">Stellar Testnet</p>
            </div>
            <div>
              <span className="text-slate-800 dark:text-slate-300">Protocol:</span>
              <p className="text-slate-900 dark:text-slate-100 font-medium">Soroban Smart Contracts</p>
            </div>
          </div>
        </div>

        {/* User Guide */}
        <div className="mt-8 glass-card border-blue-200/50 dark:border-blue-400/20 bg-gradient-to-r from-blue-50/50 to-indigo-50/50 dark:from-blue-950/30 dark:to-indigo-950/30">
          <h3 className="text-lg font-semibold text-blue-900 dark:text-blue-100 mb-3">
            How to Use the Vault
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
            <div className="flex items-start space-x-3">
              <div className="w-8 h-8 bg-gradient-to-br from-blue-500 to-blue-600 text-white rounded-full flex items-center justify-center text-sm font-bold shadow-lg">
                1
              </div>
              <div>
                <h4 className="font-semibold text-blue-900 dark:text-blue-100">Connect Wallet</h4>
                <p className="text-blue-800 dark:text-blue-300">
                  Connect your Freighter wallet to get started
                </p>
              </div>
            </div>
            <div className="flex items-start space-x-3">
              <div className="w-8 h-8 bg-gradient-to-br from-blue-500 to-blue-600 text-white rounded-full flex items-center justify-center text-sm font-bold shadow-lg">
                2
              </div>
              <div>
                <h4 className="font-semibold text-blue-900 dark:text-blue-100">Get PDOT Tokens</h4>
                <p className="text-blue-800 dark:text-blue-300">
                  Mint free PDOT tokens for the testnet
                </p>
              </div>
            </div>
            <div className="flex items-start space-x-3">
              <div className="w-8 h-8 bg-gradient-to-br from-blue-500 to-blue-600 text-white rounded-full flex items-center justify-center text-sm font-bold shadow-lg">
                3
              </div>
              <div>
                <h4 className="font-semibold text-blue-900 dark:text-blue-100">Deposit & Earn</h4>
                <p className="text-blue-800 dark:text-blue-300">
                  Deposit PDOT tokens to receive pTokens
                </p>
              </div>
            </div>
          </div>
        </div>
      </main>

      {/* Footer */}
      <footer className="glass border-t border-white/20 dark:border-white/10 mt-12 mx-4 mb-4 rounded-2xl">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">
          <div className="text-center text-sm text-slate-800 dark:text-slate-400">
            <p>
              Built in Berlin by{' '}
              <a
                href="https://github.com/PeridotFinance/Peridot-Soroban/tree/main#"
                target="_blank"
                rel="noopener noreferrer"
                className="text-emerald-700 hover:text-emerald-800 dark:text-emerald-400 dark:hover:text-emerald-300 underline font-medium transition-colors duration-200"
              >
                Peridot
              </a>{' '}
              with{' '}
              <a
                href="https://stellar.org/soroban"
                target="_blank"
                rel="noopener noreferrer"
                className="text-emerald-700 hover:text-emerald-800 dark:text-emerald-400 dark:hover:text-emerald-300 underline transition-colors duration-200"
              >
                Soroban
              </a>{' '}
              on{' '}
              <a
                href="https://stellar.org"
                target="_blank"
                rel="noopener noreferrer"
                className="text-emerald-700 hover:text-emerald-800 dark:text-emerald-400 dark:hover:text-emerald-300 underline transition-colors duration-200"
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
