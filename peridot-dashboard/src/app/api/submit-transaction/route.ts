import { NextRequest, NextResponse } from 'next/server';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export async function POST(request: NextRequest) {
  try {
    const { signedTxXdr } = await request.json();

    if (!signedTxXdr) {
      return NextResponse.json(
        { success: false, error: 'Signed transaction XDR is required' },
        { status: 400 }
      );
    }

    console.log('Submitting signed transaction via CLI...');
    console.log('XDR:', signedTxXdr);

    // Try to submit using stellar CLI
    // Note: The CLI might not have a direct submit command, so let's try a different approach
    
    // First, let's try to decode and validate the XDR
    const decodeCommand = `echo "${signedTxXdr}" | base64 -d | xxd`;
    
    try {
      const decodeResult = await execAsync(decodeCommand);
      console.log('XDR decode result:', decodeResult.stdout);
    } catch (decodeError) {
      console.log('XDR decode failed:', decodeError);
    }

    // For now, let's use a direct HTTP submission to Horizon
    const response = await fetch('https://horizon-testnet.stellar.org/transactions', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      body: `tx=${encodeURIComponent(signedTxXdr)}`
    });

    if (!response.ok) {
      const errorData = await response.json();
      console.error('Horizon submission error:', errorData);
      
      let errorMessage = 'Transaction submission failed';
      if (errorData.extras?.result_codes) {
        const codes = errorData.extras.result_codes;
        errorMessage = `Transaction failed: ${codes.transaction || 'Unknown error'}`;
        if (codes.operations) {
          errorMessage += ` (Operations: ${codes.operations.join(', ')})`;
        }
      } else if (errorData.detail) {
        errorMessage = `Transaction failed: ${errorData.detail}`;
      }
      
      return NextResponse.json({ 
        success: false, 
        error: errorMessage,
        details: errorData
      });
    }

    const result = await response.json();
    console.log('Transaction submitted successfully:', result.hash);
    
    return NextResponse.json({
      success: true,
      transactionHash: result.hash,
      message: 'Transaction submitted successfully'
    });

  } catch (error) {
    console.error('Submit transaction error:', error);
    return NextResponse.json(
      { success: false, error: 'Failed to submit transaction' },
      { status: 500 }
    );
  }
} 