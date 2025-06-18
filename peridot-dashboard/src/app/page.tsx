'use client';

import { useState, useCallback, useEffect, useRef } from 'react';
import { Shield, Github, ExternalLink, TrendingUp, Zap, Bot } from 'lucide-react';
import ConnectWallet from '@/components/ConnectWallet';
import VaultInterface from '@/components/VaultInterface';
import VaultStats from '@/components/VaultStats';
import VaultPerformanceChart from '@/components/VaultPerformanceChart';
import ThemeToggle from '@/components/ThemeToggle';
import DashboardWithCarousel from '@/components/DashboardWithCarousel';
import AIAgent from '@/components/AIAgent';
import { WalletInfo } from '@/utils/stellar';
import Image from 'next/image';

export default function Dashboard() {
  const [walletInfo, setWalletInfo] = useState<WalletInfo | null>(null);
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const [activeTab, setActiveTab] = useState<'lending' | 'faucet' | 'ai-agent'>('lending');
  const [selectedAsset, setSelectedAsset] = useState<'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL'>('PDOT');
  const [isTabSwitcherSticky, setIsTabSwitcherSticky] = useState(false);
  const tabSwitcherRef = useRef<HTMLDivElement>(null);

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

  // Handle scroll detection for sticky tab switcher
  useEffect(() => {
    const handleScroll = () => {
      if (tabSwitcherRef.current) {
        const rect = tabSwitcherRef.current.getBoundingClientRect();
        const isSticky = rect.top <= 80; // Account for header height
        setIsTabSwitcherSticky(isSticky);
      }
    };

    window.addEventListener('scroll', handleScroll);
    return () => window.removeEventListener('scroll', handleScroll);
  }, []);

  // Reusable tab switcher component
  const TabSwitcher = ({ isCompact = false }: { isCompact?: boolean }) => (
    <div className={`relative backdrop-blur-2xl bg-white/10 dark:bg-white/5 rounded-3xl p-2 border border-white/20 dark:border-white/10 shadow-2xl shadow-black/10 flex flex-row ${isCompact ? 'scale-75 sm:scale-75 scale-[0.6] origin-center' : ''}`}>
      <button
        onClick={() => setActiveTab('lending')}
        className={`relative ${isCompact ? 'px-4 py-2' : 'px-8 py-4'} rounded-2xl font-semibold overflow-hidden group active:scale-95`}
        style={{
          background: activeTab === 'lending' 
            ? 'linear-gradient(135deg, rgba(255, 255, 255, 0.25), rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.05))'
            : 'transparent',
          backdropFilter: activeTab === 'lending' ? 'blur(20px)' : 'none',
          border: activeTab === 'lending' ? '1px solid rgba(255, 255, 255, 0.2)' : '1px solid transparent',
          boxShadow: activeTab === 'lending' 
            ? '0 8px 32px rgba(0, 0, 0, 0.1), inset 0 1px 0 rgba(255, 255, 255, 0.2)'
            : 'none',
          color: activeTab === 'lending' ? '#1e293b' : '#64748b',
          transform: activeTab === 'lending' ? 'scale(1.02)' : 'scale(1)',
          transition: activeTab === 'lending' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out'
        }}
        onMouseDown={(e) => {
          e.currentTarget.style.transform = activeTab === 'lending' ? 'scale(0.97)' : 'scale(0.95)';
          e.currentTarget.style.transition = 'transform 0.1s ease-out';
        }}
        onMouseUp={(e) => {
          e.currentTarget.style.transform = activeTab === 'lending' ? 'scale(1.02)' : 'scale(1)';
          e.currentTarget.style.transition = activeTab === 'lending' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out';
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.transform = activeTab === 'lending' ? 'scale(1.02)' : 'scale(1)';
          e.currentTarget.style.transition = activeTab === 'lending' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out';
        }}
      >
        <div className="absolute inset-0 bg-gradient-to-br from-white/10 via-transparent to-white/5 opacity-0 group-hover:opacity-100 transition-opacity duration-300 rounded-2xl"></div>
        <div className="relative flex items-center justify-center space-x-2">
          <TrendingUp className="w-4 h-4" />
          <span className="font-mono uppercase tracking-wide text-sm font-bold">Lending</span>
        </div>
      </button>
      
      <button
        onClick={() => setActiveTab('faucet')}
        className={`relative ${isCompact ? 'px-4 py-2' : 'px-8 py-4'} rounded-2xl font-semibold overflow-hidden group active:scale-95`}
        style={{
          background: activeTab === 'faucet' 
            ? 'linear-gradient(135deg, rgba(255, 255, 255, 0.25), rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.05))'
            : 'transparent',
          backdropFilter: activeTab === 'faucet' ? 'blur(20px)' : 'none',
          border: activeTab === 'faucet' ? '1px solid rgba(255, 255, 255, 0.2)' : '1px solid transparent',
          boxShadow: activeTab === 'faucet' 
            ? '0 8px 32px rgba(0, 0, 0, 0.1), inset 0 1px 0 rgba(255, 255, 255, 0.2)'
            : 'none',
          color: activeTab === 'faucet' ? '#1e293b' : '#64748b',
          transform: activeTab === 'faucet' ? 'scale(1.02)' : 'scale(1)',
          transition: activeTab === 'faucet' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out'
        }}
        onMouseDown={(e) => {
          e.currentTarget.style.transform = activeTab === 'faucet' ? 'scale(0.97)' : 'scale(0.95)';
          e.currentTarget.style.transition = 'transform 0.1s ease-out';
        }}
        onMouseUp={(e) => {
          e.currentTarget.style.transform = activeTab === 'faucet' ? 'scale(1.02)' : 'scale(1)';
          e.currentTarget.style.transition = activeTab === 'faucet' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out';
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.transform = activeTab === 'faucet' ? 'scale(1.02)' : 'scale(1)';
          e.currentTarget.style.transition = activeTab === 'faucet' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out';
        }}
      >
        <div className="absolute inset-0 bg-gradient-to-br from-white/10 via-transparent to-white/5 opacity-0 group-hover:opacity-100 transition-opacity duration-300 rounded-2xl"></div>
        <div className="relative flex items-center justify-center space-x-2">
          <Zap className="w-4 h-4" />
          <span className="font-mono uppercase tracking-wide text-sm font-bold">Faucet</span>
        </div>
      </button>
      
      <button
        onClick={() => setActiveTab('ai-agent')}
        className={`relative ${isCompact ? 'px-4 py-2' : 'px-8 py-4'} rounded-2xl font-semibold overflow-hidden group active:scale-95`}
        style={{
          background: activeTab === 'ai-agent' 
            ? 'linear-gradient(135deg, rgba(255, 255, 255, 0.25), rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.05))'
            : 'transparent',
          backdropFilter: activeTab === 'ai-agent' ? 'blur(20px)' : 'none',
          border: activeTab === 'ai-agent' ? '1px solid rgba(255, 255, 255, 0.2)' : '1px solid transparent',
          boxShadow: activeTab === 'ai-agent' 
            ? '0 8px 32px rgba(0, 0, 0, 0.1), inset 0 1px 0 rgba(255, 255, 255, 0.2)'
            : 'none',
          color: activeTab === 'ai-agent' ? '#1e293b' : '#64748b',
          transform: activeTab === 'ai-agent' ? 'scale(1.02)' : 'scale(1)',
          transition: activeTab === 'ai-agent' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out'
        }}
        onMouseDown={(e) => {
          e.currentTarget.style.transform = activeTab === 'ai-agent' ? 'scale(0.97)' : 'scale(0.95)';
          e.currentTarget.style.transition = 'transform 0.1s ease-out';
        }}
        onMouseUp={(e) => {
          e.currentTarget.style.transform = activeTab === 'ai-agent' ? 'scale(1.02)' : 'scale(1)';
          e.currentTarget.style.transition = activeTab === 'ai-agent' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out';
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.transform = activeTab === 'ai-agent' ? 'scale(1.02)' : 'scale(1)';
          e.currentTarget.style.transition = activeTab === 'ai-agent' 
            ? 'all 0.4s cubic-bezier(0.4, 0, 0.2, 1)' 
            : 'all 0.15s ease-out';
        }}
      >
        <div className="absolute inset-0 bg-gradient-to-br from-white/10 via-transparent to-white/5 opacity-0 group-hover:opacity-100 transition-opacity duration-300 rounded-2xl"></div>
        <div className="relative flex items-center justify-center space-x-2">
          <Bot className="w-4 h-4" />
          <span className="font-mono uppercase tracking-wide text-sm font-bold">AI Agent</span>
        </div>
      </button>
    </div>
  );

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
            
            {/* Sticky Tab Switcher and Links */}
            <div className="flex items-center space-x-2 sm:space-x-4">
              {/* Sticky Tab Switcher - show on mobile when sticky */}
              {isTabSwitcherSticky && (
                <div className="block">
                  <TabSwitcher isCompact />
                </div>
              )}
              
              {/* Theme Toggle - hide on mobile when tab switcher is sticky */}
              <div className={`transition-opacity duration-300 ${isTabSwitcherSticky ? 'hidden sm:block' : 'block'}`}>
                <ThemeToggle />
              </div>
              
              {/* Links - hidden on mobile, visible on desktop */}
              <div className="hidden sm:flex items-center space-x-4">
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
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8 pt-4">
        {/* Introduction */}
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-slate-900 dark:text-slate-100 mb-2">
            Peridot Protocol
          </h1>
          <p className="text-lg text-slate-800 dark:text-slate-300">
            The only true crosschain Lend & Borrow platform
          </p>
        </div>

        {/* Glassy iOS-Style Tabs */}
        <div ref={tabSwitcherRef} className={`flex justify-center mb-8 transition-opacity duration-300 ${isTabSwitcherSticky ? 'opacity-0' : 'opacity-100'}`}>
          <TabSwitcher />
        </div>

        {/* Tab Content */}
        {activeTab === 'lending' ? (
          // Lending & Borrowing Interface
          <div className="max-w-6xl mx-auto space-y-8">
            <div className="glass-card">
              <ConnectWallet 
                walletInfo={walletInfo} 
                onWalletChange={handleWalletChange}
                mode="lending"
                selectedAsset={selectedAsset}
                onSelectedAssetChange={setSelectedAsset}
              />
            </div>
            
            {/* Vault Performance Chart - Only show when wallet is connected */}
            {walletInfo && <VaultPerformanceChart walletInfo={walletInfo} selectedAsset={selectedAsset} />}
          </div>
        ) : activeTab === 'ai-agent' ? (
          // AI Agent Interface
          <AIAgent walletInfo={walletInfo} />
        ) : (
          // Faucet Mode - Original Dashboard
          <>
            {/* User Guide - Moved to top */}
            <div className="mb-8 glass-card border-slate-200/50 dark:border-slate-400/20 bg-gradient-to-r from-slate-50/50 to-gray-50/50 dark:from-slate-950/30 dark:to-gray-950/30">
              <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-3">
                How to Use the Vault
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
                <div className="flex items-start space-x-3">
                  <div className="w-8 h-8 bg-gradient-to-br from-emerald-500 to-teal-600 text-white rounded-full flex items-center justify-center text-sm font-bold shadow-lg">
                    1
                  </div>
                  <div>
                    <h4 className="font-semibold text-slate-900 dark:text-slate-100">Connect Wallet</h4>
                    <p className="text-slate-700 dark:text-slate-300">
                      Connect your Freighter wallet to get started
                    </p>
                  </div>
                </div>
                <div className="flex items-start space-x-3">
                  <div className="w-8 h-8 bg-gradient-to-br from-emerald-500 to-teal-600 text-white rounded-full flex items-center justify-center text-sm font-bold shadow-lg">
                    2
                  </div>
                  <div>
                    <h4 className="font-semibold text-slate-900 dark:text-slate-100">Get PDOT Tokens</h4>
                    <p className="text-slate-700 dark:text-slate-300">
                      Mint free PDOT tokens for the testnet
                    </p>
                  </div>
                </div>
                <div className="flex items-start space-x-3">
                  <div className="w-8 h-8 bg-gradient-to-br from-emerald-500 to-teal-600 text-white rounded-full flex items-center justify-center text-sm font-bold shadow-lg">
                    3
                  </div>
                  <div>
                    <h4 className="font-semibold text-slate-900 dark:text-slate-100">Deposit & Earn</h4>
                    <p className="text-slate-700 dark:text-slate-300">
                      Deposit PDOT tokens to receive pTokens
                    </p>
                  </div>
                </div>
              </div>
            </div>

            {/* Dashboard Grid */}
            <div className="grid grid-cols-1  gap-8">
              {/* Left Column */}
              <div className="space-y-6">
                {/* Wallet Connection */}
                <div className="glass-card">
                  <ConnectWallet 
                    walletInfo={walletInfo} 
                    onWalletChange={handleWalletChange}
                    mode="faucet"
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
          </>
        )}

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

        {/* 3D Dashboard Button - Bottom Section */}
        <div className="mt-12 text-center">
          <div className="block">
            <a 
              href="/carousel" 
              className="group relative inline-flex items-center px-6 py-3 bg-gradient-to-r from-purple-600/20 via-blue-600/15 to-indigo-600/20 hover:from-purple-500/30 hover:via-blue-500/25 hover:to-indigo-500/30 active:from-purple-700/40 active:via-blue-700/35 active:to-indigo-700/40 rounded-xl border border-purple-400/10 hover:border-purple-300/20 active:border-purple-300/30 focus:outline-none focus:ring-2 focus:ring-purple-400/30 transition-all duration-200 shadow-lg hover:shadow-purple-500/20 hover:shadow-xl active:shadow-purple-500/30 backdrop-blur-lg transform hover:scale-105 active:scale-95 touch-manipulation"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-200 rounded-xl"></div>
              <span className="relative font-semibold text-slate-700 dark:text-slate-300 group-hover:text-slate-900 dark:group-hover:text-slate-100 group-active:text-slate-800 dark:group-active:text-slate-200 transition-colors duration-200">
                🎯 Experience 3D Dashboard
              </span>
            </a>
          </div>
          <p className="mt-3 text-sm text-slate-600 dark:text-slate-400">
            Explore our immersive 3D interface for advanced portfolio management
          </p>
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
