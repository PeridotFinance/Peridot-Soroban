'use client';

import { useState } from 'react';
import { Wallet, LogOut, Copy, CheckCircle, AlertCircle, ExternalLink, Coins, Loader, Zap, TrendingUp, PiggyBank, ArrowUpCircle, ArrowDownCircle, BarChart3, DollarSign, Percent } from 'lucide-react';
import { connectFreighter, getBalances, formatNumber, WalletInfo, mintTestTokens } from '@/utils/stellar';

// Add the CSS styles for loading animations and blend-in effects
const loadingStyles = `
  .pl__ring {
    animation: ring 2s ease-out infinite;
    stroke: #10b981;
  }
  .pl__ring--a {
    stroke: #10b981;
  }
  .pl__ring--b {
    animation-delay: -0.25s;
    stroke: #059669;
  }
  .pl__ring--c {
    animation-delay: -0.5s;
    stroke: #047857;
  }
  .pl__ring--d {
    animation-delay: -0.75s;
    stroke: #065f46;
  }
  @keyframes ring {
    0%, 4% {
      stroke-dasharray: 0 660;
      stroke-width: 20;
      stroke-dashoffset: -330;
    }
    12% {
      stroke-dasharray: 60 600;
      stroke-width: 30;
      stroke-dashoffset: -335;
    }
    32% {
      stroke-dasharray: 60 600;
      stroke-width: 30;
      stroke-dashoffset: -595;
    }
    40%, 54% {
      stroke-dasharray: 0 660;
      stroke-width: 20;
      stroke-dashoffset: -660;
    }
    62% {
      stroke-dasharray: 60 600;
      stroke-width: 30;
      stroke-dashoffset: -665;
    }
    82% {
      stroke-dasharray: 60 600;
      stroke-width: 30;
      stroke-dashoffset: -925;
    }
    90%, 100% {
      stroke-dasharray: 0 660;
      stroke-width: 20;
      stroke-dashoffset: -990;
    }
  }

  /* Blend-in animations */
  @keyframes fadeInUp {
    from {
      opacity: 0;
      transform: translateY(20px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes fadeInScale {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  @keyframes slideInLeft {
    from {
      opacity: 0;
      transform: translateX(-20px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }

  @keyframes slideInRight {
    from {
      opacity: 0;
      transform: translateX(20px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .animate-fade-in-up {
    animation: fadeInUp 0.6s ease-out forwards;
  }

  .animate-fade-in-scale {
    animation: fadeInScale 0.5s ease-out forwards;
  }

  .animate-slide-in-left {
    animation: slideInLeft 0.4s ease-out forwards;
  }

  .animate-slide-in-right {
    animation: slideInRight 0.4s ease-out forwards;
  }

  .animate-fade-in {
    animation: fadeIn 0.8s ease-out forwards;
  }

  .animate-delay-100 {
    animation-delay: 0.1s;
  }

  .animate-delay-200 {
    animation-delay: 0.2s;
  }

  .animate-delay-300 {
    animation-delay: 0.3s;
  }

  .animate-delay-400 {
    animation-delay: 0.4s;
  }

  .animate-delay-500 {
    animation-delay: 0.5s;
  }

  /* Initial state for animations */
  .animate-fade-in-up,
  .animate-fade-in-scale,
  .animate-slide-in-left,
  .animate-slide-in-right,
  .animate-fade-in {
    opacity: 0;
  }

  /* Smooth gradient animations */
  @keyframes gradient-shift {
    0% {
      background-position: 0% 50%;
    }
    50% {
      background-position: 100% 50%;
    }
    100% {
      background-position: 0% 50%;
    }
  }

  @keyframes gradient-pulse {
    0%, 100% {
      background-position: 0% 50%;
      opacity: 0.8;
    }
    50% {
      background-position: 100% 50%;
      opacity: 1;
    }
  }
`;

interface ConnectWalletProps {
  walletInfo: WalletInfo | null;
  onWalletChange: (info: WalletInfo | null) => void;
  mode?: 'lending' | 'faucet';
}

