'use client';

import { useState } from 'react';
import { 
  Bot, 
  MessageCircle, 
  TrendingUp, 
  Shield, 
  Zap, 
  BarChart3, 
  Send, 
  Settings, 
  CheckCircle,
  AlertTriangle,
  Target,
  ArrowUpDown,
  DollarSign,
  Repeat
} from 'lucide-react';
import { WalletInfo } from '@/utils/stellar';

interface AIAgentProps {
  walletInfo: WalletInfo | null;
}

export default function AIAgent({ walletInfo }: AIAgentProps) {
  const [activeStrategy, setActiveStrategy] = useState<'stop-loss' | 'portfolio' | 'margin' | 'crosschain'>('stop-loss');
  const [chatMessage, setChatMessage] = useState('');
  const [chatHistory, setChatHistory] = useState([
    { role: 'ai', message: 'Hello! I\'m your AI trading assistant. How can I help you optimize your portfolio today?' },
  ]);
  const [isProcessing, setIsProcessing] = useState(false);

  const strategies = [
    {
      id: 'stop-loss' as const,
      name: 'Stop Loss / Take Profit',
      icon: Target,
      description: 'Automated risk management',
      status: 'Active',
      pnl: '+$1,247.50'
    },
    {
      id: 'portfolio' as const,
      name: 'Portfolio Rebalance',
      icon: BarChart3,
      description: 'Auto-rebalancing based on targets',
      status: 'Monitoring',
      pnl: '+$892.30'
    },
    {
      id: 'margin' as const,
      name: 'Margin Trading',
      icon: TrendingUp,
      description: 'Long/Short position management',
      status: 'Ready',
      pnl: '+$2,156.80'
    },
    {
      id: 'crosschain' as const,
      name: 'Cross-Chain Transactions',
      icon: ArrowUpDown,
      description: 'Multi-chain asset management',
      status: 'Connected',
      pnl: '+$678.90'
    }
  ];

  const handleSendMessage = async () => {
    if (!chatMessage.trim()) return;
    
    const userMessage = chatMessage;
    setChatMessage('');
    setChatHistory(prev => [...prev, { role: 'user', message: userMessage }]);
    setIsProcessing(true);

    // Simulate AI response
    setTimeout(() => {
      const responses = [
        'I\'ve analyzed your portfolio and suggest reducing exposure to volatile assets by 15%.',
        'Based on current market conditions, I recommend setting a stop-loss at $42.50 for your PDOT position.',
        'Cross-chain arbitrage opportunity detected: 3.2% profit potential between Stellar and Ethereum.',
        'Your portfolio is well-balanced. Consider taking profits on your SOL position (+24.5%).',
        'Telegram alerts have been configured for price movements above 5%.'
      ];
      const randomResponse = responses[Math.floor(Math.random() * responses.length)];
      setChatHistory(prev => [...prev, { role: 'ai', message: randomResponse }]);
      setIsProcessing(false);
    }, 1500);
  };

  const StrategyCard = ({ strategy }: { strategy: typeof strategies[0] }) => (
    <div 
      className={`glass-card cursor-pointer transition-all duration-300 hover:scale-105 ${
        activeStrategy === strategy.id ? 'ring-2 ring-emerald-400/50' : ''
      }`}
      onClick={() => setActiveStrategy(strategy.id)}
    >
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center space-x-3">
          <div className="p-2 bg-gradient-to-r from-emerald-500/20 to-teal-500/20 rounded-lg">
            <strategy.icon className="w-5 h-5 text-emerald-400" />
          </div>
          <div>
            <h3 className="font-semibold text-slate-900 dark:text-slate-100">{strategy.name}</h3>
            <p className="text-sm text-slate-600 dark:text-slate-400">{strategy.description}</p>
          </div>
        </div>
        <div className="text-right">
          <div className={`text-xs px-2 py-1 rounded-full ${
            strategy.status === 'Active' ? 'bg-emerald-500/20 text-emerald-400' :
            strategy.status === 'Monitoring' ? 'bg-yellow-500/20 text-yellow-400' :
            strategy.status === 'Ready' ? 'bg-blue-500/20 text-blue-400' :
            'bg-purple-500/20 text-purple-400'
          }`}>
            {strategy.status}
          </div>
          <div className="text-sm font-bold text-emerald-400 mt-1">{strategy.pnl}</div>
        </div>
      </div>
    </div>
  );

  return (
    <div className="max-w-6xl mx-auto space-y-8">
      {/* Header */}
      <div className="text-center mb-8">
        <div className="flex items-center justify-center space-x-3 mb-4">
          <div className="p-3 bg-gradient-to-r from-emerald-500/20 to-teal-500/20 rounded-xl">
            <Bot className="w-8 h-8 text-emerald-400" />
          </div>
          <h2 className="text-2xl font-bold text-slate-900 dark:text-slate-100">
            AI Trading Agent
          </h2>
        </div>
        <p className="text-slate-600 dark:text-slate-400">
          Your intelligent companion for automated trading and portfolio management
        </p>
      </div>

      {/* Strategy Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {strategies.map((strategy) => (
          <StrategyCard key={strategy.id} strategy={strategy} />
        ))}
      </div>

      {/* Active Strategy Details */}
      <div className="glass-card">
        <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-6">
          {strategies.find(s => s.id === activeStrategy)?.name} Configuration
        </h3>
        
        {activeStrategy === 'stop-loss' && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                  Stop Loss Percentage
                </label>
                <div className="relative">
                  <input 
                    type="number" 
                    defaultValue="5" 
                    className="w-full px-4 py-3 bg-white/10 dark:bg-white/5 border border-white/20 dark:border-white/10 rounded-lg text-slate-900 dark:text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-emerald-400/50"
                  />
                  <span className="absolute right-3 top-1/2 transform -translate-y-1/2 text-slate-500">%</span>
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                  Take Profit Percentage
                </label>
                <div className="relative">
                  <input 
                    type="number" 
                    defaultValue="15" 
                    className="w-full px-4 py-3 bg-white/10 dark:bg-white/5 border border-white/20 dark:border-white/10 rounded-lg text-slate-900 dark:text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-emerald-400/50"
                  />
                  <span className="absolute right-3 top-1/2 transform -translate-y-1/2 text-slate-500">%</span>
                </div>
              </div>
            </div>
            <div className="space-y-4">
              <div className="p-4 bg-gradient-to-r from-emerald-500/10 to-teal-500/10 rounded-lg border border-emerald-400/20">
                <h4 className="font-medium text-emerald-400 mb-2">Active Positions</h4>
                <div className="space-y-2 text-sm">
                  <div className="flex justify-between">
                    <span className="text-slate-600 dark:text-slate-400">PDOT</span>
                    <span className="text-slate-900 dark:text-slate-100">Stop: $42.50 | Take: $58.00</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-600 dark:text-slate-400">XLM</span>
                    <span className="text-slate-900 dark:text-slate-100">Stop: $0.095 | Take: $0.138</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}

        {activeStrategy === 'portfolio' && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <div className="space-y-4">
              <h4 className="font-medium text-slate-900 dark:text-slate-100">Target Allocation</h4>
              <div className="space-y-3">
                {['PDOT', 'XLM', 'USDC', 'ETH', 'SOL'].map((asset) => (
                  <div key={asset} className="flex items-center justify-between">
                    <span className="text-slate-600 dark:text-slate-400">{asset}</span>
                    <input 
                      type="number" 
                      defaultValue={asset === 'PDOT' ? '40' : asset === 'XLM' ? '30' : '10'} 
                      className="w-16 px-2 py-1 bg-white/10 dark:bg-white/5 border border-white/20 dark:border-white/10 rounded text-sm text-slate-900 dark:text-slate-100 text-center"
                    />
                  </div>
                ))}
              </div>
            </div>
            <div className="space-y-4">
              <h4 className="font-medium text-slate-900 dark:text-slate-100">Current Allocation</h4>
              <div className="space-y-3">
                {['45%', '25%', '15%', '10%', '5%'].map((percentage, idx) => (
                  <div key={idx} className="flex items-center justify-between">
                    <span className="text-slate-600 dark:text-slate-400">
                      {['PDOT', 'XLM', 'USDC', 'ETH', 'SOL'][idx]}
                    </span>
                    <span className="text-slate-900 dark:text-slate-100">{percentage}</span>
                  </div>
                ))}
              </div>
            </div>
            <div className="space-y-4">
              <h4 className="font-medium text-slate-900 dark:text-slate-100">Rebalance Actions</h4>
              <div className="space-y-2 text-sm">
                <div className="flex items-center space-x-2 text-red-400">
                  <ArrowUpDown className="w-4 h-4" />
                  <span>Sell 5% PDOT</span>
                </div>
                <div className="flex items-center space-x-2 text-emerald-400">
                  <ArrowUpDown className="w-4 h-4" />
                  <span>Buy 5% XLM</span>
                </div>
                <div className="flex items-center space-x-2 text-emerald-400">
                  <ArrowUpDown className="w-4 h-4" />
                  <span>Buy 5% SOL</span>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Telegram Integration */}
      <div className="glass-card">
        <div className="flex items-center space-x-3 mb-6">
          <MessageCircle className="w-6 h-6 text-emerald-400" />
          <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100">
            Telegram Integration
          </h3>
          <div className="px-3 py-1 bg-emerald-500/20 text-emerald-400 text-xs rounded-full">
            Connected
          </div>
        </div>
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div>
            <h4 className="font-medium text-slate-900 dark:text-slate-100 mb-3">Alert Settings</h4>
            <div className="space-y-3">
              {[
                'Price movements > 5%',
                'Stop loss triggered',
                'Take profit executed',
                'Rebalance completed',
                'Cross-chain opportunities'
              ].map((alert, idx) => (
                <label key={idx} className="flex items-center space-x-3">
                  <input 
                    type="checkbox" 
                    defaultChecked={idx < 3}
                    className="w-4 h-4 text-emerald-400 bg-transparent border-white/20 rounded focus:ring-emerald-400/50" 
                  />
                  <span className="text-sm text-slate-600 dark:text-slate-400">{alert}</span>
                </label>
              ))}
            </div>
          </div>
          <div>
            <h4 className="font-medium text-slate-900 dark:text-slate-100 mb-3">Recent Alerts</h4>
            <div className="space-y-2 text-sm">
              <div className="flex items-center space-x-2">
                <CheckCircle className="w-4 h-4 text-emerald-400" />
                <span className="text-slate-600 dark:text-slate-400">Take profit executed: PDOT +15%</span>
              </div>
              <div className="flex items-center space-x-2">
                <AlertTriangle className="w-4 h-4 text-yellow-400" />
                <span className="text-slate-600 dark:text-slate-400">XLM approaching stop loss (-4.2%)</span>
              </div>
              <div className="flex items-center space-x-2">
                <CheckCircle className="w-4 h-4 text-emerald-400" />
                <span className="text-slate-600 dark:text-slate-400">Portfolio rebalanced successfully</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* AI Chat Interface */}
      <div className="glass-card">
        <div className="flex items-center space-x-3 mb-6">
          <Bot className="w-6 h-6 text-emerald-400" />
          <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100">
            AI Assistant Chat
          </h3>
        </div>
        
        <div className="space-y-4">
          <div className="h-64 overflow-y-auto space-y-3 p-4 bg-white/5 dark:bg-white/5 rounded-lg border border-white/10">
            {chatHistory.map((chat, idx) => (
              <div key={idx} className={`flex ${chat.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                <div className={`max-w-xs px-4 py-2 rounded-lg ${
                  chat.role === 'user' 
                    ? 'bg-emerald-500/20 text-emerald-100' 
                    : 'bg-slate-500/20 text-slate-100'
                }`}>
                  {chat.message}
                </div>
              </div>
            ))}
            {isProcessing && (
              <div className="flex justify-start">
                <div className="bg-slate-500/20 text-slate-100 px-4 py-2 rounded-lg">
                  <div className="flex space-x-1">
                    <div className="w-2 h-2 bg-emerald-400 rounded-full animate-bounce"></div>
                    <div className="w-2 h-2 bg-emerald-400 rounded-full animate-bounce" style={{ animationDelay: '0.1s' }}></div>
                    <div className="w-2 h-2 bg-emerald-400 rounded-full animate-bounce" style={{ animationDelay: '0.2s' }}></div>
                  </div>
                </div>
              </div>
            )}
          </div>
          
          <div className="flex space-x-3">
            <input
              type="text"
              value={chatMessage}
              onChange={(e) => setChatMessage(e.target.value)}
              onKeyPress={(e) => e.key === 'Enter' && handleSendMessage()}
              placeholder="Ask me about your portfolio, strategies, or market insights..."
              className="flex-1 px-4 py-3 bg-white/10 dark:bg-white/5 border border-white/20 dark:border-white/10 rounded-lg text-slate-900 dark:text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-emerald-400/50"
            />
            <button
              onClick={handleSendMessage}
              disabled={!chatMessage.trim() || isProcessing}
              className="px-6 py-3 bg-gradient-to-r from-emerald-500 to-teal-500 text-white rounded-lg hover:from-emerald-600 hover:to-teal-600 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200"
            >
              <Send className="w-5 h-5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
} 