import { NextRequest, NextResponse } from 'next/server';

const TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_TOKEN_CONTRACT!;
const ALICE_ADDRESS = process.env.NEXT_PUBLIC_ALICE_ADDRESS!;
const ALICE_SECRET_KEY = process.env.ALICE_SECRET_KEY!;

export async function POST(request: NextRequest) {
  try {
    const { userAddress } = await request.json();

    console.log('=== MINT TOKENS DEBUG START ===');
    console.log('Request body:', { userAddress });
    
    // Debug environment variables
    console.log('Environment Variables:');
    console.log('- TOKEN_CONTRACT_ID:', TOKEN_CONTRACT_ID ? `${TOKEN_CONTRACT_ID} (length: ${TOKEN_CONTRACT_ID.length})` : 'UNDEFINED');
    console.log('- ALICE_ADDRESS:', ALICE_ADDRESS ? `${ALICE_ADDRESS} (length: ${ALICE_ADDRESS.length})` : 'UNDEFINED');
    console.log('- ALICE_SECRET_KEY:', ALICE_SECRET_KEY ? `[SET] (length: ${ALICE_SECRET_KEY.length})` : 'UNDEFINED');

    if (!userAddress) {
      console.log('ERROR: User address is missing');
      return NextResponse.json(
        { success: false, error: 'User address is required' },
        { status: 400 }
      );
    }

    if (!TOKEN_CONTRACT_ID || !ALICE_ADDRESS) {
      console.log('ERROR: Missing contract or Alice address');
      return NextResponse.json(
        { success: false, error: 'Server configuration error: Missing contract or Alice address' },
        { status: 500 }
      );
    }

    if (!ALICE_SECRET_KEY) {
      console.log('ERROR: Missing Alice secret key');
      return NextResponse.json(
        { success: false, error: 'Server configuration error: Missing Alice secret key' },
        { status: 500 }
      );
    }

    console.log(`Processing token transfer for user: ${userAddress}`);
    
    // Use Stellar SDK directly
    const StellarSdk = await import('@stellar/stellar-sdk');
    console.log('Stellar SDK imported successfully');
    
    // Use SorobanRpc server for contract interactions
    const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
    console.log('RPC server created');
    
    // Create Alice's keypair from secret key
    let aliceKeypair;
    try {
      aliceKeypair = StellarSdk.Keypair.fromSecret(ALICE_SECRET_KEY);
      console.log('Alice keypair created successfully');
      console.log('Alice public key from keypair:', aliceKeypair.publicKey());
      console.log('Alice address matches?', aliceKeypair.publicKey() === ALICE_ADDRESS);
    } catch (keypairError) {
      console.error('Failed to create Alice keypair:', keypairError);
      return NextResponse.json(
        { success: false, error: 'Invalid Alice secret key' },
        { status: 500 }
      );
    }
    
    // Build the contract instance
    const contract = new StellarSdk.Contract(TOKEN_CONTRACT_ID);
    console.log('Contract instance created for:', TOKEN_CONTRACT_ID);
    
    // First, let's check Alice's current balance with detailed logging
    console.log('\n=== CHECKING ALICE BALANCE ===');
    
    try {
      // Debug the balance call parameters
      const aliceAddressScVal = StellarSdk.Address.fromString(ALICE_ADDRESS).toScVal();
      console.log('Alice address as ScVal:', aliceAddressScVal);
      
      const balanceOperation = contract.call('balance', aliceAddressScVal);
      console.log('Balance operation created');

      const dummyAccount = new StellarSdk.Account(
        'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
        '0'
      );

      const balanceTransaction = new StellarSdk.TransactionBuilder(dummyAccount, {
        fee: StellarSdk.BASE_FEE,
        networkPassphrase: StellarSdk.Networks.TESTNET,
      })
        .addOperation(balanceOperation)
        .setTimeout(30)
        .build();

      console.log('Balance transaction built, simulating...');
      const balanceSimResult = await rpc.simulateTransaction(balanceTransaction);
      console.log('Balance simulation result:', JSON.stringify(balanceSimResult, null, 2));
      
      let aliceBalance = '0';
      if (StellarSdk.rpc.Api.isSimulationSuccess(balanceSimResult)) {
        console.log('Balance simulation was successful');
        const resultScVal = balanceSimResult.result?.retval;
        console.log('Balance result ScVal:', resultScVal);
        
        if (resultScVal) {
          const balanceValue = StellarSdk.scValToNative(resultScVal);
          aliceBalance = balanceValue.toString();
          console.log(`Alice current balance: ${aliceBalance} (raw contract units)`);
          console.log(`Alice balance in PDOT: ${parseInt(aliceBalance) / 1000000000} PDOT`);
        } else {
          console.log('No result ScVal returned from balance query');
        }
      } else {
        console.error('Balance simulation failed:', balanceSimResult);
      }
      
      // Convert balance to check if Alice has enough tokens
      const aliceBalanceNum = parseInt(aliceBalance);
      const requiredAmount = 1000000000000; // 1000 PDOT in contract units (1000 * 10^9)
      
      console.log(`\nBalance comparison:`);
      console.log(`- Alice current: ${aliceBalanceNum} contract units`);
      console.log(`- Required: ${requiredAmount} contract units`);
      console.log(`- Alice has enough?: ${aliceBalanceNum >= requiredAmount}`);
      
      if (aliceBalanceNum < requiredAmount) {
        console.log('\n=== ATTEMPTING TO MINT TOKENS ===');
        console.log(`Alice needs more tokens. Attempting to mint...`);
        
        try {
          // Get Alice's account for building transactions
          console.log('Getting Alice account from network...');
          const aliceAccount = await rpc.getAccount(aliceKeypair.publicKey());
          console.log('Alice account loaded. Sequence number:', aliceAccount.sequenceNumber());
          
          // Debug mint parameters
          const mintToAddress = StellarSdk.Address.fromString(ALICE_ADDRESS).toScVal();
          const mintAmount = StellarSdk.nativeToScVal(requiredAmount * 1000, { type: 'u128' }); // Mint 1M tokens
          
          console.log('Mint parameters:');
          console.log('- Function name: mint');
          console.log('- To address (Alice):', ALICE_ADDRESS);
          console.log('- To address as ScVal:', mintToAddress);
          console.log('- Amount (raw):', requiredAmount * 1000);
          console.log('- Amount as ScVal:', mintAmount);
          
          const mintOperation = contract.call('mint', mintToAddress, mintAmount);
          console.log('Mint operation created');

          const mintTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(mintOperation)
            .setTimeout(30)
            .build();

          console.log('Mint transaction built. Preparing...');
          const preparedMintTransaction = await rpc.prepareTransaction(mintTransaction);
          console.log('Mint transaction prepared. Signing...');
          
          preparedMintTransaction.sign(aliceKeypair);
          console.log('Mint transaction signed. Submitting...');

          const mintResult = await rpc.sendTransaction(preparedMintTransaction);
          console.log('Mint result:', JSON.stringify(mintResult, null, 2));

          if (mintResult.status === 'ERROR') {
            console.error('❌ MINT FAILED');
            console.error('Error result:', mintResult.errorResult);
            console.error('Full result:', mintResult);
          } else {
            console.log('✅ MINT SUCCESSFUL');
            console.log('Transaction hash:', mintResult.hash);
            console.log('Waiting for confirmation...');
            await new Promise(resolve => setTimeout(resolve, 4000));
          }
        } catch (mintError) {
          console.error('❌ MINT EXCEPTION:', mintError);
          if (mintError instanceof Error) {
            console.error('Mint error name:', mintError.name);
            console.error('Mint error message:', mintError.message);
            console.error('Mint error stack:', mintError.stack);
          }
        }
      } else {
        console.log('✅ Alice has sufficient balance, skipping mint');
      }
      
      // Now attempt the transfer
      console.log('\n=== ATTEMPTING TRANSFER ===');
      
      try {
        // Get updated Alice account
        console.log('Getting updated Alice account...');
        const aliceAccount = await rpc.getAccount(aliceKeypair.publicKey());
        console.log('Updated Alice account loaded. Sequence:', aliceAccount.sequenceNumber());
        
        // Debug transfer parameters
        const transferFromAddress = StellarSdk.Address.fromString(ALICE_ADDRESS).toScVal();
        const transferToAddress = StellarSdk.Address.fromString(userAddress).toScVal();
        const transferAmount = StellarSdk.nativeToScVal(requiredAmount, { type: 'u128' });
        
        console.log('Transfer parameters:');
        console.log('- Function name: transfer');
        console.log('- From address (Alice):', ALICE_ADDRESS);
        console.log('- From address as ScVal:', transferFromAddress);
        console.log('- To address (User):', userAddress);
        console.log('- To address as ScVal:', transferToAddress);
        console.log('- Amount (raw):', requiredAmount);
        console.log('- Amount as ScVal:', transferAmount);
        
        const transferOperation = contract.call('transfer', transferFromAddress, transferToAddress, transferAmount);
        console.log('Transfer operation created');

        // APPROACH 1: Try with explicit authorization
        console.log('\n--- APPROACH 1: With Authorization ---');
        try {
          const transferTransactionWithAuth = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(transferOperation)
            .setTimeout(30)
            .build();

          console.log('Transfer transaction built. Preparing with auth...');
          const preparedTransferTransactionWithAuth = await rpc.prepareTransaction(transferTransactionWithAuth);
          console.log('Transfer transaction prepared. Signing with Alice...');
          
          preparedTransferTransactionWithAuth.sign(aliceKeypair);
          console.log('Transfer transaction signed. Submitting...');

          const transferResultWithAuth = await rpc.sendTransaction(preparedTransferTransactionWithAuth);
          console.log('Transfer result with auth:', JSON.stringify(transferResultWithAuth, null, 2));

          if (transferResultWithAuth.status !== 'ERROR') {
            console.log('✅ TRANSFER SUCCESSFUL WITH AUTH');
            return NextResponse.json({
              success: true,
              message: 'Successfully transferred 1,000 PDOT tokens',
              transactionHash: transferResultWithAuth.hash,
              details: {
                transferTransactionHash: transferResultWithAuth.hash,
                amountTransferred: '1,000 PDOT',
                aliceBalanceBefore: aliceBalance,
                contractId: TOKEN_CONTRACT_ID,
                approach: 'with_authorization'
              }
            });
          } else {
            console.error('❌ APPROACH 1 FAILED:', transferResultWithAuth.errorResult);
          }
        } catch (authError) {
          console.error('❌ APPROACH 1 EXCEPTION:', authError);
        }

        // APPROACH 2: Try different parameter order (amount, from, to)
        console.log('\n--- APPROACH 2: Different Parameter Order ---');
        try {
          const transferOperation2 = contract.call('transfer', transferAmount, transferFromAddress, transferToAddress);
          console.log('Transfer operation 2 created (amount, from, to)');

          const transferTransaction2 = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(transferOperation2)
            .setTimeout(30)
            .build();

          const preparedTransferTransaction2 = await rpc.prepareTransaction(transferTransaction2);
          preparedTransferTransaction2.sign(aliceKeypair);

          const transferResult2 = await rpc.sendTransaction(preparedTransferTransaction2);
          console.log('Transfer result 2:', JSON.stringify(transferResult2, null, 2));

          if (transferResult2.status !== 'ERROR') {
            console.log('✅ TRANSFER SUCCESSFUL WITH DIFFERENT ORDER');
            return NextResponse.json({
              success: true,
              message: 'Successfully transferred 1,000 PDOT tokens',
              transactionHash: transferResult2.hash,
              details: {
                transferTransactionHash: transferResult2.hash,
                amountTransferred: '1,000 PDOT',
                aliceBalanceBefore: aliceBalance,
                contractId: TOKEN_CONTRACT_ID,
                approach: 'different_parameter_order'
              }
            });
          } else {
            console.error('❌ APPROACH 2 FAILED:', transferResult2.errorResult);
          }
        } catch (orderError) {
          console.error('❌ APPROACH 2 EXCEPTION:', orderError);
        }

        // APPROACH 3: Try with transfer_from function instead
        console.log('\n--- APPROACH 3: Using transfer_from ---');
        try {
          const transferFromOperation = contract.call('transfer_from', transferFromAddress, transferFromAddress, transferToAddress, transferAmount);
          console.log('Transfer_from operation created');

          const transferFromTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(transferFromOperation)
            .setTimeout(30)
            .build();

          const preparedTransferFromTransaction = await rpc.prepareTransaction(transferFromTransaction);
          preparedTransferFromTransaction.sign(aliceKeypair);

          const transferFromResult = await rpc.sendTransaction(preparedTransferFromTransaction);
          console.log('Transfer_from result:', JSON.stringify(transferFromResult, null, 2));

          if (transferFromResult.status !== 'ERROR') {
            console.log('✅ TRANSFER SUCCESSFUL WITH TRANSFER_FROM');
            return NextResponse.json({
              success: true,
              message: 'Successfully transferred 1,000 PDOT tokens',
              transactionHash: transferFromResult.hash,
              details: {
                transferTransactionHash: transferFromResult.hash,
                amountTransferred: '1,000 PDOT',
                aliceBalanceBefore: aliceBalance,
                contractId: TOKEN_CONTRACT_ID,
                approach: 'transfer_from'
              }
            });
          } else {
            console.error('❌ APPROACH 3 FAILED:', transferFromResult.errorResult);
          }
        } catch (transferFromError) {
          console.error('❌ APPROACH 3 EXCEPTION:', transferFromError);
        }

        // APPROACH 4: Try simulating first to see what the contract expects
        console.log('\n--- APPROACH 4: Simulation Analysis ---');
        try {
          const simulateTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(transferOperation)
            .setTimeout(30)
            .build();

          console.log('Simulating transfer transaction...');
          const simResult = await rpc.simulateTransaction(simulateTransaction);
          console.log('Transfer simulation result:', JSON.stringify(simResult, null, 2));

          if (StellarSdk.rpc.Api.isSimulationSuccess(simResult)) {
            console.log('✅ SIMULATION SUCCESSFUL - Transaction should work');
            
            // If simulation works, try the actual transaction
            const preparedSimTransaction = await rpc.prepareTransaction(simulateTransaction);
            preparedSimTransaction.sign(aliceKeypair);
            
            const simBasedResult = await rpc.sendTransaction(preparedSimTransaction);
            console.log('Simulation-based result:', JSON.stringify(simBasedResult, null, 2));
            
            if (simBasedResult.status !== 'ERROR') {
              console.log('✅ TRANSFER SUCCESSFUL AFTER SIMULATION');
              return NextResponse.json({
                success: true,
                message: 'Successfully transferred 1,000 PDOT tokens',
                transactionHash: simBasedResult.hash,
                details: {
                  transferTransactionHash: simBasedResult.hash,
                  amountTransferred: '1,000 PDOT',
                  aliceBalanceBefore: aliceBalance,
                  contractId: TOKEN_CONTRACT_ID,
                  approach: 'after_simulation'
                }
              });
            }
          } else {
            console.error('❌ SIMULATION FAILED:', simResult);
          }
        } catch (simError) {
          console.error('❌ APPROACH 4 EXCEPTION:', simError);
        }

        // APPROACH 5: Try calling approve first, then transfer
        console.log('\n--- APPROACH 5: Approve then Transfer ---');
        try {
          // First try to approve the transfer
          const approveOperation = contract.call('approve', transferFromAddress, transferToAddress, transferAmount);
          console.log('Approve operation created');

          const approveTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(approveOperation)
            .setTimeout(30)
            .build();

          const preparedApproveTransaction = await rpc.prepareTransaction(approveTransaction);
          preparedApproveTransaction.sign(aliceKeypair);

          const approveResult = await rpc.sendTransaction(preparedApproveTransaction);
          console.log('Approve result:', JSON.stringify(approveResult, null, 2));

          if (approveResult.status !== 'ERROR') {
            console.log('✅ APPROVE SUCCESSFUL, now trying transfer...');
            
            // Wait a moment for approval to be confirmed
            await new Promise(resolve => setTimeout(resolve, 2000));
            
            // Get fresh account and try transfer again
            const freshAliceAccount = await rpc.getAccount(aliceKeypair.publicKey());
            
            const finalTransferTransaction = new StellarSdk.TransactionBuilder(freshAliceAccount, {
              fee: StellarSdk.BASE_FEE,
              networkPassphrase: StellarSdk.Networks.TESTNET,
            })
              .addOperation(transferOperation)
              .setTimeout(30)
              .build();

            const preparedFinalTransfer = await rpc.prepareTransaction(finalTransferTransaction);
            preparedFinalTransfer.sign(aliceKeypair);

            const finalTransferResult = await rpc.sendTransaction(preparedFinalTransfer);
            console.log('Final transfer result:', JSON.stringify(finalTransferResult, null, 2));

            if (finalTransferResult.status !== 'ERROR') {
              console.log('✅ TRANSFER SUCCESSFUL AFTER APPROVE');
              return NextResponse.json({
                success: true,
                message: 'Successfully transferred 1,000 PDOT tokens',
                transactionHash: finalTransferResult.hash,
                details: {
                  approveTransactionHash: approveResult.hash,
                  transferTransactionHash: finalTransferResult.hash,
                  amountTransferred: '1,000 PDOT',
                  aliceBalanceBefore: aliceBalance,
                  contractId: TOKEN_CONTRACT_ID,
                  approach: 'approve_then_transfer'
                }
              });
            } else {
              console.error('❌ FINAL TRANSFER FAILED:', finalTransferResult.errorResult);
            }
          } else {
            console.error('❌ APPROVE FAILED:', approveResult.errorResult);
          }
        } catch (approveError) {
          console.error('❌ APPROACH 5 EXCEPTION:', approveError);
        }

        // APPROACH 6: Try calling with named parameters style
        console.log('\n--- APPROACH 6: Named Parameters Style ---');
        try {
          // Try using a map-like structure for named parameters
          const namedParams = StellarSdk.nativeToScVal({
            from: ALICE_ADDRESS,
            to: userAddress,
            amount: requiredAmount.toString()
          }, { type: 'map' });
          
          console.log('Named parameters as Map:', namedParams);
          
          const namedTransferOperation = contract.call('transfer', namedParams);
          console.log('Named transfer operation created');

          const namedTransferTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(namedTransferOperation)
            .setTimeout(30)
            .build();

          const preparedNamedTransfer = await rpc.prepareTransaction(namedTransferTransaction);
          preparedNamedTransfer.sign(aliceKeypair);

          const namedTransferResult = await rpc.sendTransaction(preparedNamedTransfer);
          console.log('Named transfer result:', JSON.stringify(namedTransferResult, null, 2));

          if (namedTransferResult.status !== 'ERROR') {
            console.log('✅ TRANSFER SUCCESSFUL WITH NAMED PARAMETERS');
            return NextResponse.json({
              success: true,
              message: 'Successfully transferred 1,000 PDOT tokens',
              transactionHash: namedTransferResult.hash,
              details: {
                transferTransactionHash: namedTransferResult.hash,
                amountTransferred: '1,000 PDOT',
                aliceBalanceBefore: aliceBalance,
                contractId: TOKEN_CONTRACT_ID,
                approach: 'named_parameters'
              }
            });
          } else {
            console.error('❌ APPROACH 6 FAILED:', namedTransferResult.errorResult);
          }
        } catch (namedError) {
          console.error('❌ APPROACH 6 EXCEPTION:', namedError);
        }

        // APPROACH 7: Try different function names that might exist
        console.log('\n--- APPROACH 7: Alternative Function Names ---');
        const alternativeFunctions = [
          'send',
          'send_to',
          'move_tokens',
          'transfer_to',
          'give',
          'allocate'
        ];

        for (const funcName of alternativeFunctions) {
          try {
            console.log(`Trying function: ${funcName}`);
            
            const altOperation = contract.call(funcName, transferFromAddress, transferToAddress, transferAmount);
            console.log(`${funcName} operation created`);

            const altTransaction = new StellarSdk.TransactionBuilder(aliceAccount, {
              fee: StellarSdk.BASE_FEE,
              networkPassphrase: StellarSdk.Networks.TESTNET,
            })
              .addOperation(altOperation)
              .setTimeout(30)
              .build();

            // Simulate first to avoid wasting fees
            const altSimResult = await rpc.simulateTransaction(altTransaction);
            console.log(`${funcName} simulation:`, StellarSdk.rpc.Api.isSimulationSuccess(altSimResult) ? 'SUCCESS' : 'FAILED');

            if (StellarSdk.rpc.Api.isSimulationSuccess(altSimResult)) {
              console.log(`✅ FOUND WORKING FUNCTION: ${funcName}`);
              
              const preparedAltTransaction = await rpc.prepareTransaction(altTransaction);
              preparedAltTransaction.sign(aliceKeypair);

              const altResult = await rpc.sendTransaction(preparedAltTransaction);
              console.log(`${funcName} result:`, JSON.stringify(altResult, null, 2));

              if (altResult.status !== 'ERROR') {
                console.log(`✅ TRANSFER SUCCESSFUL WITH ${funcName.toUpperCase()}`);
                return NextResponse.json({
                  success: true,
                  message: 'Successfully transferred 1,000 PDOT tokens',
                  transactionHash: altResult.hash,
                  details: {
                    transferTransactionHash: altResult.hash,
                    amountTransferred: '1,000 PDOT',
                    aliceBalanceBefore: aliceBalance,
                    contractId: TOKEN_CONTRACT_ID,
                    approach: `alternative_function_${funcName}`
                  }
                });
              }
            }
          } catch (altError) {
            console.log(`❌ ${funcName} failed:`, altError instanceof Error ? altError.message : String(altError));
          }
        }

        // APPROACH 8: Try calling the original exec command as a fallback
        console.log('\n--- APPROACH 8: Fallback to Exec Command ---');
        try {
          console.log('Attempting to use exec as fallback...');
          
          // Import exec functionality
          const { exec } = await import('child_process');
          const { promisify } = await import('util');
          const execAsync = promisify(exec);

          // Use the exact command that was working before
          const transferCommand = `stellar contract invoke \\
            --id ${TOKEN_CONTRACT_ID} \\
            --source alice \\
            --network testnet \\
            -- \\
            transfer \\
            --from ${ALICE_ADDRESS} \\
            --to ${userAddress} \\
            --amount ${requiredAmount}`;

          console.log('Executing transfer command:', transferCommand);
          const execResult = await execAsync(transferCommand);
          console.log('Exec transfer result:', execResult.stdout);

          // Parse transaction hash if available
          const hashMatch = execResult.stdout.match(/transaction: ([a-f0-9]+)/i);
          const transactionHash = hashMatch ? hashMatch[1] : 'completed';

          console.log('✅ TRANSFER SUCCESSFUL WITH EXEC FALLBACK');
          return NextResponse.json({
            success: true,
            message: 'Successfully transferred 1,000 PDOT tokens',
            transactionHash: transactionHash,
            details: {
              transferTransactionHash: transactionHash,
              amountTransferred: '1,000 PDOT',
              aliceBalanceBefore: aliceBalance,
              contractId: TOKEN_CONTRACT_ID,
              approach: 'exec_fallback',
              execOutput: execResult.stdout
            }
          });

        } catch (execError) {
          console.error('❌ APPROACH 8 EXCEPTION:', execError);
        }

        // APPROACH 9: Try with different approve parameters
        console.log('\n--- APPROACH 9: Different Approve Parameters ---');
        try {
          // Try approve with just 2 parameters (spender, amount)
          const approve2Operation = contract.call('approve', transferToAddress, transferAmount);
          console.log('Approve operation created (2 params)');

          const approve2Transaction = new StellarSdk.TransactionBuilder(aliceAccount, {
            fee: StellarSdk.BASE_FEE,
            networkPassphrase: StellarSdk.Networks.TESTNET,
          })
            .addOperation(approve2Operation)
            .setTimeout(30)
            .build();

          const approve2SimResult = await rpc.simulateTransaction(approve2Transaction);
          console.log('Approve simulation (2 params):', StellarSdk.rpc.Api.isSimulationSuccess(approve2SimResult) ? 'SUCCESS' : 'FAILED');

          if (StellarSdk.rpc.Api.isSimulationSuccess(approve2SimResult)) {
            console.log('✅ APPROVE SIMULATION SUCCESSFUL');
            
            const preparedApprove2 = await rpc.prepareTransaction(approve2Transaction);
            preparedApprove2.sign(aliceKeypair);

            const approve2Result = await rpc.sendTransaction(preparedApprove2);
            console.log('Approve result (2 params):', JSON.stringify(approve2Result, null, 2));

            if (approve2Result.status !== 'ERROR') {
              console.log('✅ APPROVE SUCCESSFUL, now trying transfer again...');
              
              // Try transfer again after approval
              await new Promise(resolve => setTimeout(resolve, 2000));
              
              const postApproveAccount = await rpc.getAccount(aliceKeypair.publicKey());
              const postApproveTransferTransaction = new StellarSdk.TransactionBuilder(postApproveAccount, {
                fee: StellarSdk.BASE_FEE,
                networkPassphrase: StellarSdk.Networks.TESTNET,
              })
                .addOperation(transferOperation)
                .setTimeout(30)
                .build();

              const preparedPostApprove = await rpc.prepareTransaction(postApproveTransferTransaction);
              preparedPostApprove.sign(aliceKeypair);

              const postApproveResult = await rpc.sendTransaction(preparedPostApprove);
              console.log('Post-approve transfer result:', JSON.stringify(postApproveResult, null, 2));

              if (postApproveResult.status !== 'ERROR') {
                console.log('✅ TRANSFER SUCCESSFUL AFTER 2-PARAM APPROVE');
                return NextResponse.json({
                  success: true,
                  message: 'Successfully transferred 1,000 PDOT tokens',
                  transactionHash: postApproveResult.hash,
                  details: {
                    approveTransactionHash: approve2Result.hash,
                    transferTransactionHash: postApproveResult.hash,
                    amountTransferred: '1,000 PDOT',
                    aliceBalanceBefore: aliceBalance,
                    contractId: TOKEN_CONTRACT_ID,
                    approach: 'approve_2_params_then_transfer'
                  }
                });
              }
            }
          }
        } catch (approve2Error) {
          console.error('❌ APPROACH 9 EXCEPTION:', approve2Error);
        }

        // If all approaches failed, return comprehensive error
        console.error('❌ ALL TRANSFER APPROACHES FAILED');
        return NextResponse.json(
          { 
            success: false, 
            error: 'All transfer approaches failed - contract transfer functions appear to have implementation issues',
            details: {
              aliceBalance: aliceBalance,
              requiredAmount: requiredAmount.toString(),
              contractId: TOKEN_CONTRACT_ID,
              fromAddress: ALICE_ADDRESS,
              toAddress: userAddress,
              approachesTried: [
                'with_authorization', 
                'different_parameter_order', 
                'transfer_from', 
                'simulation', 
                'approve_then_transfer',
                'named_parameters',
                'alternative_function_names',
                'exec_fallback',
                'approve_2_params'
              ],
              contractIssues: [
                'transfer function hits UnreachableCodeReached',
                'transfer_from function hits UnreachableCodeReached', 
                'approve function has wrong parameter count',
                'Suggests contract implementation issues or missing functions'
              ]
            }
          },
          { status: 500 }
        );

      } catch (transferError) {
        console.error('❌ TRANSFER SETUP EXCEPTION:', transferError);
        if (transferError instanceof Error) {
          console.error('Transfer error name:', transferError.name);
          console.error('Transfer error message:', transferError.message);
          console.error('Transfer error stack:', transferError.stack);
        }
        
        return NextResponse.json(
          { 
            success: false, 
            error: `Transfer setup failed: ${transferError}`,
            details: {
              error: transferError instanceof Error ? transferError.message : String(transferError),
              aliceBalance: aliceBalance,
              contractId: TOKEN_CONTRACT_ID,
              fromAddress: ALICE_ADDRESS,
              toAddress: userAddress
            }
          },
          { status: 500 }
        );
      }

    } catch (balanceError) {
      console.error('❌ BALANCE CHECK FAILED:', balanceError);
      return NextResponse.json(
        { 
          success: false, 
          error: `Failed to check balance: ${balanceError}`,
          details: {
            error: balanceError instanceof Error ? balanceError.message : String(balanceError),
            contractId: TOKEN_CONTRACT_ID,
            aliceAddress: ALICE_ADDRESS
          }
        },
        { status: 500 }
      );
    }

  } catch (error) {
    console.error('❌ GENERAL ERROR:', error);
    
    return NextResponse.json(
      { 
        success: false, 
        error: `Failed to process tokens: ${error instanceof Error ? error.message : String(error)}`,
        details: {
          error: error instanceof Error ? error.message : String(error),
          stack: error instanceof Error ? error.stack?.split('\n').slice(0, 10) : undefined,
          contractId: TOKEN_CONTRACT_ID,
          aliceAddress: ALICE_ADDRESS
        }
      },
      { status: 500 }
    );
  } finally {
    console.log('=== MINT TOKENS DEBUG END ===\n');
  }
} 