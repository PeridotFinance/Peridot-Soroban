import { Address, nativeToScVal, xdr } from '@stellar/stellar-sdk';

export function toAddress(address: string): xdr.ScVal {
  return Address.fromString(address).toScVal();
}

export function toU128(value: bigint | number | string): xdr.ScVal {
  const big = typeof value === 'bigint' ? value : BigInt(value);
  if (big < 0n) {
    throw new Error('u128 cannot be negative');
  }
  return nativeToScVal(big, { type: 'u128' });
}

export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}
