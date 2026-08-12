import { execFile } from 'child_process';
import { promisify } from 'util';
import { Networks, StrKey, TransactionBuilder } from '@stellar/stellar-sdk';
import { NextRequest, NextResponse } from 'next/server';

const execFileAsync = promisify(execFile);
const VAULT_CONTRACT_ID = process.env.NEXT_PUBLIC_VAULT_CONTRACT!;

function parseAmount(value: unknown): string | null {
  const match = /^(0|[1-9]\d{0,29})(?:\.(\d{1,9}))?$/.exec(String(value ?? ''));
  if (!match) return null;

  const units = BigInt(match[1]) * BigInt('1000000000')
    + BigInt((match[2] ?? '').padEnd(9, '0'));
  const maxU128 = BigInt('340282366920938463463374607431768211455');
  return units > BigInt(0) && units <= maxU128 ? units.toString() : null;
}

export async function POST(request: NextRequest) {
  try {
    const { userAddress, amount } = await request.json();
    const amountInUnits = parseAmount(amount);

    if (
      typeof userAddress !== 'string'
      || !StrKey.isValidEd25519PublicKey(userAddress)
      || amountInUnits === null
    ) {
      return NextResponse.json(
        { success: false, error: 'A valid user address and positive amount are required' },
        { status: 400 }
      );
    }

    if (!VAULT_CONTRACT_ID || !StrKey.isValidContract(VAULT_CONTRACT_ID)) {
      return NextResponse.json(
        { success: false, error: 'Vault contract not configured' },
        { status: 500 }
      );
    }

    const result = await execFileAsync('stellar', [
      'contract', 'invoke',
      '--id', VAULT_CONTRACT_ID,
      '--source-account', userAddress,
      '--network', 'testnet',
      '--build-only',
      '--',
      'deposit',
      '--user', userAddress,
      '--amount', amountInUnits,
    ]);

    const transactionXdr = result.stdout.trim();
    TransactionBuilder.fromXDR(transactionXdr, Networks.TESTNET);

    return NextResponse.json({
      success: true,
      transactionXdr,
      amount,
    });
  } catch (error) {
    console.error('Deposit error:', error);
    return NextResponse.json(
      { success: false, error: 'Failed to process deposit' },
      { status: 500 }
    );
  }
}
