# Stellar Vault Dashboard

A DeFi vault dashboard built with Next.js, showcasing Soroban smart contract integration on the Stellar network. Users can deposit TEST tokens and receive pTokens (receipt tokens) using Freighter wallet integration.

## 🎯 Features

- **Wallet Integration**: Connect with Freighter wallet
- **Token Management**: Mint TEST tokens for testing
- **Vault Operations**: Deposit and withdraw functionality
- **Real-time Stats**: Live vault statistics and user share
- **Modern UI**: Beautiful, responsive interface with Tailwind CSS

## 🚀 Quick Start

### Prerequisites

- Node.js 18+ installed
- Freighter wallet extension installed
- Stellar testnet account

### Installation

1. **Clone and install dependencies:**
```bash
npm install
```

2. **Environment Setup:**
Create a `.env.local` file in the project root:
```env
NEXT_PUBLIC_STELLAR_NETWORK=testnet
NEXT_PUBLIC_VAULT_CONTRACT=CBJABFTHC6HASPK4VZFNWRRTXQKOBUEA4VIAE4G36W4C2S4LU2C5GSTH
NEXT_PUBLIC_TOKEN_CONTRACT=CAQYNJBC2BWWMQPM5567OX2DMS4QC46ZJDH3JCOPDH635KTYTXDEUSJI
NEXT_PUBLIC_ALICE_ADDRESS=GDCN5BORBQZOXM7LTAKAPJVTIG3QV6MNZKH6Z2FTUWQMCITZSN7RIB2T
ALICE_SECRET_KEY=your_alice_secret_key_here
```

3. **Run the development server:**
```bash
npm run dev
```

4. **Open [http://localhost:3000](http://localhost:3000)** in your browser

## 🏗️ Architecture

### Smart Contracts (Testnet)
- **Receipt Vault**: `CBJABFTHC6HASPK4VZFNWRRTXQKOBUEA4VIAE4G36W4C2S4LU2C5GSTH`
- **TEST Token**: `CAQYNJBC2BWWMQPM5567OX2DMS4QC46ZJDH3JCOPDH635KTYTXDEUSJI`

### Tech Stack
- **Frontend**: Next.js 14, React, TypeScript
- **Styling**: Tailwind CSS, Lucide React Icons
- **Blockchain**: Stellar SDK, Freighter API
- **Network**: Stellar Testnet, Soroban RPC

### Project Structure
```
src/
├── app/
│   ├── api/           # API routes for server-side operations
│   ├── page.tsx       # Main dashboard page
│   └── layout.tsx     # Root layout
├── components/        # React components
│   ├── ConnectWallet.tsx
│   ├── TokenManager.tsx
│   ├── VaultInterface.tsx
│   └── VaultStats.tsx
└── utils/
    └── stellar.ts     # Stellar SDK utilities
```

## 🎮 How to Use

### 1. Connect Wallet
- Install [Freighter wallet](https://freighter.app/)
- Click "Connect Freighter" button
- Approve the connection

### 2. Get Test Tokens
- Click "Get 1,000 TEST Tokens" 
- Wait for minting confirmation
- Tokens will appear in your wallet

### 3. Vault Operations
- **Deposit**: Enter amount and click "Deposit TEST Tokens"
- **Withdraw**: Enter pToken amount and click "Withdraw pTokens"
- All transactions require Freighter signature

### 4. Monitor Stats
- View total vault deposits and pTokens issued
- See your share percentage
- Monitor exchange rates (1:1 ratio)

## 🛠️ Development

### API Routes
- `POST /api/mint-tokens` - Mint TEST tokens (Updated: now uses direct SDK calls)
- ~~`GET /api/token-balance` - Get token balance~~ (Removed: now uses direct SDK calls)
- ~~`GET /api/ptoken-balance` - Get pToken balance~~ (Removed: now uses direct SDK calls)
- `GET /api/vault-stats` - Get vault statistics (Updated: now uses direct SDK calls)
- `POST /api/deposit` - Process deposits
- `POST /api/withdraw` - Process withdrawals

### Component Props
```typescript
interface WalletInfo {
  isConnected: boolean;
  address: string;
  xlmBalance: string;
  testTokenBalance: string;
  pTokenBalance: string;
}

interface VaultStats {
  totalDeposited: string;
  totalPTokens: string;
  exchangeRate: string;
  userShare: string;
}
```

## 🎨 Design System

### Colors
- **Primary**: `#62a352` (Green)
- **Secondary**: `#4a7c3a` (Dark Green)
- **Accent**: `#7bb365` (Light Green)
- **Background**: `#f8fdf6` (Very Light Green)

### Components
- Consistent rounded corners (8px, 12px)
- Green color scheme throughout
- Loading states and animations
- Responsive grid layouts

## 🔧 Configuration

### Environment Variables
| Variable | Description | Required |
|----------|-------------|----------|
| `NEXT_PUBLIC_VAULT_CONTRACT` | Vault contract address | Yes |
| `NEXT_PUBLIC_TOKEN_CONTRACT` | Token contract address | Yes |
| `NEXT_PUBLIC_ALICE_ADDRESS` | Alice's public key | Yes |
| `ALICE_SECRET_KEY` | Alice's secret key (server-side) | Yes |

### Wallet Configuration
- Network: Stellar Testnet
- Required: Freighter extension
- Permissions: Sign transactions

## 📚 Learning Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Smart Contracts](https://stellar.org/soroban)
- [Freighter Wallet](https://freighter.app/)
- [Next.js Documentation](https://nextjs.org/docs)

## ⚠️ Important Notes

- **Testnet Only**: This dashboard uses Stellar Testnet
- **No Real Value**: TEST tokens have no monetary value
- **Educational Purpose**: Built for learning and demonstration
- **Mock Implementation**: Some features use simulated responses

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## 📄 License

This project is for educational purposes. See the [Stellar Development Foundation](https://stellar.org/) for more information about Stellar and Soroban.

---

**Built with ❤️ on Stellar Testnet**
