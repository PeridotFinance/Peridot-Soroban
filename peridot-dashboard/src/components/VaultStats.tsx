'use client';

import { useState, useEffect } from 'react';
import { TrendingUp, PieChart, DollarSign, RefreshCw, Wallet, Target, ArrowUpDown, Info } from 'lucide-react';
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

  // Compact loading skeleton
  if (isLoading && refreshTrigger === 0) {
    return (
      <div className="animate-pulse space-y-4 md:space-y-6">
        {/* Compact header skeleton */}
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <div className="w-8 h-8 md:w-10 md:h-10 bg-slate-200 dark:bg-slate-700 rounded-lg md:rounded-xl"></div>
            <div className="space-y-1 md:space-y-2">
              <div className="h-4 md:h-5 bg-slate-200 dark:bg-slate-700 rounded w-28 md:w-32"></div>
              <div className="h-3 md:h-4 bg-slate-200 dark:bg-slate-700 rounded w-20 md:w-24"></div>
            </div>
          </div>
          <div className="w-8 h-8 md:w-10 md:h-10 bg-slate-200 dark:bg-slate-700 rounded-lg"></div>
        </div>
        
        {/* Compact stats skeleton */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 md:gap-4">
          {[...Array(4)].map((_, i) => (
            <div key={i} className="h-24 md:h-28 bg-slate-200 dark:bg-slate-700 rounded-lg md:rounded-xl"></div>
          ))}
        </div>
        
        {/* Compact info skeleton */}
        <div className="h-16 md:h-20 bg-slate-200 dark:bg-slate-700 rounded-lg md:rounded-xl"></div>
      </div>
    );
  }

  // Cyber-modern statistics data with neon aesthetics
  const statisticsData = [
    {
      id: 'total-deposited',
      label: 'Total Deposited',
      value: formatNumber(stats.totalDeposited),
      subtitle: 'PDOT tokens',
      icon: DollarSign,
      color: 'cyber-green',
      bgClass: 'bg-gradient-to-br from-emerald-500/10 via-green-500/5 to-teal-500/10 dark:from-emerald-400/20 dark:via-green-400/10 dark:to-teal-400/20',
      borderClass: 'border border-emerald-300/30 dark:border-emerald-400/40',
      shadowClass: 'shadow-lg shadow-emerald-500/20 dark:shadow-emerald-400/30',
      glowClass: 'hover:shadow-emerald-500/40 dark:hover:shadow-emerald-400/50',
      iconBg: 'bg-gradient-to-br from-emerald-500 to-teal-500',
      iconClass: 'text-white',
      textClass: 'text-emerald-900 dark:text-emerald-100',
      subtitleClass: 'text-emerald-700 dark:text-emerald-300',
      accentClass: 'bg-gradient-to-r from-emerald-400 to-teal-400'
    },
    {
      id: 'total-ptokens',
      label: 'Total pTokens',
      value: formatNumber(stats.totalPTokens),
      subtitle: 'Shares issued',
      icon: Target,
      color: 'liquid-glass',
      bgClass: 'bg-gradient-to-br from-white/5 via-white/2 to-white/5 dark:from-slate-700/20 dark:via-slate-600/10 dark:to-slate-700/20',
      borderClass: 'border border-white/20 dark:border-slate-600/30',
      shadowClass: 'shadow-lg shadow-black/5 dark:shadow-black/20',
      glowClass: 'hover:shadow-emerald-500/10 dark:hover:shadow-emerald-400/20',
      iconBg: 'bg-gradient-to-br from-emerald-500 to-teal-600',
      iconClass: 'text-white',
      textClass: 'text-slate-900 dark:text-slate-100',
      subtitleClass: 'text-slate-600 dark:text-slate-400',
      accentClass: 'bg-gradient-to-r from-emerald-400 to-teal-400'
    },
    {
      id: 'exchange-rate',
      label: 'Exchange Rate',
      value: `${stats.exchangeRate}:1`,
      subtitle: 'PDOT → pPDOT',
      icon: ArrowUpDown,
      color: 'liquid-glass',
      bgClass: 'bg-gradient-to-br from-white/5 via-white/2 to-white/5 dark:from-slate-700/20 dark:via-slate-600/10 dark:to-slate-700/20',
      borderClass: 'border border-white/20 dark:border-slate-600/30',
      shadowClass: 'shadow-lg shadow-black/5 dark:shadow-black/20',
      glowClass: 'hover:shadow-emerald-500/10 dark:hover:shadow-emerald-400/20',
      iconBg: 'bg-gradient-to-br from-emerald-500 to-teal-600',
      iconClass: 'text-white',
      textClass: 'text-slate-900 dark:text-slate-100',
      subtitleClass: 'text-slate-600 dark:text-slate-400',
      accentClass: 'bg-gradient-to-r from-emerald-400 to-teal-400'
    },
    {
      id: 'user-share',
      label: 'Your Share',
      value: `${stats.userShare}%`,
      subtitle: walletInfo?.isConnected ? 'of total vault' : 'Connect wallet',
      icon: Wallet,
      color: 'liquid-glass',
      bgClass: 'bg-gradient-to-br from-white/5 via-white/2 to-white/5 dark:from-slate-700/20 dark:via-slate-600/10 dark:to-slate-700/20',
      borderClass: 'border border-white/20 dark:border-slate-600/30',
      shadowClass: 'shadow-lg shadow-black/5 dark:shadow-black/20',
      glowClass: 'hover:shadow-emerald-500/10 dark:hover:shadow-emerald-400/20',
      iconBg: 'bg-gradient-to-br from-emerald-500 to-teal-600',
      iconClass: 'text-white',
      textClass: 'text-slate-900 dark:text-slate-100',
      subtitleClass: 'text-slate-600 dark:text-slate-400',
      accentClass: 'bg-gradient-to-r from-emerald-400 to-teal-400'
    }
  ];

  return (
    <div className="space-y-4 md:space-y-6">
      {/* Cyber Header with Neon Accent */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-3">
          <div className="relative w-8 h-8 md:w-10 md:h-10 rounded-lg md:rounded-xl overflow-hidden">
            {/* Animated background */}
            <div className="absolute inset-0 bg-gradient-to-br from-cyan-500 via-blue-500 to-purple-500 animate-pulse"></div>
            <div className="absolute inset-0 bg-gradient-to-br from-emerald-500 to-teal-500 opacity-80"></div>
            <div className="relative w-full h-full flex items-center justify-center shadow-xl shadow-cyan-500/30">
              <TrendingUp className="w-4 h-4 md:w-5 md:h-5 text-white drop-shadow-lg" />
            </div>
          </div>
          <div>
            <h3 className="text-lg md:text-xl font-bold bg-gradient-to-r from-slate-900 to-slate-700 dark:from-white dark:to-slate-200 bg-clip-text text-transparent">
              VAULT_ANALYTICS
            </h3>
            <p className="text-xs md:text-sm text-slate-600 dark:text-slate-400 font-mono">
              {'>'} real-time_metrics
            </p>
          </div>
        </div>
        
        <button
          onClick={handleRefresh}
          disabled={isLoading}
          className="relative group flex items-center justify-center w-8 h-8 md:w-10 md:h-10 rounded-lg bg-gradient-to-br from-slate-800 to-slate-900 dark:from-slate-700 dark:to-slate-800 border border-slate-600/50 dark:border-slate-500/50 hover:border-cyan-400/50 focus:outline-none focus:ring-2 focus:ring-cyan-400/50 disabled:opacity-50 transition-all duration-300 shadow-lg hover:shadow-cyan-400/20"
          title="Refresh analytics"
        >
          <div className="absolute inset-0 bg-gradient-to-br from-cyan-500/0 to-blue-500/0 group-hover:from-cyan-500/20 group-hover:to-blue-500/20 rounded-lg transition-all duration-300"></div>
          <RefreshCw className={`relative w-4 h-4 md:w-5 md:h-5 text-slate-300 group-hover:text-cyan-300 transition-colors duration-300 ${isLoading ? 'animate-spin' : ''}`} />
        </button>
      </div>

       {/* Cyber Error State */}
       {error && (
         <div className="relative flex items-center space-x-3 p-3 md:p-4 bg-gradient-to-r from-red-500/10 via-red-600/5 to-orange-500/10 dark:from-red-400/20 dark:via-red-500/10 dark:to-orange-400/20 border border-red-400/40 dark:border-red-400/50 rounded-lg md:rounded-xl shadow-lg shadow-red-500/20 backdrop-blur-sm">
           <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-red-400 to-orange-400"></div>
           <div className="w-6 h-6 md:w-8 md:h-8 rounded-lg bg-gradient-to-br from-red-500 to-orange-500 flex items-center justify-center shadow-lg">
             <span className="text-white font-bold text-xs md:text-sm">!</span>
           </div>
           <div className="flex-1">
             <p className="text-xs md:text-sm text-red-700 dark:text-red-300 font-mono">ERROR_CODE: 0x{Math.random().toString(16).substr(2, 6).toUpperCase()}</p>
             <p className="text-xs text-red-600 dark:text-red-400 font-mono opacity-80">{error}</p>
           </div>
         </div>
       )}

             {/* Liquid Glass Stats Grid */}
       <div className="grid grid-cols-2 md:grid-cols-4 gap-3 md:gap-4">
         {statisticsData.map((stat) => {
           const IconComponent = stat.icon;
           return (
             <div
               key={stat.id}
               className={`
                 relative group p-3 md:p-4 rounded-xl md:rounded-2xl border backdrop-blur-2xl
                 transition-all duration-300 hover:scale-[1.02] cursor-pointer
                 ${stat.bgClass} ${stat.borderClass} ${stat.shadowClass} hover:${stat.glowClass}
                 hover:border-emerald-400/30 dark:hover:border-emerald-400/40
               `}
             >
               {/* Neon accent line */}
               <div className={`absolute top-0 left-0 right-0 h-0.5 ${stat.accentClass} opacity-60 group-hover:opacity-100 transition-opacity duration-300`}></div>
               
               {/* Cyber Icon Header */}
               <div className="flex items-center justify-between mb-2">
                 <div className={`relative w-6 h-6 md:w-8 md:h-8 rounded-md md:rounded-lg ${stat.iconBg} flex items-center justify-center shadow-lg overflow-hidden`}>
                   <div className="absolute inset-0 bg-gradient-to-br from-white/20 to-transparent"></div>
                   <IconComponent className={`relative w-3 h-3 md:w-4 md:h-4 ${stat.iconClass} drop-shadow-sm`} />
                 </div>
                 {/* Pulsing indicator */}
                 <div className={`relative w-1.5 h-1.5 md:w-2 md:h-2 rounded-full ${stat.accentClass.replace('bg-gradient-to-r', 'bg-gradient-to-br')}`}>
                   <div className={`absolute inset-0 rounded-full ${stat.accentClass.replace('bg-gradient-to-r', 'bg-gradient-to-br')} animate-ping opacity-75`}></div>
                 </div>
               </div>
               
               {/* Cyber Label */}
               <p className={`text-xs md:text-sm font-semibold mb-1 ${stat.textClass} leading-tight font-mono uppercase tracking-wide`}>
                 {stat.label.replace(' ', '_')}
               </p>
               
               {/* Glowing Value */}
               <p className={`text-lg md:text-xl lg:text-2xl font-bold ${stat.textClass} leading-tight font-mono tracking-tight drop-shadow-sm`}>
                 {stat.value}
               </p>
               
               {/* Cyber Subtitle */}
               <p className={`text-xs ${stat.subtitleClass} mt-0.5 md:mt-1 font-mono opacity-80`}>
                 {stat.subtitle}
               </p>

               {/* Hover glow effect */}
               <div className={`absolute inset-0 rounded-lg md:rounded-xl ${stat.accentClass.replace('bg-gradient-to-r', 'bg-gradient-to-br')} opacity-0 group-hover:opacity-5 transition-opacity duration-300 pointer-events-none`}></div>
             </div>
           );
         })}
       </div>

       {/* Peridot Liquid Glass Info Panel */}
       <div className="relative overflow-hidden rounded-xl md:rounded-2xl bg-gradient-to-br from-white/5 via-white/2 to-white/5 dark:from-slate-700/20 dark:via-slate-600/10 dark:to-slate-700/20 border border-white/20 dark:border-slate-600/30 shadow-xl shadow-black/5 dark:shadow-black/20 backdrop-blur-2xl">
         <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-emerald-400/60 via-teal-400/60 to-cyan-400/60"></div>
         <div className="absolute inset-0 bg-gradient-to-br from-emerald-500/3 via-transparent to-teal-500/3"></div>
         <div className="relative p-4 md:p-6">
           <div className="flex items-start space-x-4">
             <div className="w-8 h-8 md:w-10 md:h-10 rounded-xl bg-gradient-to-br from-emerald-500 to-teal-600 flex items-center justify-center shadow-lg flex-shrink-0">
               <div className="absolute inset-0 bg-gradient-to-br from-white/20 to-transparent rounded-xl"></div>
               <Info className="relative w-4 h-4 md:w-5 md:h-5 text-white drop-shadow-sm" />
             </div>
             <div className="flex-1">
               <h4 className="text-sm md:text-base font-bold text-slate-900 dark:text-slate-100 mb-2 md:mb-3 font-mono uppercase tracking-wide">
                 VAULT_PROTOCOL.INFO
               </h4>
               <div className="space-y-1.5 md:space-y-2">
                 <p className="text-sm md:text-base text-slate-700 dark:text-slate-300 leading-relaxed">
                   <span className="text-emerald-600 dark:text-emerald-400 font-mono">{'> '}</span>
                   Deposit PDOT tokens → receive pTokens (vault shares)
                 </p>
                 <p className="text-sm md:text-base text-slate-700 dark:text-slate-300 leading-relaxed">
                   <span className="text-emerald-600 dark:text-emerald-400 font-mono">{'> '}</span>
                   1:1 exchange ratio maintained • instant redemption
                 </p>
               </div>
               <div className="flex items-center space-x-3 mt-3 md:mt-4">
                 <div className="relative">
                   <div className="w-2.5 h-2.5 bg-emerald-400 rounded-full"></div>
                   <div className="absolute inset-0 w-2.5 h-2.5 bg-emerald-400 rounded-full animate-ping opacity-75"></div>
                 </div>
                 <span className="text-xs md:text-sm text-emerald-600 dark:text-emerald-400 font-mono font-semibold">
                   PROTOCOL_ACTIVE
                 </span>
               </div>
             </div>
           </div>
         </div>
       </div>
    </div>
  );
} 