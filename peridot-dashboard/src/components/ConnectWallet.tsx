'use client';

import { useState } from 'react';
import { Wallet, LogOut, Copy, CheckCircle, AlertCircle, ExternalLink, Coins, Loader, Zap } from 'lucide-react';
import { connectFreighter, getBalances, formatNumber, WalletInfo, mintTestTokens } from '@/utils/stellar';

interface ConnectWalletProps {
  walletInfo: WalletInfo | null;
  onWalletChange: (info: WalletInfo | null) => void;
}

export default function ConnectWallet({ walletInfo, onWalletChange }: ConnectWalletProps) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  
  // Mint functionality
  const [isMinting, setIsMinting] = useState(false);
  const [mintingStatus, setMintingStatus] = useState<'idle' | 'success' | 'error'>('idle');
  const [mintError, setMintError] = useState<string | null>(null);

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
      const result = await mintTestTokens(walletInfo.address);
      
      if (result.success) {
        setMintingStatus('success');
        // Refresh wallet balances
        const updatedBalances = await getBalances(walletInfo.address);
        onWalletChange(updatedBalances);
        setTimeout(() => setMintingStatus('idle'), 3000);
      } else {
        setMintingStatus('error');
        setMintError(result.error || 'Minting failed');
      }
    } catch (err) {
      setMintingStatus('error');
      setMintError(`Minting failed: ${err}`);
    } finally {
      setIsMinting(false);
    }
  };

  if (walletInfo?.isConnected) {
    return (
      <>
        {/* Cyber Wallet Connected Header */}
        <div className="flex items-center space-x-4 mb-6">
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
        <div className="space-y-3 mb-4">
          {/* Smart PDOT Tokens Section */}
          {parseFloat(walletInfo.testTokenBalance || '0') === 0 ? (
            // Large mint button when no tokens - Enhanced responsiveness
            <div className="relative overflow-hidden rounded-xl bg-gradient-to-r from-emerald-500/2 via-teal-500/1 to-cyan-500/2 dark:from-emerald-400/4 dark:via-teal-400/2 dark:to-cyan-400/4 border border-emerald-400/10 dark:border-emerald-400/15 shadow-xl shadow-emerald-500/3 backdrop-blur-2xl">
              <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-emerald-400/30 via-teal-400/30 to-cyan-400/30"></div>
              <button
                onClick={handleMint}
                disabled={isMinting}
                className="w-full group relative overflow-hidden p-4 bg-gradient-to-r from-emerald-500/30 to-teal-500/30 hover:from-emerald-600/50 hover:to-teal-600/50 active:from-emerald-700/60 active:to-teal-700/60 rounded-xl border-2 border-emerald-400/20 hover:border-emerald-300/40 active:border-emerald-300/60 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 shadow-lg hover:shadow-emerald-500/20 hover:shadow-xl active:shadow-emerald-500/30 backdrop-blur-xl transform hover:scale-[1.02] active:scale-[0.98] min-h-[64px] touch-manipulation"
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
                  <span className="text-sm font-semibold text-emerald-700 dark:text-emerald-300 font-mono uppercase tracking-wide">PDOT_BALANCE::</span>
                  <span className="font-bold text-emerald-800 dark:text-emerald-200 font-mono">
                    {formatNumber(walletInfo.testTokenBalance)}
                  </span>
                </div>
                <button
                  onClick={handleMint}
                  disabled={isMinting}
                  className="group relative flex items-center space-x-1 px-4 py-2 bg-gradient-to-r from-emerald-600/30 to-teal-600/30 hover:from-emerald-500/50 hover:to-teal-500/50 active:from-emerald-700/60 active:to-teal-700/60 text-white text-xs font-bold rounded-lg border border-emerald-400/15 hover:border-emerald-300/30 active:border-emerald-300/50 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 transition-all duration-200 shadow-lg hover:shadow-emerald-500/20 hover:shadow-xl active:shadow-emerald-500/30 backdrop-blur-lg transform hover:scale-105 active:scale-95 min-h-[36px] touch-manipulation"
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
                {formatNumber(walletInfo.pTokenBalance)}
              </span>
            </div>
          </div>

        </div>

        {/* Cyber Info Panel */}
        {parseFloat(walletInfo.testTokenBalance || '0') > 0 && (
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

        {/* Cyber Disconnect Button - Enhanced responsiveness */}
        <button
          onClick={handleDisconnect}
          className="w-full group relative overflow-hidden px-4 py-3 bg-gradient-to-r from-red-600/20 via-red-700/20 to-red-800/20 hover:from-red-500/40 hover:via-red-600/40 hover:to-red-700/40 active:from-red-600/50 active:via-red-700/50 active:to-red-800/50 rounded-xl border border-red-500/10 hover:border-red-400/25 active:border-red-400/40 focus:outline-none focus:ring-4 focus:ring-red-400/30 transition-all duration-200 shadow-lg hover:shadow-red-500/20 hover:shadow-xl active:shadow-red-500/30 backdrop-blur-2xl transform hover:scale-[1.02] active:scale-[0.98] min-h-[48px] touch-manipulation"
        >
          <div className="absolute inset-0 bg-gradient-to-r from-white/2 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-200"></div>
          <div className="relative flex items-center justify-center space-x-2">
            <LogOut className="w-4 h-4 text-white group-hover:text-red-100 group-active:text-red-200 transition-all duration-200 group-hover:scale-110 group-active:scale-95" />
            <span className="text-sm font-semibold text-white group-hover:text-red-100 group-active:text-red-200 font-mono uppercase tracking-wide transition-colors duration-200">
              DISCONNECT_WALLET
            </span>
          </div>
        </button>
      </>
    );
  }

  return (
    <>
      <div className="text-center py-8">
        {/* Cyber Connection Interface */}
        <div className="relative mx-auto mb-6">
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
          <div className="mb-6 relative overflow-hidden rounded-xl bg-gradient-to-r from-red-500/2 via-red-600/1 to-orange-500/2 dark:from-red-400/3 dark:via-red-500/2 dark:to-orange-400/3 border border-red-400/10 dark:border-red-400/15 shadow-lg shadow-red-500/3 backdrop-blur-2xl">
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

        {/* Cyber Connect Button - Enhanced responsiveness */}
        <button
          onClick={handleConnect}
          disabled={isConnecting}
          className="w-full group relative overflow-hidden px-6 py-4 bg-gradient-to-r from-emerald-600/30 via-teal-600/30 to-cyan-600/30 hover:from-emerald-500/50 hover:via-teal-500/50 hover:to-cyan-500/50 active:from-emerald-700/60 active:via-teal-700/60 active:to-cyan-700/60 rounded-xl border-2 border-emerald-400/20 hover:border-emerald-300/40 active:border-emerald-300/60 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 shadow-xl hover:shadow-emerald-500/20 hover:shadow-2xl active:shadow-emerald-500/30 backdrop-blur-2xl transform hover:scale-[1.02] active:scale-[0.98] min-h-[64px] touch-manipulation"
        >
          <div className="absolute inset-0 bg-gradient-to-r from-white/3 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-200"></div>
          
          {/* Scanning animation when connecting */}
          {isConnecting && (
            <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/8 to-transparent animate-pulse"></div>
          )}
          
          <div className="relative flex items-center justify-center space-x-3">
            {isConnecting ? (
              <>
                <div className="w-6 h-6 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
                <span className="text-lg font-bold text-white font-mono uppercase tracking-wide">
                  CONNECTING...
                </span>
              </>
            ) : (
              <>
                <Wallet className="w-6 h-6 text-white group-hover:text-emerald-100 group-active:text-emerald-200 transition-all duration-200 group-hover:scale-110 group-active:scale-95" />
                <span className="text-lg font-bold text-white group-hover:text-emerald-100 group-active:text-emerald-200 font-mono uppercase tracking-wide transition-colors duration-200">
                  CONNECT_FREIGHTER
                </span>
              </>
            )}
          </div>
        </button>

        {/* Cyber Requirements Panel */}
        <div className="mt-4 relative overflow-hidden rounded-xl bg-gradient-to-r from-blue-500/2 via-indigo-500/1 to-purple-500/2 dark:from-blue-400/3 dark:via-indigo-400/2 dark:to-purple-400/3 border border-blue-400/10 dark:border-blue-400/15 shadow-lg shadow-blue-500/3 backdrop-blur-2xl">
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