import {
  Account,
  Contract,
  Keypair,
  TransactionBuilder,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

import { sleep } from './utils.js';

// 500_000 stroops = 0.05 XLM; keeps mainnet transactions off the floor.
const INCLUSION_FEE = '500000';

export class SorobanClient {
  constructor(private readonly server: rpc.Server, private readonly networkPassphrase: string) {}

  async call<T>(contractId: string, method: string, args: xdr.ScVal[]): Promise<T> {
    const contract = new Contract(contractId);
    const op = contract.call(method, ...args);

    // Use a dummy account for read-only simulation — sequence number is irrelevant.
    const dummy = new Account('GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF', '0');
    const tx = new TransactionBuilder(dummy, {
      fee: INCLUSION_FEE,
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(op)
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (!rpc.Api.isSimulationSuccess(sim)) {
      throw new Error(`Simulation error calling ${method}: ${sim.error ?? 'unknown'}`);
    }
    if (!sim.result) {
      throw new Error(`No result returned from simulation of ${method}`);
    }
    return scValToNative(sim.result.retval) as T;
  }

  async invoke(
    signer: Keypair,
    contractId: string,
    method: string,
    args: xdr.ScVal[],
  ): Promise<rpc.Api.GetTransactionResponse> {
    // getAccount returns a ready-to-use Account with the current sequence number.
    const account = await this.server.getAccount(signer.publicKey());
    const contract = new Contract(contractId);
    const op = contract.call(method, ...args);

    let tx = new TransactionBuilder(account, {
      fee: INCLUSION_FEE,
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(op)
      .setTimeout(30)
      .build();

    // prepareTransaction fills in Soroban resource fees and auth entries.
    tx = await this.server.prepareTransaction(tx);
    tx.sign(signer);

    const send = await this.server.sendTransaction(tx);
    if (send.status === 'ERROR') {
      throw new Error(`sendTransaction failed: ${send.errorResult?.toXDR('base64') ?? 'unknown'}`);
    }

    return this.awaitFinality(send.hash);
  }

  private async awaitFinality(
    hash: string,
    timeoutMs = 120_000,
  ): Promise<rpc.Api.GetTransactionResponse> {
    const started = Date.now();
    while (Date.now() - started < timeoutMs) {
      const res = await this.server.getTransaction(hash);
      if (res.status === rpc.Api.GetTransactionStatus.SUCCESS) {
        return res;
      }
      if (res.status === rpc.Api.GetTransactionStatus.FAILED) {
        throw new Error(`transaction ${hash} failed`);
      }
      await sleep(1000);
    }
    throw new Error(`transaction ${hash} not confirmed within timeout`);
  }
}
