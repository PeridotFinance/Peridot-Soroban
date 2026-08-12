import { Networks, TransactionBuilder } from '@stellar/stellar-sdk';
import { NextRequest, NextResponse } from 'next/server';

export async function POST(request: NextRequest) {
  try {
    const { signedTxXdr } = await request.json();
    if (
      typeof signedTxXdr !== 'string'
      || signedTxXdr.length === 0
      || signedTxXdr.length > 100_000
    ) {
      return NextResponse.json(
        { success: false, error: 'Signed transaction XDR is required' },
        { status: 400 }
      );
    }

    try {
      TransactionBuilder.fromXDR(signedTxXdr, Networks.TESTNET);
    } catch (decodeError) {
      console.warn('Invalid transaction XDR:', decodeError);
      return NextResponse.json(
        { success: false, error: 'Invalid signed transaction XDR' },
        { status: 400 }
      );
    }

    const response = await fetch('https://horizon-testnet.stellar.org/transactions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: `tx=${encodeURIComponent(signedTxXdr)}`,
    });

    if (!response.ok) {
      const errorData = await response.json();
      console.error('Horizon submission error:', errorData);

      let errorMessage = 'Transaction submission failed';
      if (errorData.extras?.result_codes) {
        const codes = errorData.extras.result_codes;
        errorMessage = `Transaction failed: ${codes.transaction || 'Unknown error'}`;
        if (codes.operations) errorMessage += ` (Operations: ${codes.operations.join(', ')})`;
      } else if (errorData.detail) {
        errorMessage = `Transaction failed: ${errorData.detail}`;
      }

      return NextResponse.json({
        success: false,
        error: errorMessage,
        details: errorData,
      });
    }

    const result = await response.json();
    return NextResponse.json({
      success: true,
      transactionHash: result.hash,
      message: 'Transaction submitted successfully',
    });
  } catch (error) {
    console.error('Submit transaction error:', error);
    return NextResponse.json(
      { success: false, error: 'Failed to submit transaction' },
      { status: 500 }
    );
  }
}
