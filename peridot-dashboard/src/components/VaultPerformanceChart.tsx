'use client';

import { useState, useEffect } from 'react';
import { LineChart, Line, AreaChart, Area, PieChart, Pie, Cell, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, BarChart, Bar } from 'recharts';
import { TrendingUp, BarChart3, PieChart as PieChartIcon, Filter, Calendar, DollarSign, Percent, ArrowUpRight, ArrowDownRight, Eye, EyeOff, ChevronDown, Info, AlertTriangle, Shield, Settings } from 'lucide-react';

interface VaultPerformanceChartProps {
  walletInfo: any;
  selectedAsset: 'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL';
}

// Mock data - in production this would come from your API
const generateAPYData = (timeframe: '7d' | '30d' | '90d') => {
  const data = [];
  const baseDate = new Date();
  const days = timeframe === '7d' ? 7 : timeframe === '30d' ? 30 : 90;
  
  for (let i = days; i >= 0; i--) {
    const date = new Date(baseDate);
    date.setDate(date.getDate() - i);
    data.push({
      date: date.toISOString().split('T')[0],
      'PDOT': 8.5 + Math.random() * 2 - 1,
      'XLM': 6.2 + Math.random() * 1.5 - 0.75,
      'USDC': 4.8 + Math.random() * 1 - 0.5,
      'ETH': 5.2 + Math.random() * 2 - 1,
      'SOL': 7.1 + Math.random() * 1.8 - 0.9,
    });
  }
  return data;
};

const generatePerformanceData = (timeframe: '7d' | '30d' | '90d') => {
  const data = [];
  const baseDate = new Date();
  const days = timeframe === '7d' ? 7 : timeframe === '30d' ? 30 : 90;
  let totalDeposit = 1000;
  let totalEarnings = 0;
  
  for (let i = days; i >= 0; i--) {
    const date = new Date(baseDate);
    date.setDate(date.getDate() - i);
    const dailyEarning = (totalDeposit * 0.08) / 365; // 8% APY daily
    totalEarnings += dailyEarning;
    
    data.push({
      date: date.toISOString().split('T')[0],
      totalDeposit,
      totalEarnings,
      totalValue: totalDeposit + totalEarnings,
      pTokenValue: 1 + (totalEarnings / totalDeposit),
    });
    
    if (Math.random() > 0.8) totalDeposit += Math.random() * 200;
  }
  return data;
};

const vaultDistributionData = [
  { name: 'Peridot Token (PDOT)', value: 70, color: '#10b981', absoluteValue: 142500 },
  { name: 'USD Coin (USDC)', value: 20, color: '#3b82f6', absoluteValue: 40714 },
  { name: 'Stellar Lumens (XLM)', value: 10, color: '#8b5cf6', absoluteValue: 20357 },
];