export default function ConnectWallet({ walletInfo, onWalletChange, mode = 'faucet' }: ConnectWalletProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Remove activeTab as it's now handled at dashboard level

  // Inject loading styles
  if (typeof document !== 'undefined') {
    const styleId = 'loading-animations';
    if (!document.getElementById(styleId)) {
      const style = document.createElement('style');
      style.id = styleId;
      style.textContent = loadingStyles;
      document.head.appendChild(style);
    }
  }
  
  // Mint functionality
  const [isMinting, setIsMinting] = useState(false);
  const [mintingStatus, setMintingStatus] = useState<'idle' | 'success' | 'error'>('idle');
  const [mintError, setMintError] = useState<string | null>(null);

  // Lending/Borrowing functionality
  const [selectedAsset, setSelectedAsset] = useState<'PDOT' | 'XLM' | 'USDC' | 'PTOKENS'>('PDOT');
  const [lendingMode, setLendingMode] = useState<'lend' | 'borrow'>('lend');
  const [amount, setAmount] = useState('');
  const [isProcessing, setIsProcessing] = useState(false);
  
  // Modal functionality
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modalAsset, setModalAsset] = useState<'PDOT' | 'XLM' | 'USDC' | 'PTOKENS'>('PDOT');
  const [modalAction, setModalAction] = useState<'withdraw' | 'repay'>('withdraw');
  const [modalAmount, setModalAmount] = useState('');
  const [isModalProcessing, setIsModalProcessing] = useState(false);

  const handleConnect = async () => {
    setIsConnecting(true);
    setError(null);

    try {
      const result = await connectFreighter();
      if (result.success && result.address) {
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
  };

  const copyAddress = async () => {
    if (walletInfo?.address) {
      await navigator.clipboard.writeText(walletInfo.address);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const shortenAddress = (address: string) => {
    return `${address.slice(0, 6)}...${address.slice(-4)}`;
  };

  const openStellarExpert = () => {
    if (walletInfo?.address) {
      window.open(`https://testnet.steexp.com/account/${walletInfo.address}`, '_blank');
    }
  };

  const handleMint = async () => {
    if (!walletInfo?.address) return;

    setIsMinting(true);
    setMintError(null);
    setMintingStatus('idle');

    try {
      console.log('Starting mint process for:', walletInfo.address);
      const result = await mintTestTokens(walletInfo.address);
      
      if (result.success) {
        console.log('Mint successful:', result);
        setMintingStatus('success');
        // Refresh wallet balances after a brief delay
        setTimeout(async () => {
          const updatedBalances = await getBalances(walletInfo.address);
          onWalletChange(updatedBalances);
        }, 1000);
        setTimeout(() => setMintingStatus('idle'), 3000);
      } else {
        console.error('Mint failed:', result.error);
        setMintingStatus('error');
        // Provide more specific error messages
        if (result.error?.includes('0x7D82D5')) {
          setMintError('Authorization failed - server may be out of tokens');
        } else if (result.error?.includes('configuration')) {
          setMintError('Server configuration error');
        } else if (result.error?.includes('UnreachableCodeReached') || result.error?.includes('implementation issues')) {
          setMintError('Token contract issue - please contact support');
        } else {
          setMintError(result.error || 'Minting failed');
        }
      }
    } catch (err: any) {
      console.error('Mint exception:', err);
      setMintingStatus('error');
      if (err.message?.includes('fetch')) {
        setMintError('Network error - please try again');
      } else {
        setMintError(`Minting failed: ${err.message || err}`);
      }
    } finally {
      setIsMinting(false);
    }
  };

  // Mock data for lending interface
  const assetData = {
    PDOT: { balance: walletInfo?.testTokenBalance || '0', apy: '12.5', borrowed: '0', price: '0.85' },
    XLM: { balance: '0', apy: '8.2', borrowed: '0', price: '0.12' },
    USDC: { balance: '0', apy: '5.5', borrowed: '0', price: '1.00' },
    PTOKENS: { balance: walletInfo?.pTokenBalance || '0', apy: '15.8', borrowed: '0', price: '1.20' }
  };

  const handleLendingAction = async () => {
    if (!amount || !walletInfo?.address) return;
    
    setIsProcessing(true);
    
    // Simulate transaction
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    setIsProcessing(false);
    setAmount('');
  };

  // Modal handlers
  const openModal = (asset: typeof modalAsset) => {
    setSelectedAsset(asset); // Keep the asset active
    setModalAsset(asset);
    setModalAction(lendingMode === 'lend' ? 'withdraw' : 'repay');
    setModalAmount('');
    setIsModalOpen(true);
  };

  const closeModal = () => {
    setIsModalOpen(false);
    setModalAmount('');
  };

  const handleModalAction = async () => {
    if (!modalAmount || !walletInfo?.address) return;
    
    setIsModalProcessing(true);
    
    // Simulate transaction
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    setIsModalProcessing(false);
    setModalAmount('');
    closeModal();
  };

  if (!walletInfo?.isConnected) {
    return (
      <>
        <div className="text-center py-8 animate-fade-in-up">
          {/* Cyber Connection Interface */}
          <div className="relative mx-auto mb-6 animate-fade-in-scale animate-delay-200">
            <div className="w-20 h-20 mx-auto relative">
              {/* Rotating outer ring */}
              <div className="absolute inset-0 rounded-full border-2 border-slate-300 dark:border-slate-600 animate-spin" style={{animationDuration: '3s'}}></div>
              <div className="absolute inset-2 rounded-full border-2 border-cyan-400/50 animate-ping"></div>
              
              {/* Central icon */}
              <div className="absolute inset-3 rounded-full bg-gradient-to-br from-slate-800 via-slate-700 to-slate-900 dark:from-slate-700 dark:via-slate-600 dark:to-slate-800 flex items-center justify-center shadow-xl">
                <Wallet className="w-8 h-8 text-cyan-300" />
              </div>
              
              {/* Pulse effect */}
              <div className="absolute inset-0 rounded-full bg-gradient-to-r from-cyan-500/20 to-blue-500/20 animate-pulse"></div>
            </div>
          </div>
          
          {/* Cyber Error State */}
          {error && (
            <div className="mb-6 relative overflow-hidden rounded-xl bg-gradient-to-r from-red-500/2 via-red-600/1 to-orange-500/2 dark:from-red-400/3 dark:via-red-500/2 dark:to-orange-400/3 border border-red-400/10 dark:border-red-400/15 shadow-lg shadow-red-500/3 backdrop-blur-2xl animate-fade-in-up animate-delay-300">
              <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-red-400/30 to-orange-400/30"></div>
              <div className="relative p-4 flex items-center space-x-3">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-red-500/40 to-orange-500/40 backdrop-blur-md flex items-center justify-center shadow-lg">
                  <AlertCircle className="w-4 h-4 text-white" />
                </div>
                <div className="flex-1 text-left">
                  <p className="text-sm text-red-700 dark:text-red-300 font-mono">CONNECTION_ERROR: 0x{Math.random().toString(16).substr(2, 6).toUpperCase()}</p>
                  <p className="text-xs text-red-600 dark:text-red-400 font-mono opacity-80">{error}</p>
                </div>
              </div>
            </div>
          )}

          {/* Professional Peridot Connect Button */}
          <button
            onClick={handleConnect}
            disabled={isConnecting}
            className="w-full group relative overflow-hidden px-8 py-4 rounded-2xl border border-slate-200/20 dark:border-slate-700/30 hover:border-emerald-400/30 focus:outline-none focus:ring-2 focus:ring-emerald-400/20 focus:border-emerald-400/40 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-2xl hover:shadow-emerald-500/10 backdrop-blur-xl bg-white/5 dark:bg-slate-900/50 hover:bg-white/10 dark:hover:bg-slate-800/60 min-h-[68px] touch-manipulation animate-fade-in-up animate-delay-400"
          >
            <div className="absolute inset-0 bg-gradient-to-r from-emerald-500/5 via-transparent to-cyan-500/5 opacity-0 group-hover:opacity-100 transition-opacity duration-300 rounded-2xl"></div>
            
            {/* Scanning animation when connecting */}
            {isConnecting && (
              <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/8 to-transparent animate-pulse"></div>
            )}
            
            <div className="relative flex items-center justify-center space-x-3">
              {isConnecting ? (
                <>
                  <div className="w-5 h-5 border-2 border-emerald-600 border-t-transparent rounded-full animate-spin"></div>
                  <span className="text-lg font-semibold text-slate-700 dark:text-slate-200 tracking-wide">
                    Connecting...
                  </span>
                </>
              ) : (
                <>
                  <Wallet className="w-5 h-5 text-emerald-600 dark:text-emerald-400 group-hover:text-emerald-700 dark:group-hover:text-emerald-300 transition-all duration-300" />
                  <span className="text-lg font-semibold text-slate-700 dark:text-slate-200 group-hover:text-slate-900 dark:group-hover:text-white tracking-wide transition-colors duration-300">
                    Connect Freighter Wallet
                  </span>
                </>
              )}
            </div>
          </button>

          {/* Cyber Requirements Panel */}
          <div className="mt-4 relative overflow-hidden rounded-xl bg-gradient-to-r from-blue-500/2 via-indigo-500/1 to-purple-500/2 dark:from-blue-400/3 dark:via-indigo-400/2 dark:to-purple-400/3 border border-blue-400/10 dark:border-blue-400/15 shadow-lg shadow-blue-500/3 backdrop-blur-2xl animate-fade-in animate-delay-500">
            <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-blue-400/30 via-indigo-400/30 to-purple-400/30"></div>
            <div className="relative p-3">
              <p className="text-xs text-blue-700 dark:text-blue-300 font-mono">
                <span className="text-purple-600 dark:text-purple-400 font-bold">REQUIREMENTS:</span><br/>
                {'>'} Install{' '}
                <a
                  href="https://freighter.app/"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-cyan-600 dark:text-cyan-400 hover:text-cyan-500 dark:hover:text-cyan-300 underline font-bold transition-colors duration-200"
                >
                  FREIGHTER_WALLET.APP
                </a><br/>
                {'>'} Configure network: STELLAR_TESTNET
              </p>
            </div>
          </div>
        </div>
      </>
    );
  }

  return (
    <>
      {/* Cyber Wallet Connected Header */}
      <div className="flex items-center space-x-4 mb-6 animate-slide-in-left">
        <div className="relative w-12 h-12 rounded-xl overflow-hidden">
          {/* Animated background layers */}
          <div className="absolute inset-0 bg-gradient-to-br from-emerald-500 via-teal-500 to-cyan-500 animate-pulse"></div>
          <div className="absolute inset-0 bg-gradient-to-br from-emerald-400 to-teal-400 opacity-80"></div>
          <div className="relative w-full h-full flex items-center justify-center shadow-xl shadow-emerald-500/30">
            <Wallet className="w-6 h-6 text-white drop-shadow-lg" />
          </div>
          {/* Pulsing ring */}
          <div className="absolute inset-0 rounded-xl border-2 border-emerald-400/50 animate-ping"></div>
        </div>
        <div className="flex-1">
          <h3 className="text-lg font-bold bg-gradient-to-r from-slate-900 to-slate-700 dark:from-white dark:to-slate-200 bg-clip-text text-transparent">
            WALLET_CONNECTED
          </h3>
          <p className="text-sm text-slate-600 dark:text-slate-400 font-mono">
            {'>'} freighter_protocol_active
          </p>
        </div>
        <div className="flex items-center space-x-2">
          <button
            onClick={copyAddress}
            className="group relative p-2 bg-gradient-to-r from-slate-600/20 to-slate-700/20 hover:from-slate-500/30 hover:to-slate-600/30 active:from-slate-700/40 active:to-slate-800/40 rounded-lg border border-slate-400/10 hover:border-slate-300/20 active:border-slate-300/30 focus:outline-none focus:ring-2 focus:ring-slate-400/30 transition-all duration-200 shadow-lg hover:shadow-slate-500/10 backdrop-blur-lg transform hover:scale-105 active:scale-95 touch-manipulation"
            title={copied ? 'Copied!' : 'Copy address'}
          >
            {copied ? (
              <CheckCircle className="w-4 h-4 text-emerald-400" />
            ) : (
              <Copy className="w-4 h-4 text-slate-500 dark:text-slate-400 group-hover:text-slate-700 dark:group-hover:text-slate-300 transition-colors duration-200" />
            )}
          </button>
          <button
            onClick={openStellarExpert}
            className="group relative p-2 bg-gradient-to-r from-blue-600/20 to-blue-700/20 hover:from-blue-500/30 hover:to-blue-600/30 active:from-blue-700/40 active:to-blue-800/40 rounded-lg border border-blue-400/10 hover:border-blue-300/20 active:border-blue-300/30 focus:outline-none focus:ring-2 focus:ring-blue-400/30 transition-all duration-200 shadow-lg hover:shadow-blue-500/10 backdrop-blur-lg transform hover:scale-105 active:scale-95 touch-manipulation"
            title="View on Stellar Expert"
          >
            <ExternalLink className="w-4 h-4 text-blue-500 dark:text-blue-400 group-hover:text-blue-700 dark:group-hover:text-blue-300 transition-colors duration-200" />
          </button>
        </div>
      </div>

      {/* Content based on mode */}
      <div key={mode} className="animate-fade-in-up animate-delay-200">
        {mode === 'lending' ? (
          renderLendingInterface()
        ) : (
          renderFaucetInterface()
        )}
      </div>

      {/* Token Detail Modal */}
      {isModalOpen && (
        <div 
          className="fixed inset-0 z-50 flex items-end sm:items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-fade-in"
          onClick={closeModal}
        >
          <div 
            className="w-full max-w-md bg-gradient-to-br from-white/95 via-white/98 to-white/95 dark:from-slate-800/40 dark:via-slate-700/20 dark:to-slate-800/40 border border-slate-200/60 dark:border-slate-600/30 rounded-3xl shadow-2xl backdrop-blur-3xl animate-fade-in-up max-h-[90vh] overflow-y-auto"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="relative p-6">
              {/* Header */}
              <div className="flex items-center justify-between mb-6">
                <div className="flex items-center space-x-3">
                                     <div className="w-12 h-12 rounded-xl flex items-center justify-center border border-blue-400/20 bg-gradient-to-r from-blue-500 to-cyan-500">
                    <Coins className="w-6 h-6 text-white" />
                  </div>
                  <div>
                    <h3 className="text-xl font-bold text-slate-900 dark:text-white">{modalAsset}</h3>
                    <p className="text-sm text-slate-600 dark:text-slate-400">Token Details</p>
                  </div>
                </div>
                <button
                  onClick={closeModal}
                  className="p-2 rounded-xl bg-gradient-to-r from-slate-200/60 to-slate-300/60 hover:from-slate-300/80 hover:to-slate-400/80 dark:from-slate-600/20 dark:to-slate-700/20 dark:hover:from-slate-500/30 dark:hover:to-slate-600/30 border border-slate-400/20 hover:border-slate-500/30 dark:border-slate-400/10 dark:hover:border-slate-300/20 text-slate-700 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-300 transition-all duration-200"
                >
                  <span className="text-xl">×</span>
                </button>
              </div>

              {/* Token Stats */}
              <div className="grid grid-cols-2 gap-4 mb-6">
                <div className="p-4 bg-gradient-to-br from-blue-500/10 via-cyan-500/5 to-purple-500/10 border border-blue-400/10 rounded-2xl backdrop-blur-xl">
                  <p className="text-xs text-slate-600 dark:text-slate-400 mb-1">Your Balance</p>
                  <p className="text-lg font-bold text-slate-900 dark:text-white">{assetData[modalAsset].balance}</p>
                  <p className="text-xs text-blue-500 font-semibold">{modalAsset}</p>
                </div>
                <div className="p-4 bg-gradient-to-br from-emerald-500/10 via-green-500/5 to-teal-500/10 border border-emerald-400/10 rounded-2xl backdrop-blur-xl">
                  <p className="text-xs text-slate-600 dark:text-slate-400 mb-1">Current APY</p>
                  <p className="text-lg font-bold text-emerald-600 dark:text-emerald-400">{assetData[modalAsset].apy}%</p>
                  <p className="text-xs text-emerald-500 font-semibold">Annual</p>
                </div>
                <div className="p-4 bg-gradient-to-br from-purple-500/10 via-indigo-500/5 to-blue-500/10 border border-purple-400/10 rounded-2xl backdrop-blur-xl">
                  <p className="text-xs text-slate-600 dark:text-slate-400 mb-1">Price USD</p>
                  <p className="text-lg font-bold text-slate-900 dark:text-white">${assetData[modalAsset].price}</p>
                  <p className="text-xs text-purple-500 font-semibold">Market Rate</p>
                </div>
                <div className="p-4 bg-gradient-to-br from-orange-500/10 via-yellow-500/5 to-red-500/10 border border-orange-400/10 rounded-2xl backdrop-blur-xl">
                  <p className="text-xs text-slate-600 dark:text-slate-400 mb-1">Total Liquidity</p>
                  <p className="text-lg font-bold text-slate-900 dark:text-white">$1.2M</p>
                  <p className="text-xs text-orange-500 font-semibold">Available</p>
                </div>
              </div>

              {/* Action Tabs */}
              <div className="flex items-center justify-center mb-6">
                <div className="relative p-1 bg-gradient-to-r from-slate-100/80 via-slate-50/90 to-slate-100/80 dark:from-slate-700/50 dark:via-slate-600/30 dark:to-slate-700/50 rounded-2xl backdrop-blur-2xl border border-slate-300/40 dark:border-slate-600/30">
                  <div className="flex relative">
                    <div 
                      className={`absolute top-0 bottom-0 w-1/2 bg-gradient-to-r from-slate-200/60 via-slate-100/70 to-slate-200/60 dark:from-white/10 dark:via-white/5 dark:to-white/10 rounded-xl shadow-lg transition-all duration-500 ease-out backdrop-blur-xl ${
                        modalAction === 'withdraw' ? 'left-0' : 'left-1/2'
                      }`}
                    />
                    
                    <button
                      onClick={() => setModalAction('withdraw')}
                      className={`relative px-6 py-3 rounded-xl font-semibold transition-all duration-300 ease-out ${
                        modalAction === 'withdraw'
                          ? 'text-slate-900 dark:text-white scale-105'
                          : 'text-slate-600 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'
                      }`}
                    >
                      <div className="flex items-center space-x-2">
                        <ArrowUpCircle className="w-4 h-4" />
                        <span>{lendingMode === 'lend' ? 'Withdraw' : 'Repay'}</span>
                      </div>
                    </button>
                    
                    <button
                      onClick={() => setModalAction('repay')}
                      className={`relative px-6 py-3 rounded-xl font-semibold transition-all duration-300 ease-out ${
                        modalAction === 'repay'
                          ? 'text-slate-900 dark:text-white scale-105'
                          : 'text-slate-600 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'
                      }`}
                    >
                      <div className="flex items-center space-x-2">
                        <BarChart3 className="w-4 h-4" />
                        <span>Analytics</span>
                      </div>
                    </button>
                  </div>
                </div>
              </div>

              {/* Amount Input */}
              {modalAction === 'withdraw' && (
                <div className="mb-6">
                  <div className="relative">
                    <input
                      type="number"
                      value={modalAmount}
                      onChange={(e) => setModalAmount(e.target.value)}
                      placeholder={`Enter ${modalAsset} amount`}
                      className="w-full px-6 py-4 bg-gradient-to-r from-slate-50/90 via-white/95 to-slate-50/90 dark:from-slate-700/30 dark:via-slate-600/20 dark:to-slate-700/30 border border-slate-300/50 dark:border-slate-600/30 rounded-2xl backdrop-blur-xl text-slate-900 dark:text-white placeholder-slate-500 dark:placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-400/50 transition-all duration-300 text-lg font-semibold"
                    />
                    <div className="absolute right-4 top-1/2 transform -translate-y-1/2 flex items-center space-x-2">
                      <span className="text-sm font-medium text-slate-600 dark:text-slate-400">{modalAsset}</span>
                      <button
                        onClick={() => setModalAmount(assetData[modalAsset].balance)}
                        className="px-3 py-1 bg-gradient-to-r from-blue-500/20 to-cyan-500/20 text-blue-600 dark:text-blue-400 text-xs font-semibold rounded-lg hover:from-blue-500/30 hover:to-cyan-500/30 transition-all duration-200"
                      >
                        MAX
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {/* Analytics View */}
              {modalAction === 'repay' && (
                <div className="mb-6 space-y-4">
                  <div className="p-4 bg-gradient-to-br from-emerald-500/10 via-green-500/5 to-teal-500/10 border border-emerald-400/10 rounded-2xl backdrop-blur-xl">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-slate-600 dark:text-slate-400">Total Supplied</span>
                      <span className="font-semibold text-slate-900 dark:text-white">
                        {assetData[modalAsset].balance} {modalAsset}
                      </span>
                    </div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-slate-600 dark:text-slate-400">Earnings (30d)</span>
                      <span className="font-semibold text-emerald-600 dark:text-emerald-400">
                        +{(parseFloat(assetData[modalAsset].balance) * 0.025).toFixed(2)} {modalAsset}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-slate-600 dark:text-slate-400">Next Interest</span>
                      <span className="font-semibold text-blue-600 dark:text-blue-400">
                        +{(parseFloat(assetData[modalAsset].balance) * 0.001).toFixed(4)} {modalAsset}
                      </span>
                    </div>
                  </div>
                  
                  <div className="p-4 bg-gradient-to-br from-blue-500/10 via-cyan-500/5 to-purple-500/10 border border-blue-400/10 rounded-2xl backdrop-blur-xl">
                    <h4 className="font-semibold text-slate-900 dark:text-white mb-3">Market Stats</h4>
                    <div className="space-y-2">
                      <div className="flex justify-between">
                        <span className="text-xs text-slate-600 dark:text-slate-400">Utilization Rate</span>
                        <span className="text-xs text-slate-900 dark:text-white">75.3%</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-xs text-slate-600 dark:text-slate-400">Total Borrowed</span>
                        <span className="text-xs text-slate-900 dark:text-white">$2.1M</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-xs text-slate-600 dark:text-slate-400">Total Reserves</span>
                        <span className="text-xs text-slate-900 dark:text-white">$154K</span>
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Action Button */}
              {modalAction === 'withdraw' && (
                                 <button
                   onClick={handleModalAction}
                   disabled={!modalAmount || isModalProcessing}
                   className="w-full group relative overflow-hidden px-6 py-4 rounded-2xl border border-emerald-400/20 hover:border-emerald-300/40 active:border-emerald-300/60 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-xl hover:shadow-emerald-500/20 hover:shadow-2xl backdrop-blur-2xl transform hover:scale-[1.02] active:scale-[0.98] text-white font-semibold text-lg bg-gradient-to-r from-emerald-500 to-cyan-500"
                 >
                  <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-300"></div>
                  <div className="relative flex items-center justify-center space-x-3">
                    {isModalProcessing ? (
                      <>
                        <div className="w-5 h-5 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
                        <span>Processing...</span>
                      </>
                    ) : (
                      <>
                        <ArrowUpCircle className="w-5 h-5" />
                        <span>{lendingMode === 'lend' ? 'Withdraw' : 'Repay'} {modalAsset}</span>
                      </>
                    )}
                  </div>
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Disconnect Button */}
      <button
        onClick={handleDisconnect}
        className="w-full group relative overflow-hidden px-4 py-3 rounded-xl border border-slate-200/20 dark:border-slate-700/30 hover:border-slate-300/30 dark:hover:border-slate-600/40 active:border-slate-400/40 dark:active:border-slate-500/50 focus:outline-none focus:ring-4 focus:ring-slate-400/20 dark:focus:ring-slate-500/30 transition-all duration-300 shadow-lg hover:shadow-slate-500/10 dark:hover:shadow-slate-400/15 hover:shadow-xl backdrop-blur-2xl transform hover:scale-[1.01] active:scale-[0.99] min-h-[48px] touch-manipulation mt-6 animate-fade-in animate-delay-500 bg-gradient-to-r from-slate-100/10 via-white/5 to-slate-100/10 dark:from-slate-800/20 dark:via-slate-700/10 dark:to-slate-800/20"
      >
        <div className="absolute inset-0 bg-gradient-to-r from-white/5 via-slate-50/8 to-transparent dark:from-slate-700/10 dark:via-slate-600/15 dark:to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-60 transition-opacity duration-300"></div>
        <div className="relative flex items-center justify-center space-x-2">
          <LogOut className="w-4 h-4 text-slate-600 dark:text-slate-400 group-hover:text-slate-800 dark:group-hover:text-slate-200 group-active:text-slate-700 dark:group-active:text-slate-300 transition-all duration-300 group-hover:scale-105 group-active:scale-95" />
          <span className="text-sm font-medium text-slate-600 dark:text-slate-400 group-hover:text-slate-800 dark:group-hover:text-slate-200 group-active:text-slate-700 dark:group-active:text-slate-300 font-mono uppercase tracking-wide transition-colors duration-300">
            Disconnect
          </span>
        </div>
      </button>
    </>
  );

  // Lending Interface Component
  function renderLendingInterface() {
    return (
      <div className="space-y-6">
        {/* Peridot-Style Liquid Glass Portfolio Cards */}
        <div className="grid grid-cols-3 gap-2 sm:gap-3 mb-4 animate-fade-in-scale [&_*]:!transition-[transform,opacity]">
          {/* Total Balance Card */}
          <div className="group relative overflow-hidden rounded-xl bg-gradient-to-r from-blue-500/2 via-cyan-500/1 to-indigo-500/2 dark:from-blue-400/3 dark:via-cyan-400/2 dark:to-indigo-400/3 border border-blue-400/10 dark:border-blue-400/15 hover:border-blue-300/30 shadow-lg shadow-blue-500/10 hover:shadow-blue-500/25 hover:shadow-xl backdrop-blur-2xl transition-all duration-300 transform hover:scale-[1.02] animate-slide-in-left animate-delay-100"
               style={{ boxShadow: '0 4px 20px -4px rgba(59, 130, 246, 0.15), 0 8px 32px -8px rgba(0, 0, 0, 0.1)' }}>
            
            <div className="relative p-3 sm:p-4">
              {/* Icon with refined styling */}
              <div className="flex items-center justify-center mb-2 sm:mb-3">
                <div className="w-8 h-8 sm:w-10 sm:h-10 rounded-lg bg-gradient-to-br from-blue-500/40 to-indigo-500/40 backdrop-blur-md flex items-center justify-center shadow-lg group-hover:shadow-blue-500/25 transition-all duration-300">
                  <DollarSign className="w-4 h-4 sm:w-5 sm:h-5 text-white group-hover:scale-110 transition-transform duration-300" />
                </div>
              </div>
              
              {/* Content with improved typography */}
              <div className="text-center space-y-1">
                <p className="text-xs font-semibold text-blue-700 dark:text-blue-300 font-mono uppercase tracking-wide">
                  BALANCE
                </p>
                <p className="text-lg sm:text-xl font-bold text-blue-800 dark:text-blue-200 font-mono group-hover:text-blue-600 dark:group-hover:text-blue-100 transition-colors duration-300">
                  $0.00
                </p>
                <div className="flex items-center justify-center space-x-1">
                  <div className="w-1 h-1 sm:w-1.5 sm:h-1.5 rounded-full bg-emerald-400 animate-pulse"></div>
                  <p className="text-xs sm:text-sm font-semibold text-emerald-600 dark:text-emerald-400 font-mono">+0.00%</p>
                </div>
              </div>
            </div>
          </div>

          {/* Total Lent Card */}
          <div className="group relative overflow-hidden rounded-xl bg-gradient-to-r from-emerald-500/2 via-green-500/1 to-teal-500/2 dark:from-emerald-400/3 dark:via-green-400/2 dark:to-teal-400/3 border border-emerald-400/10 dark:border-emerald-400/15 hover:border-emerald-300/30 shadow-lg shadow-emerald-500/10 hover:shadow-emerald-500/25 hover:shadow-xl backdrop-blur-2xl transition-all duration-300 transform hover:scale-[1.02] animate-fade-in-up animate-delay-200"
               style={{ boxShadow: '0 4px 20px -4px rgba(16, 185, 129, 0.15), 0 8px 32px -8px rgba(0, 0, 0, 0.1)' }}>
            
            <div className="relative p-3 sm:p-4">
              <div className="flex items-center justify-center mb-2 sm:mb-3">
                <div className="w-8 h-8 sm:w-10 sm:h-10 rounded-lg bg-gradient-to-br from-emerald-500/40 to-teal-500/40 backdrop-blur-md flex items-center justify-center shadow-lg group-hover:shadow-emerald-500/25 transition-all duration-300">
                  <ArrowUpCircle className="w-4 h-4 sm:w-5 sm:h-5 text-white group-hover:scale-110 transition-transform duration-300" />
                </div>
              </div>
              
              <div className="text-center space-y-1">
                <p className="text-xs font-semibold text-emerald-700 dark:text-emerald-300 font-mono uppercase tracking-wide">
                  LENT
                </p>
                <p className="text-lg sm:text-xl font-bold text-emerald-800 dark:text-emerald-200 font-mono group-hover:text-emerald-600 dark:group-hover:text-emerald-100 transition-colors duration-300">
                  $0.00
                </p>
                <div className="flex items-center justify-center space-x-1">
                  <div className="w-1 h-1 sm:w-1.5 sm:h-1.5 rounded-full bg-emerald-400 animate-pulse"></div>
                  <p className="text-xs sm:text-sm font-semibold text-emerald-600 dark:text-emerald-400 font-mono">APY</p>
                </div>
              </div>
            </div>
          </div>

          {/* Total Borrowed Card */}
          <div className="group relative overflow-hidden rounded-xl bg-gradient-to-r from-orange-500/2 via-red-500/1 to-pink-500/2 dark:from-orange-400/3 dark:via-red-400/2 dark:to-pink-400/3 border border-orange-400/10 dark:border-orange-400/15 hover:border-orange-300/30 shadow-lg shadow-orange-500/10 hover:shadow-orange-500/25 hover:shadow-xl backdrop-blur-2xl transition-all duration-300 transform hover:scale-[1.02] animate-slide-in-right animate-delay-300"
               style={{ boxShadow: '0 4px 20px -4px rgba(249, 115, 22, 0.15), 0 8px 32px -8px rgba(0, 0, 0, 0.1)' }}>
            
            <div className="relative p-3 sm:p-4">
              <div className="flex items-center justify-center mb-2 sm:mb-3">
                <div className="w-8 h-8 sm:w-10 sm:h-10 rounded-lg bg-gradient-to-br from-orange-500/40 to-red-500/40 backdrop-blur-md flex items-center justify-center shadow-lg group-hover:shadow-orange-500/25 transition-all duration-300">
                  <ArrowDownCircle className="w-4 h-4 sm:w-5 sm:h-5 text-white group-hover:scale-110 transition-transform duration-300" />
                </div>
              </div>
              
              <div className="text-center space-y-1">
                <p className="text-xs font-semibold text-orange-700 dark:text-orange-300 font-mono uppercase tracking-wide">
                  BORROWED
                </p>
                <p className="text-lg sm:text-xl font-bold text-orange-800 dark:text-orange-200 font-mono group-hover:text-orange-600 dark:group-hover:text-orange-100 transition-colors duration-300">
                  $0.00
                </p>
                <div className="flex items-center justify-center space-x-1">
                  <div className="w-1 h-1 sm:w-1.5 sm:h-1.5 rounded-full bg-orange-400 animate-pulse"></div>
                  <p className="text-xs sm:text-sm font-semibold text-orange-600 dark:text-orange-400 font-mono">Interest</p>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Lending/Borrowing Interface */}
        <div className="relative overflow-hidden rounded-2xl bg-gradient-to-r from-purple-500/2 via-blue-500/1 to-cyan-500/2 dark:from-purple-400/3 dark:via-blue-400/2 dark:to-cyan-400/3 border border-purple-400/10 dark:border-purple-400/15 shadow-lg shadow-purple-500/10 backdrop-blur-2xl animate-fade-in-scale animate-delay-400"
             style={{ boxShadow: '0 8px 32px -8px rgba(147, 51, 234, 0.12), 0 16px 64px -16px rgba(0, 0, 0, 0.08)' }}>
          
          <div className="relative p-4 sm:p-6 lg:p-8">
            {/* Mode Toggle */}
            <div className="flex items-center justify-center mb-4 sm:mb-6 lg:mb-8">
              <div className="relative p-1 bg-gradient-to-r from-white/10 via-white/5 to-white/10 dark:from-slate-700/50 dark:via-slate-600/30 dark:to-slate-700/50 rounded-2xl backdrop-blur-2xl border border-white/20 dark:border-slate-600/30">
                <div className="flex relative">
                  <div 
                    className={`absolute top-0 bottom-0 w-1/2 bg-gradient-to-r from-white/30 via-white/20 to-white/30 dark:from-white/10 dark:via-white/5 dark:to-white/10 rounded-xl shadow-lg transition-all duration-500 ease-out backdrop-blur-xl ${
                      lendingMode === 'lend' ? 'left-0' : 'left-1/2'
                    }`}
                  />
                  
                  <button
                    onClick={() => setLendingMode('lend')}
                    className={`relative px-4 sm:px-6 lg:px-8 py-3 sm:py-4 rounded-xl font-semibold transition-all duration-300 ease-out ${
                      lendingMode === 'lend'
                        ? 'scale-105'
                        : 'hover:text-slate-800 dark:hover:text-slate-200'
                    }`}
                    style={{
                      color: lendingMode === 'lend' ? '#1e293b' : '#64748b'
                    }}
                  >
                    <div className="flex items-center space-x-1.5 sm:space-x-2">
                      <ArrowUpCircle className="w-4 h-4 sm:w-5 sm:h-5" />
                      <span className="text-sm sm:text-base">Lend</span>
                    </div>
                  </button>
                  
                                    <button
                    onClick={() => setLendingMode('borrow')}
                    className={`relative px-4 sm:px-6 lg:px-8 py-3 sm:py-4 rounded-xl font-semibold transition-all duration-300 ease-out ${
                      lendingMode === 'borrow'
                        ? 'scale-105'
                        : 'hover:text-slate-800 dark:hover:text-slate-200'
                    }`}
                    style={{
                      color: lendingMode === 'borrow' ? '#1e293b' : '#64748b'
                    }}
                  >
                    <div className="flex items-center space-x-1.5 sm:space-x-2">
                      <ArrowDownCircle className="w-4 h-4 sm:w-5 sm:h-5" />
                      <span className="text-sm sm:text-base">Borrow</span>
                    </div>
                  </button>
                </div>
              </div>
            </div>

            {/* Asset Selection */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-2 sm:gap-3 mb-4 sm:mb-6">
              {Object.entries(assetData).map(([asset, data]) => (
                <button
                  key={asset}
                  onClick={() => openModal(asset as typeof modalAsset)}
                  className={`group relative overflow-hidden p-3 sm:p-4 rounded-xl sm:rounded-2xl border transition-all duration-300 ease-out backdrop-blur-xl transform hover:scale-105 active:scale-95 cursor-pointer ${
                    selectedAsset === asset
                      ? 'bg-gradient-to-br from-emerald-500/20 via-teal-500/10 to-cyan-500/20 border-emerald-400/30'
                      : 'bg-gradient-to-br from-white/5 via-white/2 to-white/5 dark:from-slate-700/20 dark:via-slate-600/10 dark:to-slate-700/20 border-white/10 dark:border-slate-600/20 hover:border-emerald-400/20'
                  }`}
                  style={{
                    boxShadow: selectedAsset === asset 
                      ? '0 8px 32px -8px rgba(16, 185, 129, 0.25), 0 16px 48px -16px rgba(16, 185, 129, 0.15), 0 4px 16px -4px rgba(0, 0, 0, 0.1)'
                      : '0 4px 20px -4px rgba(0, 0, 0, 0.08), 0 8px 32px -8px rgba(0, 0, 0, 0.04), 0 12px 48px -12px rgba(0, 0, 0, 0.02)'
                  }}
                  onMouseEnter={(e) => {
                    if (selectedAsset !== asset) {
                      e.currentTarget.style.boxShadow = '0 8px 32px -8px rgba(16, 185, 129, 0.12), 0 16px 48px -16px rgba(16, 185, 129, 0.08), 0 4px 16px -4px rgba(0, 0, 0, 0.15)';
                    }
                  }}
                  onMouseLeave={(e) => {
                    if (selectedAsset !== asset) {
                      e.currentTarget.style.boxShadow = '0 4px 20px -4px rgba(0, 0, 0, 0.08), 0 8px 32px -8px rgba(0, 0, 0, 0.04), 0 12px 48px -12px rgba(0, 0, 0, 0.02)';
                    }
                  }}
                >
                  <div className="absolute inset-0 bg-gradient-to-br from-white/5 via-transparent to-white/2 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
                  
                  {/* Click indicator - always visible */}
                  <div className="absolute top-2 right-2 opacity-40 group-hover:opacity-80 transition-opacity duration-300">
                    <div className="w-4 h-4 rounded-full bg-emerald-500/15 group-hover:bg-emerald-500/25 flex items-center justify-center transition-colors duration-300">
                      <ExternalLink className="w-2.5 h-2.5 text-emerald-400 group-hover:text-emerald-300 transition-colors duration-300" />
                    </div>
                  </div>
                  
                  <div className="relative text-center">
                    <h4 className="font-bold text-slate-900 dark:text-white mb-1 group-hover:text-emerald-600 dark:group-hover:text-emerald-400 transition-colors duration-300">{asset}</h4>
                    <p className="text-xs text-slate-600 dark:text-slate-400 mb-2">
                      {lendingMode === 'borrow' ? 'Borrowed:' : 'Balance:'} {lendingMode === 'borrow' ? data.borrowed : data.balance}
                    </p>
                    <div className="flex items-center justify-center space-x-1">
                      <Percent className="w-3 h-3 text-emerald-500 group-hover:scale-110 transition-transform duration-300" />
                      <span className="text-xs font-semibold text-emerald-500">{data.apy}% APY</span>
                    </div>
                  </div>
                </button>
              ))}
            </div>

            {/* Amount Input */}
            <div className="mb-4 sm:mb-6">
              <div className="inputbox">
                <input
                  type="number"
                  value={amount}
                  onChange={(e) => setAmount(e.target.value)}
                  required
                />
                <span>Enter {selectedAsset} amount</span>
                <i></i>
                <div className="max-button">
                  <button
                    onClick={() => setAmount(assetData[selectedAsset].balance)}
                    className="px-2 py-1 bg-gradient-to-r from-emerald-500/20 to-teal-500/20 text-emerald-600 dark:text-emerald-400 text-xs font-bold rounded border border-emerald-400/30 hover:from-emerald-500/30 hover:to-teal-500/30 hover:border-emerald-400/50 transition-all duration-300"
                  >
                    MAX
                  </button>
                </div>
              </div>
            </div>

            {/* Action Button */}
            <button
              onClick={handleLendingAction}
              disabled={!amount || isProcessing}
              className="w-full group relative overflow-hidden px-4 sm:px-6 py-3 sm:py-4 rounded-xl sm:rounded-2xl border border-emerald-400/20 hover:border-emerald-300/40 active:border-emerald-300/60 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-xl hover:shadow-emerald-500/20 hover:shadow-2xl backdrop-blur-2xl transform hover:scale-[1.02] active:scale-[0.98] text-white font-semibold text-base sm:text-lg bg-gradient-to-r from-emerald-500 to-cyan-500"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-300"></div>
              <div className="relative flex items-center justify-center space-x-3">
                {isProcessing ? (
                  <>
                    <div className="w-5 h-5 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
                    <span>Processing...</span>
                  </>
                ) : (
                  <>
                    {lendingMode === 'lend' ? (
                      <ArrowUpCircle className="w-5 h-5" />
                    ) : (
                      <ArrowDownCircle className="w-5 h-5" />
                    )}
                    <span>{lendingMode === 'lend' ? 'Lend' : 'Borrow'} {selectedAsset}</span>
                  </>
                )}
              </div>
            </button>


          </div>
        </div>
      </div>
    );
  }

  // Faucet Interface Component
  function renderFaucetInterface() {
    return (
      <div className="space-y-6 animate-fade-in">
        {/* Cyber Status Messages */}
        {mintingStatus === 'success' && (
          <div className="mb-4 relative overflow-hidden rounded-xl bg-gradient-to-r from-emerald-500/2 via-green-500/1 to-teal-500/2 dark:from-emerald-400/3 dark:via-green-400/2 dark:to-teal-400/3 border border-emerald-400/15 dark:border-emerald-400/20 shadow-lg shadow-emerald-500/5 backdrop-blur-xl">
            <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-emerald-400/40 to-teal-400/40"></div>
            <div className="relative p-3 flex items-center space-x-3">
              <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-emerald-500/40 to-teal-500/40 backdrop-blur-md flex items-center justify-center shadow-lg">
                <CheckCircle className="w-4 h-4 text-white" />
              </div>
              <div className="flex-1">
                <p className="text-sm text-emerald-700 dark:text-emerald-300 font-mono">
                  SUCCESS: TOKENS_MINTED
                </p>
                <p className="text-xs text-emerald-600 dark:text-emerald-400 font-mono opacity-80">
                  1,000 PDOT tokens added to wallet
                </p>
              </div>
            </div>
          </div>
        )}

        {mintingStatus === 'error' && mintError && (
          <div className="mb-4 relative overflow-hidden rounded-xl bg-gradient-to-r from-red-500/2 via-red-600/1 to-orange-500/2 dark:from-red-400/3 dark:via-red-500/2 dark:to-orange-400/3 border border-red-400/15 dark:border-red-400/20 shadow-lg shadow-red-500/5 backdrop-blur-xl">
            <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-red-400/40 to-orange-400/40"></div>
            <div className="relative p-3 flex items-center space-x-3">
              <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-red-500/40 to-orange-500/40 backdrop-blur-md flex items-center justify-center shadow-lg">
                <AlertCircle className="w-4 h-4 text-white" />
              </div>
              <div className="flex-1">
                <p className="text-sm text-red-700 dark:text-red-300 font-mono">ERROR_CODE: 0x{Math.random().toString(16).substr(2, 6).toUpperCase()}</p>
                <p className="text-xs text-red-600 dark:text-red-400 font-mono opacity-80">{mintError}</p>
              </div>
            </div>
          </div>
        )}

        {/* Cyber Token Balances */}
        <div className="space-y-3 mb-4 animate-fade-in-up animate-delay-200">
          {/* Smart PDOT Tokens Section */}
          {parseFloat(walletInfo?.testTokenBalance || '0') === 0 ? (
            // Large mint button when no tokens - Enhanced responsiveness
            <div className="relative overflow-hidden rounded-xl bg-gradient-to-r from-emerald-500/2 via-teal-500/1 to-cyan-500/2 dark:from-emerald-400/4 dark:via-teal-400/2 dark:to-cyan-400/4 border border-emerald-400/10 dark:border-emerald-400/15 shadow-xl shadow-emerald-500/3 backdrop-blur-2xl">
              <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-emerald-400/30 via-teal-400/30 to-cyan-400/30"></div>
              <button
                onClick={handleMint}
                disabled={isMinting}
                className="w-full group relative overflow-hidden p-4 rounded-xl border-2 border-emerald-400/20 hover:border-emerald-300/40 active:border-emerald-300/60 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 shadow-lg hover:shadow-emerald-500/20 hover:shadow-xl active:shadow-emerald-500/30 backdrop-blur-xl transform hover:scale-[1.02] active:scale-[0.98] min-h-[64px] touch-manipulation bg-gradient-to-r from-emerald-500 to-cyan-500"
              >
                <div className="absolute inset-0 bg-gradient-to-r from-white/3 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-200"></div>
                <div className="relative flex items-center justify-center space-x-3">
                  {isMinting ? (
                    <>
                      <svg className="w-8 h-8" viewBox="0 0 240 240">
                        <circle className="pl__ring pl__ring--a" cx="120" cy="120" r="105" fill="none" strokeWidth="20" strokeDasharray="0 660" strokeDashoffset="-330" strokeLinecap="round"></circle>
                        <circle className="pl__ring pl__ring--b" cx="120" cy="120" r="35" fill="none" strokeWidth="20" strokeDasharray="0 220" strokeDashoffset="-110" strokeLinecap="round"></circle>
                        <circle className="pl__ring pl__ring--c" cx="85" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
                        <circle className="pl__ring pl__ring--d" cx="155" cy="120" r="70" fill="none" strokeWidth="20" strokeDasharray="0 440" strokeLinecap="round"></circle>
                      </svg>
                      <span className="text-lg font-bold text-white font-mono uppercase tracking-wide">MINTING_TOKENS...</span>
                    </>
                  ) : (
                    <>
                      <Zap className="w-6 h-6 text-white drop-shadow-lg group-hover:scale-110 group-active:scale-95 transition-transform duration-200" />
                      <div className="text-center">
                        <div className="text-lg font-bold text-white font-mono uppercase tracking-wide group-hover:text-emerald-100 transition-colors duration-200">MINT_1000_PDOT</div>
                        <div className="text-sm text-emerald-100 font-mono opacity-90 group-hover:opacity-100 transition-opacity duration-200">Initialize vault operations</div>
                      </div>
                    </>
                  )}
                </div>
              </button>
            </div>
          ) : (
            // Regular balance display with small mint button - Enhanced responsiveness
            <div className="relative overflow-hidden rounded-xl bg-gradient-to-r from-emerald-500/2 via-green-500/1 to-teal-500/2 dark:from-emerald-400/3 dark:via-green-400/2 dark:to-teal-400/3 border border-emerald-400/10 dark:border-emerald-400/15 shadow-lg shadow-emerald-500/3 backdrop-blur-2xl">
              <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-emerald-400/30 to-teal-400/30"></div>
              <div className="relative flex items-center justify-between p-3">
                <div className="flex items-center space-x-3">
                  <span className="text-sm font-semibold text-emerald-700 dark:text-emerald-300 font-mono uppercase tracking-wide">PDOT_BALANCE:</span>
                  <span className={`font-bold text-emerald-800 dark:text-emerald-200 font-mono ${
                    parseFloat(walletInfo?.testTokenBalance || '0') >= 100000 ? 'text-xs sm:text-sm' :
                    parseFloat(walletInfo?.testTokenBalance || '0') >= 10000 ? 'text-sm sm:text-base' :
                    'text-base sm:text-lg'
                  }`}>
                    {formatNumber(walletInfo?.testTokenBalance || '0')}
                  </span>
                </div>
                                  <button
                  onClick={handleMint}
                  disabled={isMinting}
                  className="group relative flex items-center space-x-1 px-4 py-2 text-white text-xs font-bold rounded-lg border border-emerald-400/15 hover:border-emerald-300/30 active:border-emerald-300/50 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 transition-all duration-200 shadow-lg hover:shadow-emerald-500/20 hover:shadow-xl active:shadow-emerald-500/30 backdrop-blur-lg transform hover:scale-105 active:scale-95 min-h-[36px] touch-manipulation bg-gradient-to-r from-emerald-500 to-cyan-500"
                  title="Mint more PDOT tokens"
                >
                  <div className="absolute inset-0 bg-gradient-to-r from-white/3 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-200 rounded-lg"></div>
                  {isMinting ? (
                    <svg className="relative w-4 h-4" viewBox="0 0 60 60">
                      <circle className="pl__ring pl__ring--a" cx="30" cy="30" r="26.25" fill="none" strokeWidth="5" strokeDasharray="0 165" strokeDashoffset="-82.5" strokeLinecap="round"></circle>
                      <circle className="pl__ring pl__ring--b" cx="30" cy="30" r="8.75" fill="none" strokeWidth="5" strokeDasharray="0 55" strokeDashoffset="-27.5" strokeLinecap="round"></circle>
                      <circle className="pl__ring pl__ring--c" cx="21.25" cy="30" r="17.5" fill="none" strokeWidth="5" strokeDasharray="0 110" strokeLinecap="round"></circle>
                      <circle className="pl__ring pl__ring--d" cx="38.75" cy="30" r="17.5" fill="none" strokeWidth="5" strokeDasharray="0 110" strokeLinecap="round"></circle>
                    </svg>
                  ) : (
                    <Coins className="relative w-3 h-3 group-hover:scale-110 group-active:scale-90 transition-transform duration-200" />
                  )}
                  <span className="relative font-mono group-hover:text-emerald-100 transition-colors duration-200">+1000</span>
                </button>
              </div>
            </div>
          )}
          
          {/* pTokens Balance */}
          <div className="relative overflow-hidden rounded-xl bg-gradient-to-r from-blue-500/2 via-cyan-500/1 to-purple-500/2 dark:from-blue-400/3 dark:via-cyan-400/2 dark:to-purple-400/3 border border-blue-400/10 dark:border-blue-400/15 shadow-lg shadow-blue-500/3 backdrop-blur-2xl">
            <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-blue-400/30 via-cyan-400/30 to-purple-400/30"></div>
            <div className="relative flex items-center justify-between p-3">
              <span className="text-sm font-semibold text-blue-700 dark:text-blue-300 font-mono uppercase tracking-wide">PTOKENS:</span>
              <span className="font-bold text-blue-800 dark:text-blue-200 font-mono">
                {formatNumber(walletInfo?.pTokenBalance || '0')}
              </span>
            </div>
          </div>
        </div>

        {/* Cyber Info Panel */}
        {parseFloat(walletInfo?.testTokenBalance || '0') > 0 && (
          <div className="mb-4 relative overflow-hidden rounded-xl bg-gradient-to-r from-blue-500/2 via-cyan-500/1 to-indigo-500/2 dark:from-blue-400/3 dark:via-cyan-400/2 dark:to-indigo-400/3 border border-blue-400/10 dark:border-blue-400/15 shadow-lg shadow-blue-500/3 backdrop-blur-2xl">
            <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-blue-400/30 via-cyan-400/30 to-indigo-400/30"></div>
            <div className="relative p-3">
              <p className="text-xs text-blue-700 dark:text-blue-300 font-mono">
                <span className="text-cyan-600 dark:text-cyan-400 font-bold">SYSTEM_INFO:</span> MINT_MORE_TOKENS_AVAILABLE<br/>
                {'>'} Execute <span className="bg-emerald-600/10 px-1 py-0.5 rounded text-emerald-600 dark:text-emerald-400 font-bold backdrop-blur-lg">+1000</span> command for additional PDOT tokens
              </p>
            </div>
          </div>
        )}
      </div>
    );
  }
} 