'use client';

import { useState } from 'react';
import { Wallet, Coins, Shield, TrendingUp, Settings, Database } from 'lucide-react';
import AnimatedCardCarousel from './AnimatedCardCarousel';
import { WalletInfo } from '@/utils/stellar';

interface ExampleCarouselUsageProps {
  walletInfo?: WalletInfo | null;
}

export default function ExampleCarouselUsage({ walletInfo }: ExampleCarouselUsageProps) {
  const [activeComponent, setActiveComponent] = useState('wallet');

  // Define your cards with different components
  const componentCards = [
    {
      id: 'wallet',
      color: '142, 249, 252', // Cyan
      content: (
        <div className="text-white">
          <Wallet className="w-8 h-8 mx-auto mb-2" />
          <h3 className="font-bold text-sm mb-1">WALLET</h3>
          <p className="text-xs opacity-80">
            {walletInfo?.isConnected ? 'Connected' : 'Connect Freighter'}
          </p>
          {walletInfo?.isConnected && (
            <div className="mt-2 text-xs">
              <div>PDOT: {walletInfo.testTokenBalance || '0'}</div>
              <div>pTokens: {walletInfo.pTokenBalance || '0'}</div>
            </div>
          )}
        </div>
      )
    },
    {
      id: 'portfolio',
      color: '215, 252, 142', // Lime
      content: (
        <div className="text-white">
          <TrendingUp className="w-8 h-8 mx-auto mb-2" />
          <h3 className="font-bold text-sm mb-1">PORTFOLIO</h3>
          <p className="text-xs opacity-80">Track your positions</p>
          <div className="mt-2 text-xs">
            <div>Total Value: $0.00</div>
            <div>24h Change: +0%</div>
          </div>
        </div>
      )
    },
    {
      id: 'vault',
      color: '252, 208, 142', // Orange
      content: (
        <div className="text-white">
          <Shield className="w-8 h-8 mx-auto mb-2" />
          <h3 className="font-bold text-sm mb-1">VAULT</h3>
          <p className="text-xs opacity-80">Secure storage</p>
          <div className="mt-2 text-xs">
            <div>Locked: 0 PDOT</div>
            <div>Rewards: 0</div>
          </div>
        </div>
      )
    },
    {
      id: 'swap',
      color: '204, 142, 252', // Purple
      content: (
        <div className="text-white">
          <Coins className="w-8 h-8 mx-auto mb-2" />
          <h3 className="font-bold text-sm mb-1">SWAP</h3>
          <p className="text-xs opacity-80">Exchange tokens</p>
          <div className="mt-2 text-xs">
            <div>Best Rate</div>
            <div>Low Fees</div>
          </div>
        </div>
      )
    },
    {
      id: 'analytics',
      color: '252, 142, 239', // Pink
      content: (
        <div className="text-white">
          <Database className="w-8 h-8 mx-auto mb-2" />
          <h3 className="font-bold text-sm mb-1">ANALYTICS</h3>
          <p className="text-xs opacity-80">Market insights</p>
          <div className="mt-2 text-xs">
            <div>TVL: $0</div>
            <div>Volume: $0</div>
          </div>
        </div>
      )
    },
    {
      id: 'settings',
      color: '142, 202, 252', // Light Blue
      content: (
        <div className="text-white">
          <Settings className="w-8 h-8 mx-auto mb-2" />
          <h3 className="font-bold text-sm mb-1">SETTINGS</h3>
          <p className="text-xs opacity-80">Preferences</p>
          <div className="mt-2 text-xs">
            <div>Theme: Dark</div>
            <div>Network: Testnet</div>
          </div>
        </div>
      )
    }
  ];

  const handleComponentChange = (componentId: string) => {
    setActiveComponent(componentId);
    console.log('Active component:', componentId);
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900">
      {/* Header */}
      <div className="container mx-auto px-4 py-8">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-white mb-2">
            Peridot Dashboard Components
          </h1>
          <p className="text-slate-400 font-mono">
            Scroll or use arrow keys to navigate • Current: {activeComponent.toUpperCase()}
          </p>
        </div>

        {/* Animated Card Carousel */}
        <div className="mb-12">
          <AnimatedCardCarousel
            cards={componentCards}
            activeCardId={activeComponent}
            onCardChange={handleComponentChange}
            autoRotate={false}
            enableScrollSpy={true}
            scrollOffset={150}
          />
        </div>

        {/* Active Component Details */}
        <div className="max-w-2xl mx-auto">
          <div className="bg-slate-800/50 backdrop-blur-xl rounded-2xl border border-slate-700/50 p-6">
            <h2 className="text-xl font-bold text-white mb-4 font-mono">
              ACTIVE_COMPONENT: {activeComponent.toUpperCase()}
            </h2>
            
            {/* Component-specific content */}
            {activeComponent === 'wallet' && (
              <div className="space-y-4">
                <div className="text-slate-300">
                  <h3 className="font-semibold mb-2">Wallet Management</h3>
                  <p className="text-sm">
                    Connect your Freighter wallet to access Peridot's features. 
                    Once connected, you can mint test tokens, view balances, and interact with the protocol.
                  </p>
                </div>
                {walletInfo?.isConnected && (
                  <div className="bg-emerald-900/20 border border-emerald-700/30 rounded-lg p-3">
                    <p className="text-emerald-300 text-sm font-mono">
                      Status: CONNECTED • Address: {walletInfo.address?.slice(0, 8)}...
                    </p>
                  </div>
                )}
              </div>
            )}

            {activeComponent === 'portfolio' && (
              <div className="space-y-4">
                <div className="text-slate-300">
                  <h3 className="font-semibold mb-2">Portfolio Overview</h3>
                  <p className="text-sm">
                    Track your token holdings, staking rewards, and overall performance across all Peridot positions.
                  </p>
                </div>
              </div>
            )}

            {activeComponent === 'vault' && (
              <div className="space-y-4">
                <div className="text-slate-300">
                  <h3 className="font-semibold mb-2">Secure Vault</h3>
                  <p className="text-sm">
                    Lock your tokens in secure vaults to earn rewards while maintaining security through our protocol.
                  </p>
                </div>
              </div>
            )}

            {/* Add more component details as needed */}

            {/* Navigation Instructions */}
            <div className="mt-6 pt-4 border-t border-slate-700/50">
              <div className="grid grid-cols-2 gap-4 text-xs text-slate-400 font-mono">
                <div>
                  <span className="text-slate-300">Scroll Navigation:</span><br/>
                  Scroll page to change active component
                </div>
                <div>
                  <span className="text-slate-300">Keyboard Navigation:</span><br/>
                  Use ← → arrow keys to navigate
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
} 