export default function VaultPerformanceChart({ walletInfo, selectedAsset }: VaultPerformanceChartProps) {
  const [activeChart, setActiveChart] = useState<'apy' | 'performance' | 'distribution'>('apy');
  const [selectedAssets, setSelectedAssets] = useState<string[]>(['PDOT', 'XLM', 'USDC', 'ETH', 'SOL']);
  const [timeframe, setTimeframe] = useState<'7d' | '30d' | '90d'>('30d');
  const [apyData, setApyData] = useState(generateAPYData('30d'));
  const [performanceData, setPerformanceData] = useState(generatePerformanceData('30d'));
  const [showTooltip, setShowTooltip] = useState(true);
  const [isFiltersExpanded, setIsFiltersExpanded] = useState(false);

  // Get real user data
  const userSuppliedPDOT = parseFloat(walletInfo?.pTokenBalance || '0');
  const borrowedAmountsFromStorage = typeof window !== 'undefined' ? 
    JSON.parse(localStorage.getItem(`borrowed_amounts_${walletInfo?.address}`) || '{"PDOT": 0, "XLM": 0, "USDC": 0, "ETH": 0, "SOL": 0}') :
    {"PDOT": 0, "XLM": 0, "USDC": 0, "ETH": 0, "SOL": 0};

  // Asset-specific reserve configuration with real data
  const assetConfig = {
    PDOT: {
      name: 'Peridot Token',
      totalSupply: userSuppliedPDOT + 50000, // User supply + protocol liquidity
      totalBorrowed: borrowedAmountsFromStorage.PDOT + 25000, // User borrowed + others
      reserveFactor: 15,
              utilizationRate: userSuppliedPDOT > 0 ? Math.round(((borrowedAmountsFromStorage.PDOT + 25000) / (userSuppliedPDOT + 50000) * 100) * 100) / 100 : 50.0,
      liquidationThreshold: 80,
      ltv: 75,
      status: 'active' as const,
      borrowCap: 200000,
      supplyCap: 500000,
      reserveSize: Math.floor((borrowedAmountsFromStorage.PDOT + 25000) * 0.15)
    },
    XLM: {
      name: 'Stellar Lumens',
      totalSupply: borrowedAmountsFromStorage.XLM + 40000,
      totalBorrowed: borrowedAmountsFromStorage.XLM,
      reserveFactor: 20,
              utilizationRate: borrowedAmountsFromStorage.XLM > 0 ? Math.max(0.01, Math.round((borrowedAmountsFromStorage.XLM / (borrowedAmountsFromStorage.XLM + 40000) * 100) * 100) / 100) : 0,
      liquidationThreshold: 85,
      ltv: 80,
      status: 'active' as const,
      borrowCap: 100000,
      supplyCap: 200000,
      reserveSize: Math.floor(borrowedAmountsFromStorage.XLM * 0.2)
    },
    USDC: {
      name: 'USD Coin',
      totalSupply: borrowedAmountsFromStorage.USDC + 20000,
      totalBorrowed: borrowedAmountsFromStorage.USDC,
      reserveFactor: 10,
              utilizationRate: borrowedAmountsFromStorage.USDC > 0 ? Math.max(0.01, Math.round((borrowedAmountsFromStorage.USDC / (borrowedAmountsFromStorage.USDC + 20000) * 100) * 100) / 100) : 0,
      liquidationThreshold: 90,
      ltv: 85,
      status: 'active' as const,
      borrowCap: 50000,
      supplyCap: 100000,
      reserveSize: Math.floor(borrowedAmountsFromStorage.USDC * 0.1)
    },
    ETH: {
      name: 'Ethereum',
      totalSupply: borrowedAmountsFromStorage.ETH + 8000,
      totalBorrowed: borrowedAmountsFromStorage.ETH,
      reserveFactor: 15,
              utilizationRate: borrowedAmountsFromStorage.ETH > 0 ? Math.max(0.01, Math.round((borrowedAmountsFromStorage.ETH / (borrowedAmountsFromStorage.ETH + 8000) * 100) * 100) / 100) : 0,
      liquidationThreshold: 82,
      ltv: 77,
      status: 'active' as const,
      borrowCap: 20000,
      supplyCap: 40000,
      reserveSize: Math.floor(borrowedAmountsFromStorage.ETH * 0.15)
    },
    SOL: {
      name: 'Solana',
      totalSupply: borrowedAmountsFromStorage.SOL + 12000,
      totalBorrowed: borrowedAmountsFromStorage.SOL,
      reserveFactor: 20,
              utilizationRate: borrowedAmountsFromStorage.SOL > 0 ? Math.max(0.01, Math.round((borrowedAmountsFromStorage.SOL / (borrowedAmountsFromStorage.SOL + 12000) * 100) * 100) / 100) : 0,
      liquidationThreshold: 78,
      ltv: 73,
      status: 'active' as const,
      borrowCap: 30000,
      supplyCap: 60000,
      reserveSize: Math.floor(borrowedAmountsFromStorage.SOL * 0.2)
    }
  };

  const currentAssetConfig = assetConfig[selectedAsset];

  // Update data when timeframe changes
  useEffect(() => {
    setApyData(generateAPYData(timeframe));
    setPerformanceData(generatePerformanceData(timeframe));
  }, [timeframe]);

  const assetColors = {
    'PDOT': '#10b981',
    'XLM': '#3b82f6', 
    'USDC': '#8b5cf6',
    'ETH': '#f59e0b',
    'SOL': '#ec4899'
  };

  const toggleAsset = (asset: string) => {
    setSelectedAssets(prev => 
      prev.includes(asset) 
        ? prev.filter(a => a !== asset)
        : [...prev, asset]
    );
  };

  const CustomTooltip = ({ active, payload, label }: any) => {
    if (active && payload && payload.length && showTooltip) {
      return (
        <div className="glass-card border border-white/20 dark:border-white/10 p-3 shadow-xl backdrop-blur-2xl rounded-lg">
          <p className="text-sm font-semibold text-slate-900 dark:text-white mb-2">{label}</p>
          {payload.map((entry: any, index: number) => (
            <div key={index} className="flex items-center justify-between space-x-3 mb-1">
              <div className="flex items-center space-x-2">
                <div 
                  className="w-3 h-3 rounded-full" 
                  style={{ backgroundColor: entry.color }}
                />
                <span className="text-xs text-slate-700 dark:text-slate-300">{entry.dataKey}:</span>
              </div>
              <span className="text-xs font-bold text-slate-900 dark:text-white">
                {activeChart === 'apy' ? `${entry.value.toFixed(2)}%` : 
                 activeChart === 'performance' ? `$${entry.value.toLocaleString()}` :
                 `${entry.value}%`}
              </span>
            </div>
          ))}
        </div>
      );
    }
    return null;
  };

  const CustomPieTooltip = ({ active, payload }: any) => {
    if (active && payload && payload.length && showTooltip) {
      const data = payload[0].payload;
      return (
        <div className="glass-card border border-white/20 dark:border-white/10 p-3 shadow-xl backdrop-blur-2xl rounded-lg">
          <div className="flex items-center space-x-2 mb-2">
            <div 
              className="w-3 h-3 rounded-full" 
              style={{ backgroundColor: data.color }}
            />
            <span className="text-sm font-semibold text-slate-900 dark:text-white">{data.name}</span>
          </div>
          <div className="space-y-1">
            <div className="flex justify-between">
              <span className="text-xs text-slate-700 dark:text-slate-300">Percentage:</span>
              <span className="text-xs font-bold text-slate-900 dark:text-white">{data.value}%</span>
            </div>
            <div className="flex justify-between">
              <span className="text-xs text-slate-700 dark:text-slate-300">Value:</span>
              <span className="text-xs font-bold text-slate-900 dark:text-white">${data.absoluteValue.toLocaleString()}</span>
            </div>
          </div>
        </div>
      );
    }
    return null;
  };

  return (
    <div className="glass-card space-y-4 md:space-y-6">
      {/* Compact Mobile Header */}
      <div className="space-y-3">
        <div className="flex items-start justify-between">
          <div className="flex items-center space-x-3">
            <div className="w-8 h-8 md:w-10 md:h-10 rounded-xl bg-gradient-to-br from-purple-500 to-blue-500 flex items-center justify-center shadow-lg">
              <BarChart3 className="w-4 h-4 md:w-5 md:h-5 text-white" />
            </div>
            <div className="min-w-0 flex-1">
              <h3 className="text-base md:text-lg font-bold text-slate-900 dark:text-white leading-tight">
                {currentAssetConfig.name} Analytics
              </h3>
              <p className="text-xs md:text-sm text-slate-600 dark:text-slate-400 leading-tight">
                Reserve status & performance
              </p>
            </div>
          </div>

          <button
            onClick={() => setShowTooltip(!showTooltip)}
            className="p-2 rounded-lg bg-white/10 dark:bg-white/5 border border-white/20 dark:border-white/10 text-slate-700 dark:text-slate-300 hover:bg-white/20 transition-all duration-200"
            title={showTooltip ? 'Hide tooltips' : 'Show tooltips'}
          >
            {showTooltip ? <Eye className="w-4 h-4" /> : <EyeOff className="w-4 h-4" />}
          </button>
        </div>

        {/* Reserve Status & Configuration */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 p-4 rounded-xl bg-gradient-to-br from-slate-50/50 via-white/30 to-slate-100/50 dark:from-slate-800/50 dark:via-slate-700/30 dark:to-slate-800/50 border border-slate-200/30 dark:border-slate-600/20 backdrop-blur-lg">
          <div className="group relative">
            <div className="flex items-center space-x-2 mb-1">
              <Settings className="w-3 h-3 text-blue-500" />
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400">Utilization</span>
              <div className="group/tooltip relative">
                <Info className="w-3 h-3 text-slate-400 hover:text-slate-600 cursor-help" />
                <div className="absolute bottom-full left-1/2 transform -translate-x-1/2 mb-2 px-2 py-1 bg-slate-900 text-white text-xs rounded opacity-0 group-hover/tooltip:opacity-100 transition-opacity whitespace-nowrap z-10">
                  Total borrowed / Total supplied
                </div>
              </div>
            </div>
            <p className="text-lg font-bold text-slate-900 dark:text-white">
              {currentAssetConfig.utilizationRate < 0.01 && currentAssetConfig.utilizationRate > 0 
                ? "< 0.01%" 
                : `${currentAssetConfig.utilizationRate}%`}
            </p>
            <div className="w-full bg-slate-200 dark:bg-slate-700 rounded-full h-1.5 mt-1">
              <div 
                className="bg-gradient-to-r from-blue-500 to-cyan-500 h-1.5 rounded-full transition-all duration-500"
                style={{ width: `${currentAssetConfig.utilizationRate}%` }}
              ></div>
            </div>
          </div>

          <div className="group relative">
            <div className="flex items-center space-x-2 mb-1">
              <Shield className="w-3 h-3 text-emerald-500" />
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400">LTV</span>
              <div className="group/tooltip relative">
                <Info className="w-3 h-3 text-slate-400 hover:text-slate-600 cursor-help" />
                <div className="absolute bottom-full left-1/2 transform -translate-x-1/2 mb-2 px-2 py-1 bg-slate-900 text-white text-xs rounded opacity-0 group-hover/tooltip:opacity-100 transition-opacity whitespace-nowrap z-10">
                  Max loan-to-value ratio
                </div>
              </div>
            </div>
            <p className="text-lg font-bold text-emerald-600 dark:text-emerald-400">{currentAssetConfig.ltv}%</p>
          </div>

          <div className="group relative">
            <div className="flex items-center space-x-2 mb-1">
              <AlertTriangle className="w-3 h-3 text-orange-500" />
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400">Liq. Threshold</span>
              <div className="group/tooltip relative">
                <Info className="w-3 h-3 text-slate-400 hover:text-slate-600 cursor-help" />
                <div className="absolute bottom-full left-1/2 transform -translate-x-1/2 mb-2 px-2 py-1 bg-slate-900 text-white text-xs rounded opacity-0 group-hover/tooltip:opacity-100 transition-opacity whitespace-nowrap z-10">
                  Liquidation threshold
                </div>
              </div>
            </div>
            <p className="text-lg font-bold text-orange-600 dark:text-orange-400">{currentAssetConfig.liquidationThreshold}%</p>
          </div>

          <div className="group relative">
            <div className="flex items-center space-x-2 mb-1">
              <DollarSign className="w-3 h-3 text-violet-500" />
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400">Reserve</span>
              <div className="group/tooltip relative">
                <Info className="w-3 h-3 text-slate-400 hover:text-slate-600 cursor-help" />
                <div className="absolute bottom-full left-1/2 transform -translate-x-1/2 mb-2 px-2 py-1 bg-slate-900 text-white text-xs rounded opacity-0 group-hover/tooltip:opacity-100 transition-opacity whitespace-nowrap z-10">
                  Protocol reserve size
                </div>
              </div>
            </div>
            <p className="text-lg font-bold text-violet-600 dark:text-violet-400">{currentAssetConfig.reserveSize.toLocaleString()}</p>
          </div>
        </div>

        {/* Market Overview */}
        <div className="grid grid-cols-2 gap-3 p-4 rounded-xl bg-gradient-to-br from-slate-50/30 via-white/20 to-slate-100/30 dark:from-slate-800/30 dark:via-slate-700/20 dark:to-slate-800/30 border border-slate-200/20 dark:border-slate-600/15 backdrop-blur-lg">
          <div className="text-center">
            <div className="flex items-center justify-center space-x-1 mb-1">
              <ArrowUpRight className="w-3 h-3 text-emerald-500" />
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400">Total Supply</span>
            </div>
            <p className="text-xl font-bold text-emerald-600 dark:text-emerald-400">{currentAssetConfig.totalSupply.toLocaleString()}</p>
            <p className="text-xs text-slate-500 dark:text-slate-400">Cap: {currentAssetConfig.supplyCap.toLocaleString()}</p>
          </div>
          
          <div className="text-center">
            <div className="flex items-center justify-center space-x-1 mb-1">
              <ArrowDownRight className="w-3 h-3 text-orange-500" />
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400">Total Borrowed</span>
            </div>
            <p className="text-xl font-bold text-orange-600 dark:text-orange-400">{currentAssetConfig.totalBorrowed.toLocaleString()}</p>
            <p className="text-xs text-slate-500 dark:text-slate-400">Cap: {currentAssetConfig.borrowCap.toLocaleString()}</p>
          </div>
        </div>

        {/* Chart Type Selector - Mobile Optimized */}
        <div className="w-full">
          <div className="flex w-full backdrop-blur-2xl bg-white/10 dark:bg-white/5 rounded-xl p-1 border border-white/20 dark:border-white/10">
            <button
              onClick={() => setActiveChart('apy')}
              className={`flex-1 px-2 md:px-3 py-2 rounded-lg text-xs md:text-sm font-semibold transition-all duration-300 ${
                activeChart === 'apy'
                  ? 'bg-gradient-to-r from-purple-500 to-blue-500 text-white shadow-lg'
                  : 'text-slate-700 dark:text-slate-300 hover:bg-white/10'
              }`}
            >
              <TrendingUp className="w-3 h-3 md:w-4 md:h-4 inline mr-1" />
              <span className="hidden sm:inline">APY</span>
              <span className="sm:hidden">APY</span>
            </button>
            <button
              onClick={() => setActiveChart('performance')}
              className={`flex-1 px-2 md:px-3 py-2 rounded-lg text-xs md:text-sm font-semibold transition-all duration-300 ${
                activeChart === 'performance'
                  ? 'bg-gradient-to-r from-purple-500 to-blue-500 text-white shadow-lg'
                  : 'text-slate-700 dark:text-slate-300 hover:bg-white/10'
              }`}
            >
              <BarChart3 className="w-3 h-3 md:w-4 md:h-4 inline mr-1" />
              <span className="hidden sm:inline">Portfolio</span>
              <span className="sm:hidden">Port.</span>
            </button>
            <button
              onClick={() => setActiveChart('distribution')}
              className={`flex-1 px-2 md:px-3 py-2 rounded-lg text-xs md:text-sm font-semibold transition-all duration-300 ${
                activeChart === 'distribution'
                  ? 'bg-gradient-to-r from-purple-500 to-blue-500 text-white shadow-lg'
                  : 'text-slate-700 dark:text-slate-300 hover:bg-white/10'
              }`}
            >
              <PieChartIcon className="w-3 h-3 md:w-4 md:h-4 inline mr-1" />
              <span className="hidden sm:inline">Distribution</span>
              <span className="sm:hidden">Dist.</span>
            </button>
          </div>
        </div>
      </div>

      {/* Mobile-First Collapsible Controls */}
      {activeChart !== 'distribution' && (
        <div className="space-y-3">
          <button
            onClick={() => setIsFiltersExpanded(!isFiltersExpanded)}
            className="md:hidden flex items-center justify-between w-full p-2 rounded-lg bg-white/5 border border-white/10 text-slate-700 dark:text-slate-300"
          >
            <span className="text-sm font-medium">Chart Options</span>
            <ChevronDown className={`w-4 h-4 transition-transform ${isFiltersExpanded ? 'rotate-180' : ''}`} />
          </button>
          
          <div className={`space-y-3 ${isFiltersExpanded ? 'block' : 'hidden md:block'}`}>
            {/* Asset Filters - Mobile Stack */}
            <div className="space-y-2">
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400 uppercase tracking-wide">Assets</span>
              <div className="grid grid-cols-2 sm:flex sm:flex-wrap gap-2">
                {Object.entries(assetColors).map(([asset, color]) => (
                  <button
                    key={asset}
                    onClick={() => toggleAsset(asset)}
                    className={`px-3 py-2 rounded-lg text-xs font-semibold border transition-all duration-200 ${
                      selectedAssets.includes(asset)
                        ? `border-[${color}] bg-gradient-to-r text-white shadow-lg`
                        : 'border-white/20 dark:border-white/10 bg-white/5 text-slate-700 dark:text-slate-300 hover:bg-white/10'
                    }`}
                    style={selectedAssets.includes(asset) ? {
                      backgroundColor: color,
                      borderColor: color,
                      boxShadow: `0 4px 12px ${color}30`
                    } : {}}
                  >
                    <div className="flex items-center justify-center space-x-1">
                      <div 
                        className="w-2 h-2 rounded-full" 
                        style={{ backgroundColor: selectedAssets.includes(asset) ? 'white' : color }}
                      />
                      <span>{asset}</span>
                    </div>
                  </button>
                ))}
              </div>
            </div>

            {/* Timeframe Selector */}
            <div className="space-y-2">
              <span className="text-xs font-medium text-slate-600 dark:text-slate-400 uppercase tracking-wide">Period</span>
              <div className="flex w-full backdrop-blur-2xl bg-white/10 dark:bg-white/5 rounded-lg p-1 border border-white/20 dark:border-white/10">
                {(['7d', '30d', '90d'] as const).map((tf) => (
                  <button
                    key={tf}
                    onClick={() => setTimeframe(tf)}
                    className={`flex-1 px-3 py-2 rounded-md text-xs font-semibold transition-all duration-200 ${
                      timeframe === tf
                        ? 'bg-gradient-to-r from-blue-500 to-purple-500 text-white shadow-lg'
                        : 'text-slate-700 dark:text-slate-300 hover:bg-white/10'
                    }`}
                  >
                    {tf}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Chart Area - Mobile Optimized Height */}
      <div className="h-64 sm:h-80 md:h-96 w-full">
        {activeChart === 'apy' && (
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={apyData} margin={{ top: 10, right: 10, left: 0, bottom: 10 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" opacity={0.3} />
              <XAxis 
                dataKey="date" 
                tick={{ fontSize: 10, fill: '#64748b' }}
                axisLine={{ stroke: '#cbd5e1', strokeWidth: 1 }}
                interval="preserveStartEnd"
              />
              <YAxis 
                tick={{ fontSize: 10, fill: '#64748b' }}
                axisLine={{ stroke: '#cbd5e1', strokeWidth: 1 }}
                width={40}
              />
              <Tooltip content={<CustomTooltip />} />
              {selectedAssets.map((asset) => (
                <Line
                  key={asset}
                  type="monotone"
                  dataKey={asset}
                  stroke={assetColors[asset as keyof typeof assetColors]}
                  strokeWidth={2}
                  dot={false}
                  activeDot={{ r: 4, stroke: assetColors[asset as keyof typeof assetColors], strokeWidth: 2 }}
                />
              ))}
            </LineChart>
          </ResponsiveContainer>
        )}

        {activeChart === 'performance' && (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={performanceData} margin={{ top: 10, right: 10, left: 0, bottom: 10 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" opacity={0.3} />
              <XAxis 
                dataKey="date" 
                tick={{ fontSize: 10, fill: '#64748b' }}
                axisLine={{ stroke: '#cbd5e1', strokeWidth: 1 }}
                interval="preserveStartEnd"
              />
              <YAxis 
                tick={{ fontSize: 10, fill: '#64748b' }}
                axisLine={{ stroke: '#cbd5e1', strokeWidth: 1 }}
                width={50}
              />
              <Tooltip content={<CustomTooltip />} />
              <Area
                type="monotone"
                dataKey="totalDeposit"
                stackId="1"
                stroke="#3b82f6"
                fill="#3b82f6"
                fillOpacity={0.6}
              />
              <Area
                type="monotone"
                dataKey="totalEarnings"
                stackId="1"
                stroke="#10b981"
                fill="#10b981"
                fillOpacity={0.8}
              />
            </AreaChart>
          </ResponsiveContainer>
        )}

        {activeChart === 'distribution' && (
          <div className="space-y-4">
            <div className="flex items-center justify-center h-full min-h-[200px]">
              <div className="w-full max-w-md">
                <ResponsiveContainer width="100%" height={250}>
                  <PieChart>
                    <Pie
                      data={vaultDistributionData}
                      cx="50%"
                      cy="50%"
                      innerRadius={50}
                      outerRadius={100}
                      paddingAngle={3}
                      dataKey="value"
                      label={({ name, value }) => `${name}: ${value}%`}
                      labelLine={false}
                    >
                      {vaultDistributionData.map((entry, index) => (
                        <Cell key={`cell-${index}`} fill={entry.color} />
                      ))}
                    </Pie>
                    <Tooltip content={<CustomPieTooltip />} />
                  </PieChart>
                </ResponsiveContainer>
              </div>
            </div>
            
            {/* Distribution Legend */}
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              {vaultDistributionData.map((item, index) => (
                <div key={index} className="flex items-center justify-between p-3 rounded-lg bg-white/5 border border-white/10">
                  <div className="flex items-center space-x-2">
                    <div 
                      className="w-4 h-4 rounded-full" 
                      style={{ backgroundColor: item.color }}
                    />
                    <span className="text-sm font-medium text-slate-900 dark:text-white">{item.name}</span>
                  </div>
                  <div className="text-right">
                    <p className="text-sm font-bold text-slate-900 dark:text-white">{item.value}%</p>
                    <p className="text-xs text-slate-600 dark:text-slate-400">${item.absoluteValue.toLocaleString()}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Key Metrics - Refined Liquid Glass Design */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 md:gap-4">
        <div className="group relative text-center p-4 md:p-5 rounded-xl bg-gradient-to-br from-slate-50/8 via-white/4 to-slate-100/6 dark:from-slate-800/12 dark:via-slate-700/6 dark:to-slate-800/8 backdrop-blur-xl border border-slate-200/15 dark:border-slate-700/20 shadow-[0_4px_20px_-4px_rgba(0,0,0,0.08)] hover:shadow-[0_8px_30px_-8px_rgba(0,0,0,0.12)] transition-all duration-400 hover:scale-[1.01] hover:bg-gradient-to-br hover:from-slate-50/12 hover:via-white/6 hover:to-slate-100/10 dark:hover:from-slate-800/16 dark:hover:via-slate-700/8 dark:hover:to-slate-800/12">
          <div className="absolute inset-0 rounded-xl bg-gradient-to-br from-emerald-500/3 via-transparent to-emerald-400/2 opacity-0 group-hover:opacity-100 transition-opacity duration-400" />
          <div className="relative z-10">
            <div className="flex items-center justify-center mb-3">
              <div className="w-7 h-7 md:w-8 md:h-8 rounded-lg bg-gradient-to-br from-emerald-500/20 to-emerald-600/15 flex items-center justify-center shadow-sm">
                <ArrowUpRight className="w-3.5 h-3.5 md:w-4 md:h-4 text-emerald-600 dark:text-emerald-400" />
              </div>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400 mb-2 font-medium tracking-wider uppercase opacity-80">Current APY</p>
            <p className="text-lg md:text-xl font-semibold text-slate-700 dark:text-slate-200 leading-tight">8.5%</p>
          </div>
        </div>
        
        <div className="group relative text-center p-4 md:p-5 rounded-xl bg-gradient-to-br from-slate-50/8 via-white/4 to-slate-100/6 dark:from-slate-800/12 dark:via-slate-700/6 dark:to-slate-800/8 backdrop-blur-xl border border-slate-200/15 dark:border-slate-700/20 shadow-[0_4px_20px_-4px_rgba(0,0,0,0.08)] hover:shadow-[0_8px_30px_-8px_rgba(0,0,0,0.12)] transition-all duration-400 hover:scale-[1.01] hover:bg-gradient-to-br hover:from-slate-50/12 hover:via-white/6 hover:to-slate-100/10 dark:hover:from-slate-800/16 dark:hover:via-slate-700/8 dark:hover:to-slate-800/12">
          <div className="absolute inset-0 rounded-xl bg-gradient-to-br from-blue-500/3 via-transparent to-blue-400/2 opacity-0 group-hover:opacity-100 transition-opacity duration-400" />
          <div className="relative z-10">
            <div className="flex items-center justify-center mb-3">
              <div className="w-7 h-7 md:w-8 md:h-8 rounded-lg bg-gradient-to-br from-blue-500/20 to-blue-600/15 flex items-center justify-center shadow-sm">
                <DollarSign className="w-3.5 h-3.5 md:w-4 md:h-4 text-blue-600 dark:text-blue-400" />
              </div>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400 mb-2 font-medium tracking-wider uppercase opacity-80">Total Value</p>
            <p className="text-lg md:text-xl font-semibold text-slate-700 dark:text-slate-200 leading-tight">$1,247</p>
          </div>
        </div>
        
        <div className="group relative text-center p-4 md:p-5 rounded-xl bg-gradient-to-br from-slate-50/8 via-white/4 to-slate-100/6 dark:from-slate-800/12 dark:via-slate-700/6 dark:to-slate-800/8 backdrop-blur-xl border border-slate-200/15 dark:border-slate-700/20 shadow-[0_4px_20px_-4px_rgba(0,0,0,0.08)] hover:shadow-[0_8px_30px_-8px_rgba(0,0,0,0.12)] transition-all duration-400 hover:scale-[1.01] hover:bg-gradient-to-br hover:from-slate-50/12 hover:via-white/6 hover:to-slate-100/10 dark:hover:from-slate-800/16 dark:hover:via-slate-700/8 dark:hover:to-slate-800/12">
          <div className="absolute inset-0 rounded-xl bg-gradient-to-br from-violet-500/3 via-transparent to-violet-400/2 opacity-0 group-hover:opacity-100 transition-opacity duration-400" />
          <div className="relative z-10">
            <div className="flex items-center justify-center mb-3">
              <div className="w-7 h-7 md:w-8 md:h-8 rounded-lg bg-gradient-to-br from-violet-500/20 to-violet-600/15 flex items-center justify-center shadow-sm">
                <Percent className="w-3.5 h-3.5 md:w-4 md:h-4 text-violet-600 dark:text-violet-400" />
              </div>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400 mb-2 font-medium tracking-wider uppercase opacity-80">P-Token Ratio</p>
            <p className="text-lg md:text-xl font-semibold text-slate-700 dark:text-slate-200 leading-tight">1.03</p>
          </div>
        </div>
        
        <div className="group relative text-center p-4 md:p-5 rounded-xl bg-gradient-to-br from-slate-50/8 via-white/4 to-slate-100/6 dark:from-slate-800/12 dark:via-slate-700/6 dark:to-slate-800/8 backdrop-blur-xl border border-slate-200/15 dark:border-slate-700/20 shadow-[0_4px_20px_-4px_rgba(0,0,0,0.08)] hover:shadow-[0_8px_30px_-8px_rgba(0,0,0,0.12)] transition-all duration-400 hover:scale-[1.01] hover:bg-gradient-to-br hover:from-slate-50/12 hover:via-white/6 hover:to-slate-100/10 dark:hover:from-slate-800/16 dark:hover:via-slate-700/8 dark:hover:to-slate-800/12">
          <div className="absolute inset-0 rounded-xl bg-gradient-to-br from-amber-500/3 via-transparent to-amber-400/2 opacity-0 group-hover:opacity-100 transition-opacity duration-400" />
          <div className="relative z-10">
            <div className="flex items-center justify-center mb-3">
              <div className="w-7 h-7 md:w-8 md:h-8 rounded-lg bg-gradient-to-br from-amber-500/20 to-amber-600/15 flex items-center justify-center shadow-sm">
                <TrendingUp className="w-3.5 h-3.5 md:w-4 md:h-4 text-amber-600 dark:text-amber-400" />
              </div>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400 mb-2 font-medium tracking-wider uppercase opacity-80">30d Earnings</p>
            <p className="text-lg md:text-xl font-semibold text-slate-700 dark:text-slate-200 leading-tight">$29</p>
          </div>
        </div>
      </div>
    </div>
  );
} 