'use client';

import { useState, useRef } from 'react';
import { createPortal } from 'react-dom';
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

  /* New floating and elegant animations */
  @keyframes float-in {
    from {
      opacity: 0;
      transform: translateY(-20px) scale(0.95);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @keyframes float-up {
    from {
      opacity: 0;
      transform: translateY(20px) scale(0.95);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @keyframes scale-in {
    from {
      opacity: 0;
      transform: scale(0.8);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .animate-float-in {
    animation: float-in 0.5s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  .animate-float-up {
    animation: float-up 0.6s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  .animate-scale-in {
    animation: scale-in 0.4s cubic-bezier(0.16, 1, 0.3, 1) forwards;
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

  /* Input text visibility - hide when not focused to prevent overlap */
  .inputbox input:not(:focus):not(:valid) {
    color: transparent !important;
  }
  
  .inputbox input:focus,
  .inputbox input:valid {
    color: #23242a !important;
  }
  
  [data-theme="dark"] .inputbox input:focus,
  [data-theme="dark"] .inputbox input:valid {
    color: #ffffff !important;
  }
`;

interface ConnectWalletProps {
  walletInfo: WalletInfo | null;
  onWalletChange: (info: WalletInfo | null) => void;
  mode?: 'lending' | 'faucet';
  selectedAsset?: 'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL';
  onSelectedAssetChange?: (asset: 'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL') => void;
}

export default function ConnectWallet({ 
  walletInfo, 
  onWalletChange, 
  mode = 'faucet', 
  selectedAsset: propSelectedAsset = 'PDOT',
  onSelectedAssetChange 
}: ConnectWalletProps) {
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

  // Token prices - centralized pricing data
  const tokenPrices = {
    PDOT: 0.85,
    XLM: 0.12,
    USDC: 1.00,
    ETH: 2517.48,
    SOL: 147.64
  };

  // Lending/Borrowing functionality - use prop or local state
  const [localSelectedAsset, setLocalSelectedAsset] = useState<'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL'>('PDOT');
  const selectedAsset = mode === 'lending' && propSelectedAsset ? propSelectedAsset : localSelectedAsset;
  
  const setSelectedAsset = (asset: 'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL') => {
    if (mode === 'lending' && onSelectedAssetChange) {
      onSelectedAssetChange(asset);
    } else {
      setLocalSelectedAsset(asset);
    }
  };
  const [lendingMode, setLendingMode] = useState<'lend' | 'borrow'>('lend');
  const [amount, setAmount] = useState('');
  const [isProcessing, setIsProcessing] = useState(false);

  // Mock borrowed amounts - stored in localStorage
  const [borrowedAmounts, setBorrowedAmounts] = useState<Record<string, number>>(() => {
    if (typeof window !== 'undefined') {
      const stored = localStorage.getItem(`borrowed_amounts_${walletInfo?.address}`);
      return stored ? JSON.parse(stored) : { PDOT: 0, XLM: 0, USDC: 0, ETH: 0, SOL: 0 };
    }
    return { PDOT: 0, XLM: 0, USDC: 0, ETH: 0, SOL: 0 };
  });

  // Save borrowed amounts to localStorage when they change
  const updateBorrowedAmounts = (newAmounts: Record<string, number>) => {
    setBorrowedAmounts(newAmounts);
    if (typeof window !== 'undefined' && walletInfo?.address) {
      localStorage.setItem(`borrowed_amounts_${walletInfo.address}`, JSON.stringify(newAmounts));
    }
  };
  
  // Earnings popup functionality
  const [isInputFocused, setIsInputFocused] = useState(false);
  const [showEarningsPopup, setShowEarningsPopup] = useState(false);
  const inputContainerRef = useRef<HTMLDivElement>(null);
  
  // Modal functionality
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modalAsset, setModalAsset] = useState<'PDOT' | 'XLM' | 'USDC' | 'ETH' | 'SOL'>('PDOT');
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
  // pTokens represent supplied PDOT (1:1) and are used as collateral, not as a separate asset
  const suppliedPDOT = walletInfo?.pTokenBalance || '0'; // pTokens = supplied PDOT
  const availablePDOT = walletInfo?.testTokenBalance || '0'; // Available PDOT to supply
  
  // Calculate collateral value and borrowing power
  const collateralValueUSD = parseFloat(suppliedPDOT) * tokenPrices.PDOT;
  const maxBorrowingPowerUSD = collateralValueUSD * 0.8; // 80% of collateral
  const totalBorrowedUSD = Object.keys(borrowedAmounts).reduce((total, asset) => {
    return total + (borrowedAmounts[asset] * tokenPrices[asset as keyof typeof tokenPrices]);
  }, 0);
  const remainingBorrowingPowerUSD = Math.max(0, maxBorrowingPowerUSD - totalBorrowedUSD);
  
  const assetData = {
    PDOT: { 
      balance: availablePDOT, 
      lendApy: '12.5', 
      borrowApy: '15.8', 
      borrowed: borrowedAmounts.PDOT.toString(), 
      price: tokenPrices.PDOT.toString() 
    },
    XLM: { 
      balance: borrowedAmounts.XLM.toString(), 
      lendApy: '8.2', 
      borrowApy: '11.7', 
      borrowed: borrowedAmounts.XLM.toString(), 
      price: tokenPrices.XLM.toString() 
    },
    USDC: { 
      balance: borrowedAmounts.USDC.toString(), 
      lendApy: '5.5', 
      borrowApy: '8.2', 
      borrowed: borrowedAmounts.USDC.toString(), 
      price: tokenPrices.USDC.toString() 
    },
    ETH: { 
      balance: borrowedAmounts.ETH.toString(), 
      lendApy: '4.8', 
      borrowApy: '7.3', 
      borrowed: borrowedAmounts.ETH.toString(), 
      price: tokenPrices.ETH.toString() 
    },
    SOL: { 
      balance: borrowedAmounts.SOL.toString(), 
      lendApy: '6.3', 
      borrowApy: '9.1', 
      borrowed: borrowedAmounts.SOL.toString(), 
      price: tokenPrices.SOL.toString() 
    }
  };

  const handleLendingAction = async () => {
    if (!amount || !walletInfo?.address) return;
    
    // Clear previous error messages
    setError(null);
    setMintingStatus('idle');
    setMintError(null);
    
    // Validate amount
    const amountNum = parseFloat(amount);
    if (amountNum <= 0) {
      setError('Amount must be greater than 0');
      return;
    }
    
    // Check if user has sufficient balance or collateral
    if (lendingMode === 'lend' && selectedAsset === 'PDOT') {
      const availableBalance = parseFloat(walletInfo?.testTokenBalance || '0');
      if (amountNum > availableBalance) {
        setError(`Insufficient PDOT balance. Available: ${availableBalance} PDOT`);
        return;
      }
    } else if (lendingMode === 'borrow') {
      // Check if user has sufficient collateral to borrow
      if (parseFloat(suppliedPDOT) === 0) {
        setError('You need to supply collateral before borrowing');
        return;
      }
      
      const assetPrice = parseFloat(assetData[selectedAsset].price);
      const borrowValueUSD = amountNum * assetPrice;
      const newTotalBorrowedUSD = totalBorrowedUSD + borrowValueUSD;
      
      if (newTotalBorrowedUSD > maxBorrowingPowerUSD) {
        const maxBorrowableAmount = remainingBorrowingPowerUSD / assetPrice;
        setError(`Exceeds borrowing limit. Max available: ${maxBorrowableAmount.toFixed(4)} ${selectedAsset} (${remainingBorrowingPowerUSD.toFixed(2)} USD)`);
        return;
      }
    }
    
    setIsProcessing(true);
    
    try {
      if (selectedAsset === 'PDOT') {
        // PDOT operations use smart contracts
        if (lendingMode === 'lend') {
          // Deposit PDOT to vault and receive pTokens
          const { depositToVault } = await import('@/utils/stellar');
          
          const result = await depositToVault(
            walletInfo.address, 
            amount,
            (status: string) => {
              console.log('Deposit status:', status);
            }
          );
          
          if (result.success) {
            // Transaction successful - refresh balances with delay
            setAmount('');
            setMintingStatus('success');
            
            // Add delay to ensure ledger state has updated, then refresh multiple times
            setTimeout(async () => {
              const { getBalances } = await import('@/utils/stellar');
              const updatedBalances = await getBalances(walletInfo.address);
              onWalletChange(updatedBalances);
            }, 1500);
            
            // Additional refresh after 3 seconds to ensure all updates are captured
            setTimeout(async () => {
              const { getBalances } = await import('@/utils/stellar');
              const updatedBalances = await getBalances(walletInfo.address);
              onWalletChange(updatedBalances);
            }, 3000);
            
            setTimeout(() => setMintingStatus('idle'), 8000);
          } else {
            setError(result.error || 'Deposit failed');
            setMintingStatus('error');
            setMintError(result.error || 'Deposit failed');
          }
        } else {
          // PDOT borrowing - TODO: Implement smart contract borrowing
          setError('PDOT borrowing via smart contract not yet implemented');
        }
      } else {
        // Other tokens (XLM, USDC, ETH, SOL) use mock localStorage
        if (lendingMode === 'lend') {
          // Mock lending for other assets - not implemented yet
          setError('Lending ' + selectedAsset + ' is simulated and not yet implemented');
        } else {
          // Mock borrowing - update local state
          const newBorrowedAmounts = {
            ...borrowedAmounts,
            [selectedAsset]: borrowedAmounts[selectedAsset] + amountNum
          };
          
          updateBorrowedAmounts(newBorrowedAmounts);
          setAmount('');
          setMintingStatus('success');
          setTimeout(() => setMintingStatus('idle'), 8000);
        }
      }
    } catch (err: any) {
      console.error('Lending action error:', err);
      setError(`Transaction failed: ${err.message || err}`);
      setMintingStatus('error');
      setMintError(`Transaction failed: ${err.message || err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleWithdraw = async () => {
    if (!modalAmount || !walletInfo?.address) return;
    
    // Clear previous error messages
    setError(null);
    setMintingStatus('idle');
    setMintError(null);
    
    // Validate amount
    const amountNum = parseFloat(modalAmount);
    if (amountNum <= 0) {
      setError('Amount must be greater than 0');
      return;
    }
    
    // Check if user has sufficient pTokens to withdraw
    const availablePTokens = parseFloat(walletInfo?.pTokenBalance || '0');
    if (amountNum > availablePTokens) {
      setError(`Insufficient pTokens. Available: ${availablePTokens} pTokens`);
      return;
    }
    
    setIsModalProcessing(true);
    
    try {
      // Withdraw using pTokens (1:1 with PDOT)
      const { withdrawFromVault } = await import('@/utils/stellar');
      
      const result = await withdrawFromVault(
        walletInfo.address, 
        modalAmount, // pToken amount to withdraw
        (status: string) => {
          console.log('Withdraw status:', status);
        }
      );
      
      if (result.success) {
        // Transaction successful - refresh balances with delay
        setModalAmount('');
        closeModal();
        setMintingStatus('success');
        
        // Add delay to ensure ledger state has updated, then refresh multiple times
        setTimeout(async () => {
          const { getBalances } = await import('@/utils/stellar');
          const updatedBalances = await getBalances(walletInfo.address);
          onWalletChange(updatedBalances);
        }, 1500);
        
        // Additional refresh after 3 seconds to ensure all updates are captured
        setTimeout(async () => {
          const { getBalances } = await import('@/utils/stellar');
          const updatedBalances = await getBalances(walletInfo.address);
          onWalletChange(updatedBalances);
        }, 3000);
        
        setTimeout(() => setMintingStatus('idle'), 8000);
      } else {
        setError(result.error || 'Withdrawal failed');
        setMintingStatus('error');
        setMintError(result.error || 'Withdrawal failed');
      }
    } catch (err: any) {
      console.error('Withdrawal error:', err);
      setError(`Withdrawal failed: ${err.message || err}`);
      setMintingStatus('error');
      setMintError(`Withdrawal failed: ${err.message || err}`);
    } finally {
      setIsModalProcessing(false);
    }
  };

  // Earnings/Interest calculation and popup handlers
  const calculateProjectedValue = (inputAmount: string) => {
    if (!inputAmount || isNaN(parseFloat(inputAmount))) return 0;
    const numAmount = parseFloat(inputAmount);
    const apy = parseFloat(lendingMode === 'borrow' ? assetData[selectedAsset].borrowApy : assetData[selectedAsset].lendApy);
    const price = parseFloat(assetData[selectedAsset].price);
    const monthlyRate = apy / 100 / 12; // Convert APY to monthly rate
    const value = numAmount * monthlyRate * price; // Value in USD
    return value;
  };

  const handleInputFocus = () => {
    setIsInputFocused(true);
    setShowEarningsPopup(true);
  };

  const handleInputBlur = () => {
    setIsInputFocused(false);
    // Delay hiding popup to allow for interaction
    setTimeout(() => {
      if (!isInputFocused) {
        setShowEarningsPopup(false);
      }
    }, 300);
  };

  const handlePopupMouseEnter = () => {
    setShowEarningsPopup(true);
  };

  const handlePopupMouseLeave = () => {
    if (!isInputFocused) {
      setShowEarningsPopup(false);
    }
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
    
    if (modalAction === 'withdraw') {
      await handleWithdraw();
    } else if (modalAction === 'repay') {
      await handleRepay();
    } else {
      // For other modal actions
      setIsModalProcessing(true);
      
      // Simulate transaction for other actions
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      setIsModalProcessing(false);
      setModalAmount('');
      closeModal();
    }
  };

  const handleRepay = async () => {
    if (!modalAmount || !walletInfo?.address) return;
    
    // Clear previous error messages
    setError(null);
    setMintingStatus('idle');
    setMintError(null);
    
    // Validate amount
    const amountNum = parseFloat(modalAmount);
    if (amountNum <= 0) {
      setError('Amount must be greater than 0');
      return;
    }
    
    // Check if user has sufficient borrowed amount to repay
    const currentBorrowed = borrowedAmounts[modalAsset];
    if (amountNum > currentBorrowed) {
      setError(`Cannot repay more than borrowed. Borrowed: ${currentBorrowed} ${modalAsset}`);
      return;
    }
    
    setIsModalProcessing(true);
    
    try {
      // Mock repayment - reduce borrowed amount
      const newBorrowedAmounts = {
        ...borrowedAmounts,
        [modalAsset]: Math.max(0, borrowedAmounts[modalAsset] - amountNum)
      };
      
      updateBorrowedAmounts(newBorrowedAmounts);
      setModalAmount('');
      closeModal();
      setMintingStatus('success');
      setTimeout(() => setMintingStatus('idle'), 8000);
    } catch (err: any) {
      console.error('Repayment error:', err);
      setError(`Repayment failed: ${err.message || err}`);
      setMintingStatus('error');
      setMintError(`Repayment failed: ${err.message || err}`);
    } finally {
      setIsModalProcessing(false);
    }
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
          className="fixed inset-0 z-50 flex items-end sm:items-center justify-center p-3 sm:p-4 bg-black/70 backdrop-blur-lg animate-fade-in"
          onClick={closeModal}
        >
          {/* Enhanced Mobile-First Elegant Modal */}
          <div 
            className="w-full max-w-sm sm:max-w-md relative overflow-hidden bg-gradient-to-br from-white/98 via-slate-50/95 to-white/98 dark:from-slate-800/98 dark:via-slate-700/95 dark:to-slate-800/98 border border-white/60 dark:border-slate-600/50 rounded-3xl sm:rounded-4xl shadow-2xl backdrop-blur-3xl animate-float-up max-h-[85vh] sm:max-h-[90vh] overflow-y-auto"
            onClick={(e) => e.stopPropagation()}
            style={{
              boxShadow: '0 25px 50px -12px rgba(0, 0, 0, 0.25), 0 8px 32px -8px rgba(0, 0, 0, 0.15), inset 0 1px 0 rgba(255, 255, 255, 0.1)'
            }}
          >
            {/* Peridot Liquid Glass Top Highlight */}
            <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-emerald-400/50 to-transparent"></div>
            <div className="absolute top-1 left-1/2 transform -translate-x-1/2 w-20 h-1 bg-gradient-to-r from-transparent via-white/60 to-transparent rounded-full blur-sm"></div>
            
            {/* Glass Reflection Effect */}
            <div className="absolute top-0 left-0 right-0 h-32 bg-gradient-to-b from-white/10 via-white/5 to-transparent pointer-events-none"></div>
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

              {/* Market Overview */}
              <div className="grid grid-cols-2 gap-3 mb-6">
                <div className="p-4 bg-gradient-to-br from-emerald-500/8 via-emerald-400/4 to-teal-500/8 dark:from-emerald-400/12 dark:via-emerald-300/6 dark:to-teal-400/12 border border-emerald-400/20 dark:border-emerald-300/25 rounded-2xl backdrop-blur-3xl shadow-lg shadow-emerald-500/10 hover:shadow-emerald-500/20 transition-all duration-300 hover:border-emerald-300/40">
                  <div className="flex items-center space-x-2 mb-2">
                    <div className="w-2 h-2 rounded-full bg-emerald-400 shadow-sm shadow-emerald-400/50"></div>
                    <p className="text-xs text-emerald-700 dark:text-emerald-300 font-medium">Total Supplied</p>
                  </div>
                  <p className="text-xl font-bold text-emerald-800 dark:text-emerald-200 mb-1">
                    {modalAsset === 'XLM' ? '20' : '15.7'}
                  </p>
                  <p className="text-xs text-emerald-600 dark:text-emerald-400 font-semibold opacity-90">{modalAsset}</p>
                </div>
                
                <div className="p-4 bg-gradient-to-br from-orange-500/8 via-amber-500/4 to-red-500/8 dark:from-orange-400/12 dark:via-amber-400/6 dark:to-red-400/12 border border-orange-400/20 dark:border-orange-300/25 rounded-2xl backdrop-blur-3xl shadow-lg shadow-orange-500/10 hover:shadow-orange-500/20 transition-all duration-300 hover:border-orange-300/40">
                  <div className="flex items-center space-x-2 mb-2">
                    <div className="w-2 h-2 rounded-full bg-orange-400 shadow-sm shadow-orange-400/50"></div>
                    <p className="text-xs text-orange-700 dark:text-orange-300 font-medium">Total Borrowed</p>
                  </div>
                  <p className="text-xl font-bold text-orange-800 dark:text-orange-200 mb-1">
                    {modalAsset === 'XLM' ? '10.43' : '8.2'}
                  </p>
                  <p className="text-xs text-orange-600 dark:text-orange-400 font-semibold opacity-90">{modalAsset}</p>
                </div>
                
                <div className="p-4 bg-gradient-to-br from-purple-500/8 via-indigo-500/4 to-blue-500/8 dark:from-purple-400/12 dark:via-indigo-400/6 dark:to-blue-400/12 border border-purple-400/20 dark:border-purple-300/25 rounded-2xl backdrop-blur-3xl shadow-lg shadow-purple-500/10 hover:shadow-purple-500/20 transition-all duration-300 hover:border-purple-300/40">
                  <div className="flex items-center space-x-2 mb-2">
                    <div className="w-2 h-2 rounded-full bg-purple-400 shadow-sm shadow-purple-400/50"></div>
                    <p className="text-xs text-purple-700 dark:text-purple-300 font-medium">Reserved</p>
                  </div>
                  <p className="text-xl font-bold text-purple-800 dark:text-purple-200 mb-1">
                    {modalAsset === 'XLM' ? '2.04' : '1.85'}
                  </p>
                  <p className="text-xs text-purple-600 dark:text-purple-400 font-semibold opacity-90">{modalAsset}</p>
                </div>
                
                <div className="p-4 bg-gradient-to-br from-cyan-500/8 via-blue-500/4 to-indigo-500/8 dark:from-cyan-400/12 dark:via-blue-400/6 dark:to-indigo-400/12 border border-cyan-400/20 dark:border-cyan-300/25 rounded-2xl backdrop-blur-3xl shadow-lg shadow-cyan-500/10 hover:shadow-cyan-500/20 transition-all duration-300 hover:border-cyan-300/40">
                  <div className="flex items-center space-x-2 mb-2">
                    <div className="w-2 h-2 rounded-full bg-cyan-400 shadow-sm shadow-cyan-400/50"></div>
                    <p className="text-xs text-cyan-700 dark:text-cyan-300 font-medium">Available to Borrow</p>
                  </div>
                  <p className="text-xl font-bold text-cyan-800 dark:text-cyan-200 mb-1">
                    {modalAsset === 'XLM' ? '7' : '5.9'}
                  </p>
                  <p className="text-xs text-cyan-600 dark:text-cyan-400 font-semibold opacity-90">{modalAsset}</p>
                </div>
              </div>

              {/* Your Position & Market Info */}
              <div className="grid grid-cols-2 gap-3 mb-6">
                <div className="p-4 bg-gradient-to-br from-slate-50/60 via-white/40 to-slate-100/60 dark:from-slate-700/40 dark:via-slate-600/20 dark:to-slate-700/40 border border-slate-200/50 dark:border-slate-600/30 rounded-2xl backdrop-blur-3xl shadow-lg shadow-slate-500/5 hover:shadow-slate-500/15 transition-all duration-300 hover:border-emerald-300/30">
                  <div className="flex items-center space-x-2 mb-2">
                    <div className="w-2 h-2 rounded-full bg-emerald-400 shadow-sm shadow-emerald-400/50"></div>
                    <p className="text-xs text-slate-600 dark:text-slate-400 font-medium">
                      {modalAction === 'withdraw' ? 'Supplied Balance' : 'Your Balance'}
                    </p>
                  </div>
                  <p className="text-lg font-bold text-slate-900 dark:text-white mb-1">
                    {modalAsset === 'PDOT' && modalAction === 'withdraw' 
                      ? formatNumber(suppliedPDOT) 
                      : assetData[modalAsset].balance}
                  </p>
                  <p className="text-xs text-emerald-500 font-semibold">
                    {modalAsset === 'PDOT' && modalAction === 'withdraw' ? 'pTokens' : modalAsset}
                  </p>
                </div>
                
                <div className="p-4 bg-gradient-to-br from-emerald-50/60 via-emerald-50/40 to-teal-50/60 dark:from-emerald-900/40 dark:via-emerald-800/20 dark:to-teal-900/40 border border-emerald-200/50 dark:border-emerald-600/30 rounded-2xl backdrop-blur-3xl shadow-lg shadow-emerald-500/5 hover:shadow-emerald-500/15 transition-all duration-300 hover:border-emerald-300/40">
                  <div className="flex items-center space-x-2 mb-2">
                    <div className="w-2 h-2 rounded-full bg-emerald-400 shadow-sm shadow-emerald-400/50"></div>
                    <p className="text-xs text-emerald-700 dark:text-emerald-300 font-medium">Current APY</p>
                  </div>
                  <p className="text-lg font-bold text-emerald-700 dark:text-emerald-300 mb-1">{lendingMode === 'borrow' ? assetData[modalAsset].borrowApy : assetData[modalAsset].lendApy}%</p>
                  <p className="text-xs text-emerald-600 dark:text-emerald-400 font-semibold opacity-90">Annual</p>
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
                        onClick={() => {
                          // For withdrawal, use pToken balance (supplied amount)
                          const maxAmount = modalAsset === 'PDOT' && modalAction === 'withdraw' 
                            ? suppliedPDOT 
                            : assetData[modalAsset].balance;
                          setModalAmount(maxAmount);
                        }}
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
                        {modalAsset === 'PDOT' 
                          ? `${formatNumber(suppliedPDOT)} PDOT` 
                          : `${assetData[modalAsset].balance} ${modalAsset}`}
                      </span>
                    </div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-slate-600 dark:text-slate-400">Earnings (30d)</span>
                      <span className="font-semibold text-emerald-600 dark:text-emerald-400">
                        +{modalAsset === 'PDOT' 
                          ? (parseFloat(suppliedPDOT) * 0.025).toFixed(2) 
                          : (parseFloat(assetData[modalAsset].balance) * 0.025).toFixed(2)} {modalAsset}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-slate-600 dark:text-slate-400">Next Interest</span>
                      <span className="font-semibold text-blue-600 dark:text-blue-400">
                        +{modalAsset === 'PDOT' 
                          ? (parseFloat(suppliedPDOT) * 0.001).toFixed(4) 
                          : (parseFloat(assetData[modalAsset].balance) * 0.001).toFixed(4)} {modalAsset}
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
        {/* Success Message - Modal-like Confirmation */}
        {mintingStatus === 'success' && (
          <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-md animate-fade-in">
            <div className="w-full max-w-sm relative overflow-hidden rounded-3xl bg-gradient-to-br from-white/98 via-emerald-50/95 to-white/98 dark:from-slate-800/98 dark:via-emerald-900/95 dark:to-slate-800/98 border border-emerald-200/60 dark:border-emerald-600/40 shadow-2xl backdrop-blur-3xl animate-float-up"
                 style={{ boxShadow: '0 25px 50px -12px rgba(16, 185, 129, 0.25)' }}>
              {/* Liquid Glass Top Border */}
              <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-emerald-400/60 to-transparent"></div>
              
              {/* Floating Highlight */}
              <div className="absolute top-1 left-1/2 transform -translate-x-1/2 w-20 h-1 bg-gradient-to-r from-transparent via-white/50 to-transparent rounded-full blur-sm"></div>
              
              <div className="relative p-6 text-center">
                {/* Animated Success Icon */}
                <div className="relative mx-auto mb-4 w-16 h-16">
                  <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-emerald-500/90 via-teal-500/85 to-emerald-600/90 backdrop-blur-xl flex items-center justify-center shadow-lg shadow-emerald-500/30 animate-scale-in">
                    <CheckCircle className="w-8 h-8 text-white animate-fade-in animate-delay-200" />
                  </div>
                  <div className="absolute inset-0 rounded-2xl bg-gradient-to-br from-emerald-400/20 to-teal-400/20 animate-pulse"></div>
                </div>
                
                {/* Message Content */}
                <div className="mb-6">
                  <h3 className="text-lg font-bold text-emerald-700 dark:text-emerald-300 mb-2 animate-slide-in-right animate-delay-100">
                    Transaction Completed
                  </h3>
                  <p className="text-sm text-emerald-600/90 dark:text-emerald-400/90 animate-slide-in-right animate-delay-200">
                    Vault operation completed successfully
                  </p>
                </div>
                
                {/* Close Button */}
                <button
                  onClick={() => setMintingStatus('idle')}
                  className="w-full px-6 py-3 rounded-xl bg-gradient-to-r from-emerald-500/90 to-teal-500/90 hover:from-emerald-500 hover:to-teal-500 text-white font-semibold transition-all duration-200 backdrop-blur-sm shadow-lg"
                >
                  Continue
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Error Message - Modal-like Alert */}
        {(mintingStatus === 'error' && mintError) || error ? (
          <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-md animate-fade-in">
            <div className="w-full max-w-sm relative overflow-hidden rounded-3xl bg-gradient-to-br from-white/98 via-red-50/95 to-white/98 dark:from-slate-800/98 dark:via-red-900/95 dark:to-slate-800/98 border border-red-200/60 dark:border-red-600/40 shadow-2xl backdrop-blur-3xl animate-float-up"
                 style={{ boxShadow: '0 25px 50px -12px rgba(239, 68, 68, 0.25)' }}>
              {/* Liquid Glass Top Border */}
              <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-red-400/60 to-transparent"></div>
              
              {/* Floating Highlight */}
              <div className="absolute top-1 left-1/2 transform -translate-x-1/2 w-20 h-1 bg-gradient-to-r from-transparent via-white/50 to-transparent rounded-full blur-sm"></div>
              
              <div className="relative p-6 text-center">
                {/* Animated Error Icon */}
                <div className="relative mx-auto mb-4 w-16 h-16">
                  <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-red-500/90 via-orange-500/85 to-red-600/90 backdrop-blur-xl flex items-center justify-center shadow-lg shadow-red-500/30 animate-scale-in">
                    <AlertCircle className="w-8 h-8 text-white animate-fade-in animate-delay-200" />
                  </div>
                  <div className="absolute inset-0 rounded-2xl bg-gradient-to-br from-red-400/20 to-orange-400/20 animate-pulse"></div>
                </div>
                
                {/* Message Content */}
                <div className="mb-6">
                  <h3 className="text-lg font-bold text-red-700 dark:text-red-300 mb-2 animate-slide-in-right animate-delay-100">
                    Transaction Failed
                  </h3>
                  <p className="text-sm text-red-600/90 dark:text-red-400/90 animate-slide-in-right animate-delay-200">
                    {mintError || error || 'An error occurred'}
                  </p>
                </div>
                
                {/* Close Button */}
                <button
                  onClick={() => {
                    setMintingStatus('idle');
                    setMintError(null);
                    setError(null);
                  }}
                  className="w-full px-6 py-3 rounded-xl bg-gradient-to-r from-red-500/90 to-orange-500/90 hover:from-red-500 hover:to-orange-500 text-white font-semibold transition-all duration-200 backdrop-blur-sm shadow-lg"
                >
                  Try Again
                </button>
              </div>
            </div>
          </div>
        ) : null}

        {/* Compound-Style Portfolio Overview */}
        <div className="relative overflow-hidden rounded-3xl bg-gradient-to-br from-slate-50/60 via-white/20 to-slate-100/40 dark:from-slate-900/60 dark:via-slate-800/20 dark:to-slate-700/40 border border-slate-200/30 dark:border-slate-700/30 shadow-2xl backdrop-blur-2xl mb-6 animate-fade-in-scale"
             style={{ boxShadow: '0 20px 60px -12px rgba(0, 0, 0, 0.08), 0 8px 32px -8px rgba(0, 0, 0, 0.04)' }}>
          
          <div className="relative p-6 sm:p-8">
            {/* Top Section - Responsive Layout */}
            <div className="mb-8">
              {/* Desktop Layout - Horizontal */}
              <div className="hidden sm:flex items-center justify-between">
                {/* Supply Balance - Based on supplied PDOT (pTokens) */}
                <div className="text-left">
                  <p className="text-sm font-medium text-emerald-600 dark:text-emerald-400 mb-1 font-mono uppercase tracking-wide">
                    Supply Balance
                  </p>
                  <p className="text-3xl sm:text-4xl font-bold text-slate-900 dark:text-white font-mono">
                    ${(parseFloat(suppliedPDOT) * 0.85).toFixed(2)}
                  </p>
                  <p className="text-xs text-slate-600 dark:text-slate-400 font-mono mt-1">
                    {formatNumber(suppliedPDOT)} PDOT supplied
                  </p>
                </div>

                {/* Dynamic APY Circle - Liquid Glass Design */}
                <div className="flex flex-col items-center">
                  <div className="relative w-24 h-24 sm:w-28 sm:h-28">
                    {/* Outer Glass Ring with Multiple Layers */}
                    <div className={`absolute inset-0 rounded-full backdrop-blur-3xl border border-white/40 shadow-2xl ${
                      lendingMode === 'borrow' 
                        ? 'bg-gradient-to-br from-white/30 via-orange-200/20 to-red-300/25' 
                        : 'bg-gradient-to-br from-white/30 via-emerald-200/20 to-teal-300/25'
                    }`}
                         style={{ 
                           boxShadow: lendingMode === 'borrow'
                             ? '0 8px 32px rgba(249, 115, 22, 0.3), 0 4px 16px rgba(234, 88, 12, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.3)'
                             : '0 8px 32px rgba(16, 185, 129, 0.3), 0 4px 16px rgba(20, 184, 166, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.3)' 
                         }}>
                    </div>
                    
                    {/* Middle Frosted Layer */}
                    <div className={`absolute inset-1 rounded-full backdrop-blur-2xl shadow-inner ${
                      lendingMode === 'borrow'
                        ? 'bg-gradient-to-br from-orange-400/40 via-red-500/50 to-orange-600/40 border border-orange-300/50'
                        : 'bg-gradient-to-br from-emerald-400/40 via-teal-500/50 to-emerald-600/40 border border-emerald-300/50'
                    }`}
                         style={{ 
                           boxShadow: lendingMode === 'borrow'
                             ? '0 4px 20px rgba(249, 115, 22, 0.4), inset 0 2px 4px rgba(255, 255, 255, 0.1), inset 0 -2px 4px rgba(0, 0, 0, 0.1)'
                             : '0 4px 20px rgba(16, 185, 129, 0.4), inset 0 2px 4px rgba(255, 255, 255, 0.1), inset 0 -2px 4px rgba(0, 0, 0, 0.1)' 
                         }}>
                    </div>
                    
                    {/* Inner Content with Glass Effect */}
                    <div className={`absolute inset-2 rounded-full backdrop-blur-xl shadow-inner flex flex-col items-center justify-center ${
                      lendingMode === 'borrow'
                        ? 'bg-gradient-to-br from-orange-600/90 via-red-600/85 to-orange-700/90 border border-orange-400/60'
                        : 'bg-gradient-to-br from-emerald-600/90 via-teal-600/85 to-emerald-700/90 border border-emerald-400/60'
                    }`}
                         style={{ 
                           boxShadow: lendingMode === 'borrow'
                             ? 'inset 0 2px 8px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.2), 0 2px 12px rgba(249, 115, 22, 0.5)'
                             : 'inset 0 2px 8px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.2), 0 2px 12px rgba(16, 185, 129, 0.5)' 
                         }}>
                      
                      {/* Top Highlight */}
                      <div className="absolute top-1 left-1/2 transform -translate-x-1/2 w-8 h-2 bg-gradient-to-b from-white/40 to-transparent rounded-full blur-sm"></div>
                      
                      <span className={`text-xs font-medium mb-0.5 font-mono uppercase tracking-wide drop-shadow-sm ${
                        lendingMode === 'borrow' ? 'text-orange-100/90' : 'text-emerald-100/90'
                      }`}>
                        {lendingMode === 'borrow' ? 'BORROW APY' : 'SUPPLY APY'}
                      </span>
                      <span className="text-lg sm:text-xl font-bold text-white font-mono drop-shadow-md">
                        {lendingMode === 'borrow' ? `-${assetData[selectedAsset].borrowApy}%` : `+${assetData[selectedAsset].lendApy}%`}
                      </span>
                    </div>
                  </div>
                </div>

                {/* Borrow Balance */}
                <div className="text-right">
                  <p className="text-sm font-medium text-orange-600 dark:text-orange-400 mb-1 font-mono uppercase tracking-wide">
                    Borrow Balance
                  </p>
                  <p className="text-3xl sm:text-4xl font-bold text-slate-900 dark:text-white font-mono">
                    ${totalBorrowedUSD.toFixed(2)}
                  </p>
                  <p className="text-xs text-slate-600 dark:text-slate-400 font-mono mt-1">
                    {totalBorrowedUSD > 0 ? 'Active loans' : 'No active loans'}
                  </p>
                </div>
              </div>

              {/* Mobile Layout - APY Circle Left, Balances Stacked Right */}
              <div className="flex sm:hidden items-center space-x-4">
                {/* Dynamic APY Circle - Mobile Liquid Glass */}
                <div className="flex-shrink-0">
                  <div className="relative w-20 h-20">
                    {/* Outer Glass Ring with Multiple Layers */}
                    <div className={`absolute inset-0 rounded-full backdrop-blur-3xl border border-white/40 shadow-2xl ${
                      lendingMode === 'borrow' 
                        ? 'bg-gradient-to-br from-white/30 via-orange-200/20 to-red-300/25' 
                        : 'bg-gradient-to-br from-white/30 via-emerald-200/20 to-teal-300/25'
                    }`}
                         style={{ 
                           boxShadow: lendingMode === 'borrow'
                             ? '0 6px 24px rgba(249, 115, 22, 0.3), 0 3px 12px rgba(234, 88, 12, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.3)'
                             : '0 6px 24px rgba(16, 185, 129, 0.3), 0 3px 12px rgba(20, 184, 166, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.3)' 
                         }}>
                    </div>
                    
                    {/* Middle Frosted Layer */}
                    <div className={`absolute inset-0.5 rounded-full backdrop-blur-2xl shadow-inner ${
                      lendingMode === 'borrow'
                        ? 'bg-gradient-to-br from-orange-400/40 via-red-500/50 to-orange-600/40 border border-orange-300/50'
                        : 'bg-gradient-to-br from-emerald-400/40 via-teal-500/50 to-emerald-600/40 border border-emerald-300/50'
                    }`}
                         style={{ 
                           boxShadow: lendingMode === 'borrow'
                             ? '0 3px 16px rgba(249, 115, 22, 0.4), inset 0 1px 3px rgba(255, 255, 255, 0.1), inset 0 -1px 3px rgba(0, 0, 0, 0.1)'
                             : '0 3px 16px rgba(16, 185, 129, 0.4), inset 0 1px 3px rgba(255, 255, 255, 0.1), inset 0 -1px 3px rgba(0, 0, 0, 0.1)' 
                         }}>
                    </div>
                    
                    {/* Inner Content with Glass Effect */}
                    <div className={`absolute inset-1.5 rounded-full backdrop-blur-xl shadow-inner flex flex-col items-center justify-center ${
                      lendingMode === 'borrow'
                        ? 'bg-gradient-to-br from-orange-600/90 via-red-600/85 to-orange-700/90 border border-orange-400/60'
                        : 'bg-gradient-to-br from-emerald-600/90 via-teal-600/85 to-emerald-700/90 border border-emerald-400/60'
                    }`}
                         style={{ 
                           boxShadow: lendingMode === 'borrow'
                             ? 'inset 0 1px 6px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.2), 0 1px 8px rgba(249, 115, 22, 0.5)'
                             : 'inset 0 1px 6px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.2), 0 1px 8px rgba(16, 185, 129, 0.5)' 
                         }}>
                      
                      {/* Top Highlight */}
                      <div className="absolute top-0.5 left-1/2 transform -translate-x-1/2 w-6 h-1.5 bg-gradient-to-b from-white/40 to-transparent rounded-full blur-sm"></div>
                      
                      <span className={`text-xs font-medium mb-0.5 font-mono uppercase tracking-wide drop-shadow-sm ${
                        lendingMode === 'borrow' ? 'text-orange-100/90' : 'text-emerald-100/90'
                      }`}>
                        {lendingMode === 'borrow' ? 'BORROW APY' : 'SUPPLY APY'}
                      </span>
                      <span className="text-base font-bold text-white font-mono drop-shadow-md">
                        {lendingMode === 'borrow' ? `-${assetData[selectedAsset].borrowApy}%` : `+${assetData[selectedAsset].lendApy}%`}
                      </span>
                    </div>
                  </div>
                </div>

                {/* Balances Stack */}
                <div className="flex-1 space-y-4">
                  {/* Supply Balance */}
                  <div>
                    <p className="text-xs font-medium text-emerald-600 dark:text-emerald-400 mb-1 font-mono uppercase tracking-wide">
                      Supply Balance
                    </p>
                    <p className="text-2xl font-bold text-slate-900 dark:text-white font-mono">
                      ${(parseFloat(suppliedPDOT) * 0.85).toFixed(2)}
                    </p>
                  </div>

                  {/* Borrow Balance */}
                  <div>
                    <p className="text-xs font-medium text-orange-600 dark:text-orange-400 mb-1 font-mono uppercase tracking-wide">
                      Borrow Balance
                    </p>
                    <p className="text-2xl font-bold text-slate-900 dark:text-white font-mono">
                      ${totalBorrowedUSD.toFixed(2)}
                    </p>
                  </div>
                </div>
              </div>
            </div>

            {/* Borrow Limit Bar - Based on 80% of supplied collateral */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-slate-700 dark:text-slate-300 font-mono">Borrow Limit (80% of collateral)</span>
                <span className="text-sm font-bold text-emerald-600 dark:text-emerald-400 font-mono">
                  ${remainingBorrowingPowerUSD.toFixed(2)} available
                </span>
              </div>
              
              {/* Progress Bar */}
              <div className="relative h-2 bg-slate-200/50 dark:bg-slate-700/50 rounded-full overflow-hidden backdrop-blur-sm">
                <div className="absolute inset-0 bg-gradient-to-r from-slate-200/30 to-slate-300/30 dark:from-slate-700/30 dark:to-slate-600/30"></div>
                <div 
                  className="absolute top-0 left-0 h-full bg-gradient-to-r from-orange-400 via-amber-500 to-orange-500 rounded-full shadow-lg transition-all duration-1000 ease-out"
                  style={{ 
                    width: maxBorrowingPowerUSD > 0 ? `${(totalBorrowedUSD / maxBorrowingPowerUSD * 100).toFixed(1)}%` : '0%',
                    boxShadow: '0 2px 8px rgba(245, 158, 11, 0.3), inset 0 1px 0 rgba(255, 255, 255, 0.2)'
                  }}
                >
                  <div className="absolute inset-0 bg-gradient-to-t from-transparent via-white/10 to-white/20 rounded-full"></div>
                </div>
              </div>
              
              <div className="flex items-center justify-between text-xs text-slate-600 dark:text-slate-400 font-mono">
                <span>${totalBorrowedUSD.toFixed(2)} borrowed</span>
                <span>${maxBorrowingPowerUSD.toFixed(2)} max</span>
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

            {/* Asset Selection - Mobile First Grid */}
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-2 sm:gap-3 mb-4 sm:mb-6">
              {Object.entries(assetData).map(([asset, data]) => (
                <button
                  key={asset}
                  onClick={() => setSelectedAsset(asset as typeof selectedAsset)}
                  className={`group relative overflow-hidden p-3 sm:p-4 rounded-xl border transition-all duration-300 ease-out backdrop-blur-xl transform hover:scale-105 active:scale-95 cursor-pointer ${
                    selectedAsset === asset
                      ? 'bg-gradient-to-br from-emerald-500/20 via-teal-500/10 to-cyan-500/20 border-emerald-400/30'
                      : 'bg-gradient-to-br from-white/5 via-white/2 to-white/5 dark:from-slate-700/20 dark:via-slate-600/10 dark:to-slate-700/20 border-white/10 dark:border-slate-600/20 hover:border-emerald-400/20'
                  }`}
                  style={{
                    boxShadow: selectedAsset === asset 
                      ? '0 8px 32px -8px rgba(16, 185, 129, 0.25), 0 16px 48px -16px rgba(16, 185, 129, 0.15), 0 4px 16px -4px rgba(0, 0, 0, 0.1)'
                      : '0 4px 20px -4px rgba(0, 0, 0, 0.08), 0 8px 32px -8px rgba(0, 0, 0, 0.04), 0 12px 48px -12px rgba(0, 0, 0, 0.02)'
                  }}
                >
                  <div className="absolute inset-0 bg-gradient-to-br from-white/5 via-transparent to-white/2 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
                  
                  <div className="relative text-center">
                    <div className="flex items-center justify-between mb-2">
                      <h4 className="font-bold text-slate-900 dark:text-white text-sm group-hover:text-emerald-600 dark:group-hover:text-emerald-400 transition-colors duration-300">{asset}</h4>
                      <span className="text-xs font-semibold text-slate-600 dark:text-slate-400">${parseFloat(data.price).toFixed(asset === 'ETH' ? 0 : 2)}</span>
                    </div>
                    <p className="text-xs text-slate-600 dark:text-slate-400 mb-2">
                      {lendingMode === 'borrow' ? 'Borrowed:' : 'Balance:'} {formatNumber(lendingMode === 'borrow' ? data.borrowed : data.balance)}
                    </p>
                    <div className="flex items-center justify-center space-x-1 mb-2">
                      <Percent className="w-3 h-3 text-emerald-500 group-hover:scale-110 transition-transform duration-300" />
                      <span className="text-xs font-semibold text-emerald-500">{lendingMode === 'borrow' ? data.borrowApy : data.lendApy}% APY</span>
                    </div>
                    
                    {/* Repay/Withdraw button for borrowed assets */}
                    {lendingMode === 'borrow' && parseFloat(data.borrowed) > 0 && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setModalAsset(asset as typeof modalAsset);
                          setModalAction('repay');
                          setModalAmount('');
                          setIsModalOpen(true);
                        }}
                        className="w-full mt-2 px-2 py-1 bg-gradient-to-r from-orange-500/20 to-red-500/20 text-orange-600 dark:text-orange-400 text-xs font-bold rounded hover:from-orange-500/30 hover:to-red-500/30 transition-all duration-200"
                      >
                        Repay
                      </button>
                    )}
                  </div>
                </button>
              ))}
            </div>

            {/* Amount Input */}
            <div className="mb-4 sm:mb-6 relative" ref={inputContainerRef}>
              <div className="inputbox">
                <input
                  type="number"
                  value={amount}
                  onChange={(e) => setAmount(e.target.value)}
                  onFocus={handleInputFocus}
                  onBlur={handleInputBlur}
                  placeholder=""
                  step="any"
                  min="0"
                  required
                />
                <span>Enter {selectedAsset} amount</span>
                <i></i>
                <div className="max-button">
                                        <button
                        onClick={() => {
                          if (lendingMode === 'borrow') {
                            // Calculate max borrowable amount for selected asset (80% of remaining borrowing power)
                            // Subtract a small buffer (0.1%) to avoid floating point precision issues
                            const assetPrice = parseFloat(assetData[selectedAsset].price);
                            const maxBorrowable = (remainingBorrowingPowerUSD * 0.999) / assetPrice;
                            setAmount(maxBorrowable.toFixed(4));
                          } else {
                            setAmount(assetData[selectedAsset].balance);
                          }
                        }}
                    className="px-2 py-1 bg-gradient-to-r from-emerald-500/20 to-teal-500/20 text-emerald-600 dark:text-emerald-400 text-xs font-bold rounded border border-emerald-400/30 hover:from-emerald-500/30 hover:to-teal-500/30 hover:border-emerald-400/50 transition-all duration-300"
                  >
                    {lendingMode === 'borrow' ? '80%' : 'MAX'}
                  </button>
                </div>
              </div>
              
              {/* Earnings/Interest Projection Popup */}
              {(showEarningsPopup || isInputFocused) && amount && parseFloat(amount) > 0 && typeof document !== 'undefined' && createPortal(
                <div 
                  className="fixed inset-0 z-[9999] pointer-events-auto"
                  onClick={() => {
                    setShowEarningsPopup(false);
                    setIsInputFocused(false);
                  }}
                >
                  <div 
                    className={`absolute w-80 max-w-sm transform transition-all duration-300 ease-out ${
                      showEarningsPopup ? 'translate-y-0 opacity-100 scale-100' : 'translate-y-2 opacity-0 scale-95'
                    }`}
                    style={{
                      left: inputContainerRef.current ? Math.min(inputContainerRef.current.getBoundingClientRect().left, window.innerWidth - 320) : 0,
                      top: inputContainerRef.current ? inputContainerRef.current.getBoundingClientRect().bottom + 12 : 0,
                    }}
                    onMouseEnter={handlePopupMouseEnter}
                    onMouseLeave={handlePopupMouseLeave}
                    onClick={(e) => e.stopPropagation()}
                  >
                    <div className="relative overflow-hidden rounded-2xl bg-gradient-to-br from-white/95 via-white/98 to-white/95 dark:from-slate-800/90 dark:via-slate-700/85 dark:to-slate-800/90 border border-emerald-300/40 dark:border-emerald-400/30 shadow-2xl shadow-emerald-500/20 backdrop-blur-3xl">
                      <div className={`absolute inset-0 bg-gradient-to-br ${
                        lendingMode === 'lend' 
                          ? 'from-emerald-500/5 via-transparent to-cyan-500/5' 
                          : 'from-orange-500/5 via-transparent to-red-500/5'
                      }`}></div>
                      <div className={`absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r ${
                        lendingMode === 'lend'
                          ? 'from-emerald-400/60 via-teal-400/60 to-cyan-400/60'
                          : 'from-orange-400/60 via-amber-400/60 to-red-400/60'
                      }`}></div>
                      
                      <div className="relative p-4">
                        <div className="flex items-center space-x-3 mb-3">
                          <div className={`w-8 h-8 rounded-xl backdrop-blur-sm flex items-center justify-center border ${
                            lendingMode === 'lend'
                              ? 'bg-gradient-to-br from-emerald-500/20 to-teal-500/20 border-emerald-400/20'
                              : 'bg-gradient-to-br from-orange-500/20 to-red-500/20 border-orange-400/20'
                          }`}>
                            <svg className={`w-4 h-4 ${
                              lendingMode === 'lend' 
                                ? 'text-emerald-600 dark:text-emerald-400' 
                                : 'text-orange-600 dark:text-orange-400'
                            }`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
                                d={lendingMode === 'lend' ? "M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" : "M13 17h8m0 0V9m0 8l-8-8-4 4-6-6"} />
                            </svg>
                          </div>
                          <div>
                            <h4 className="text-sm font-semibold text-slate-900 dark:text-white">30-Day Projection</h4>
                            <p className="text-xs text-slate-600 dark:text-slate-400">
                              {lendingMode === 'lend' ? 'Estimated earnings' : 'Interest to pay'}
                            </p>
                          </div>
                        </div>
                        
                        <div className="space-y-2">
                          <div className="flex items-center justify-between">
                            <span className="text-sm text-slate-600 dark:text-slate-400">Amount</span>
                            <span className="text-sm font-medium text-slate-900 dark:text-white">{amount} {selectedAsset}</span>
                          </div>
                          <div className="flex items-center justify-between">
                            <span className="text-sm text-slate-600 dark:text-slate-400">APY</span>
                            <span className={`text-sm font-medium ${
                              lendingMode === 'lend' 
                                ? 'text-emerald-600 dark:text-emerald-400' 
                                : 'text-orange-600 dark:text-orange-400'
                            }`}>{lendingMode === 'borrow' ? assetData[selectedAsset].borrowApy : assetData[selectedAsset].lendApy}%</span>
                          </div>
                          <div className="h-px bg-gradient-to-r from-transparent via-slate-300/50 dark:via-slate-600/50 to-transparent"></div>
                          <div className="flex items-center justify-between">
                            <span className="text-sm font-semibold text-slate-700 dark:text-slate-300">
                              {lendingMode === 'lend' ? 'Est. Earnings (30d)' : 'Interest Cost (30d)'}
                            </span>
                            <div className="text-right">
                              <span className={`text-lg font-bold ${
                                lendingMode === 'lend' 
                                  ? 'text-emerald-600 dark:text-emerald-400' 
                                  : 'text-orange-600 dark:text-orange-400'
                              }`}>
                                ${calculateProjectedValue(amount).toFixed(2)}
                              </span>
                              <p className="text-xs text-slate-500 dark:text-slate-400">
                                {lendingMode === 'lend' ? '+' : '-'}{(calculateProjectedValue(amount) / parseFloat(assetData[selectedAsset].price)).toFixed(4)} {selectedAsset}
                              </p>
                            </div>
                          </div>
                        </div>
                        
                        <div className={`mt-3 p-2 rounded-xl border ${
                          lendingMode === 'lend'
                            ? 'bg-gradient-to-r from-emerald-500/10 via-teal-500/5 to-cyan-500/10 border-emerald-400/20'
                            : 'bg-gradient-to-r from-orange-500/10 via-amber-500/5 to-red-500/10 border-orange-400/20'
                        }`}>
                          <p className={`text-xs text-center ${
                            lendingMode === 'lend' 
                              ? 'text-emerald-700 dark:text-emerald-300' 
                              : 'text-orange-700 dark:text-orange-300'
                          }`}>
                            <span className="font-medium">
                              {lendingMode === 'lend' ? 'Compound interest' : 'Interest rate'} 
                            </span> calculated monthly
                          </p>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>,
                document.body
              )}
            </div>

            {/* Action Button */}
            <button
              onClick={handleLendingAction}
              disabled={!amount || isProcessing || parseFloat(amount) <= 0}
              className="w-full group relative overflow-hidden px-4 sm:px-6 py-3 sm:py-4 rounded-xl sm:rounded-2xl border border-emerald-400/20 hover:border-emerald-300/40 active:border-emerald-300/60 focus:outline-none focus:ring-4 focus:ring-emerald-400/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-300 shadow-xl hover:shadow-emerald-500/20 hover:shadow-2xl backdrop-blur-2xl transform hover:scale-[1.02] active:scale-[0.98] text-white font-semibold text-base sm:text-lg bg-gradient-to-r from-emerald-500 to-cyan-500"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-white/5 to-transparent opacity-0 group-hover:opacity-100 group-active:opacity-50 transition-opacity duration-300"></div>
              <div className="relative flex items-center justify-center space-x-3">
                {isProcessing ? (
                  <>
                    <div className="w-5 h-5 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
                    <span>
                      {lendingMode === 'lend' && selectedAsset === 'PDOT' 
                        ? 'Depositing to Vault...' 
                        : 'Processing...'}
                    </span>
                  </>
                ) : (
                  <>
                    {lendingMode === 'lend' ? (
                      <PiggyBank className="w-5 h-5" />
                    ) : (
                      <ArrowDownCircle className="w-5 h-5" />
                    )}
                    <span>
                      {lendingMode === 'lend' 
                        ? `Supply ${selectedAsset} to Vault` 
                        : `Borrow ${selectedAsset}`}
                    </span>
                  </>
                )}
              </div>
            </button>

            {/* Withdraw Section - Only show if user has supplied PDOT */}
            {lendingMode === 'lend' && selectedAsset === 'PDOT' && parseFloat(suppliedPDOT) > 0 && (
              <div className="mt-6 pt-6 border-t border-slate-200/20 dark:border-slate-600/20">
                <div className="flex items-center justify-between mb-4">
                  <div>
                    <h4 className="text-sm font-semibold text-slate-800 dark:text-slate-200">Your Supply Position</h4>
                    <p className="text-xs text-slate-600 dark:text-slate-400">{formatNumber(suppliedPDOT)} PDOT supplied</p>
                  </div>
                  <div className="text-right">
                    <p className="text-sm font-semibold text-emerald-600 dark:text-emerald-400">{formatNumber(suppliedPDOT)} pTokens</p>
                    <p className="text-xs text-slate-600 dark:text-slate-400">Receipt tokens</p>
                  </div>
                </div>
                
                <button
                  onClick={() => {
                    setModalAsset('PDOT');
                    setModalAction('withdraw');
                    setModalAmount('');
                    setIsModalOpen(true);
                  }}
                  className="w-full group relative overflow-hidden px-4 py-3 rounded-xl border border-orange-400/20 hover:border-orange-300/40 active:border-orange-300/60 focus:outline-none focus:ring-4 focus:ring-orange-400/30 transition-all duration-300 shadow-lg hover:shadow-orange-500/20 backdrop-blur-xl transform hover:scale-[1.02] active:scale-[0.98] bg-gradient-to-r from-orange-500/10 to-amber-500/10 hover:from-orange-500/20 hover:to-amber-500/20"
                >
                  <div className="relative flex items-center justify-center space-x-2">
                    <ArrowUpCircle className="w-4 h-4 text-orange-600 dark:text-orange-400" />
                    <span className="text-sm font-semibold text-orange-700 dark:text-orange-300">Withdraw PDOT</span>
                  </div>
                </button>
              </div>
            )}

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