'use client';

import { useState, useEffect } from 'react';
import { TrendingUp, PieChart, DollarSign, RefreshCw } from 'lucide-react';
import { getVaultStats, formatNumber, VaultStats as VaultStatsType } from '@/utils/stellar';

interface VaultStatsProps {
  walletInfo: any;
  refreshTrigger: number;
}

export default function VaultStats({ walletInfo, refreshTrigger }: VaultStatsProps) {
  const [stats, setStats] = useState<VaultStatsType>({
    totalDeposited: '0',
    totalPTokens: '0',
    exchangeRate: '1',
    userShare: '0'
  });
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchStats();
  }, [refreshTrigger]);

  const fetchStats = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const vaultStats = await getVaultStats();
      
      // Calculate user share if wallet is connected
      let userShare = '0';
      if (walletInfo?.pTokenBalance && vaultStats.totalPTokens !== '0') {
        const userPTokens = parseFloat(walletInfo.pTokenBalance);
        const totalPTokens = parseFloat(vaultStats.totalPTokens);
        userShare = ((userPTokens / totalPTokens) * 100).toFixed(2);
      }

      setStats({
        ...vaultStats,
        userShare
      });
    } catch (err) {
      setError(`Failed to load vault stats: ${err}`);
      console.error('Error fetching vault stats:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleRefresh = () => {
    fetchStats();
  };

  if (isLoading && refreshTrigger === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="animate-pulse">
          <div className="flex items-center justify-between mb-4">
            <div className="h-6 bg-gray-200 rounded w-32"></div>
            <div className="h-8 w-8 bg-gray-200 rounded-full"></div>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="h-20 bg-gray-200 rounded"></div>
            <div className="h-20 bg-gray-200 rounded"></div>
            <div className="h-20 bg-gray-200 rounded"></div>
            <div className="h-20 bg-gray-200 rounded"></div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center space-x-3">
          <div className="w-10 h-10 bg-green-100 rounded-full flex items-center justify-center">
            <TrendingUp className="w-5 h-5 text-green-600" />
          </div>
          <div>
            <h3 className="font-semibold text-gray-900">Vault Statistics</h3>
            <p className="text-sm text-gray-600">Real-time vault performance</p>
          </div>
        </div>
        <button
          onClick={handleRefresh}
          disabled={isLoading}
          className="p-2 text-gray-400 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-green-500 rounded-full transition-colors"
          title="Refresh stats"
        >
          <RefreshCw className={`w-5 h-5 ${isLoading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md">
          <p className="text-sm text-red-700">{error}</p>
        </div>
      )}

      <div className="grid grid-cols-2 gap-4">
        {/* Total Deposited */}
        <div className="p-4 bg-green-50 rounded-lg border border-green-100">
          <div className="flex items-center space-x-2 mb-2">
            <DollarSign className="w-4 h-4 text-green-600" />
            <span className="text-sm font-medium text-green-700">Total Deposited</span>
          </div>
          <p className="text-2xl font-bold text-green-800">
            {formatNumber(stats.totalDeposited)}
          </p>
          <p className="text-xs text-green-600 mt-1">PDOT tokens</p>
        </div>

        {/* Total pTokens */}
        <div className="p-4 bg-blue-50 rounded-lg border border-blue-100">
          <div className="flex items-center space-x-2 mb-2">
            <TrendingUp className="w-4 h-4 text-blue-600" />
            <span className="text-sm font-medium text-blue-700">Total pTokens</span>
          </div>
          <p className="text-2xl font-bold text-blue-800">
            {formatNumber(stats.totalPTokens)}
          </p>
          <p className="text-xs text-blue-600 mt-1">pTokens issued</p>
        </div>

        {/* Exchange Rate */}
        <div className="p-4 bg-purple-50 rounded-lg border border-purple-100">
          <div className="flex items-center space-x-2 mb-2">
            <RefreshCw className="w-4 h-4 text-purple-600" />
            <span className="text-sm font-medium text-purple-700">Exchange Rate</span>
          </div>
          <p className="text-2xl font-bold text-purple-800">
            {stats.exchangeRate}:1
          </p>
          <p className="text-xs text-purple-600 mt-1">PDOT : pPDOT</p>
        </div>

        {/* User Share */}
        <div className="p-4 bg-orange-50 rounded-lg border border-orange-100">
          <div className="flex items-center space-x-2 mb-2">
            <PieChart className="w-4 h-4 text-orange-600" />
            <span className="text-sm font-medium text-orange-700">Your Share</span>
          </div>
          <p className="text-2xl font-bold text-orange-800">
            {stats.userShare}%
          </p>
          <p className="text-xs text-orange-600 mt-1">
            {walletInfo?.isConnected ? 'of total vault' : 'Connect wallet'}
          </p>
        </div>
      </div>


      {/* Vault Info */}
      <div className="mt-4 p-3 bg-gray-50 rounded-lg">
        <h4 className="text-sm font-medium text-gray-900 mb-1">
          How the Vault Works
        </h4>
        <p className="text-xs text-gray-600">
          Deposit PDOT tokens to receive pTokens at a 1:1 ratio. Your pTokens represent 
          your share of the vault and can be redeemed for the underlying PDOT tokens at any time.
        </p>
      </div>
    </div>
  );
} 