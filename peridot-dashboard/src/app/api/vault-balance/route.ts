import { execFile } from 'child_process';
import { promisify } from 'util';
import { StrKey } from '@stellar/stellar-sdk';
import { NextRequest, NextResponse } from 'next/server';

const execFileAsync = promisify(execFile);
const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;

function formatUnits(raw: string): string {
  const padded = raw.replace(/^0+(?=\d)/, '').padStart(10, '0');
  const whole = padded.slice(0, -9);
  const fraction = padded.slice(-9).replace(/0+$/, '');
  return fraction ? `${whole}.${fraction}` : whole;
}

export async function GET(request: NextRequest) {
  try {
    const address = new URL(request.url).searchParams.get('address');
    if (!address || !StrKey.isValidEd25519PublicKey(address)) {
      return NextResponse.json(
        { error: 'A valid address parameter is required' },
        { status: 400 }
      );
    }

    if (!VAULT_CONTRACT_ID || !StrKey.isValidContract(VAULT_CONTRACT_ID)) {
      return NextResponse.json(
        { error: 'Vault contract not configured' },
        { status: 500 }
      );
    }

    const result = await execFileAsync('stellar', [
      'contract', 'invoke',
      '--id', VAULT_CONTRACT_ID,
      '--source-account', address,
      '--network', 'testnet',
      '--send', 'no', '--',
      'get_user_balance',
      '--user', address,
    ]);

    const balanceMatch = result.stdout.match(/^"?(\d+)"?\s*$/);
    const balance = balanceMatch ? formatUnits(balanceMatch[1]) : '0';

    return NextResponse.json({ balance, address });
  } catch (error) {
    console.error('Vault balance error:', error);
    return NextResponse.json(
      { error: 'Failed to fetch vault balance' },
      { status: 500 }
    );
  }
}
