import {
  Account,
  Contract,
  Keypair,
  TransactionBuilder,
  BASE_FEE,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

import { sleep } from './utils.js';

const DUMMY_SOURCE = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';

export class SorobanClient {
  private readonly dummyAccount = new Account(DUMMY_SOURCE, '0');

  constructor(private readonly server: rpc.Server, private readonly networkPassphrase: string) {}

  async call<T>(contractId: string, method: string, args: xdr.ScVal[]): Promise<T> {
    const contract = new Contract(contractId);
    const op = contract.call(method, ...args);

    const tx = new TransactionBuilder(this.dummyAccount, {
      fee: BASE_FEE,
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(op)
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (!rpc.Api.isSimulationSuccess(sim)) {
      const message = sim.error || 'simulation failed';
      throw new Error(`Simulation error calling ${method}: ${message}`);
    }
    return scValToNative(sim.result?.retval) as T;
  }

  async invoke(
    signer: Keypair,
    contractId: string,
    method: string,
    args: xdr.ScVal[],
  ): Promise<rpc.Api.GetTransactionResponse> {
    const accountResponse = await this.server.getAccount(signer.publicKey());
    const sequence =
      typeof accountResponse.sequence === 'string'
        ? accountResponse.sequence
        : accountResponse.sequence.toString();
    const account = new Account(accountResponse.accountId, sequence);
    const contract = new Contract(contractId);
    const op = contract.call(method, ...args);

    let tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(op)
      .setTimeout(30)
      .build();

    tx = await this.server.prepareTransaction(tx);
    tx.sign(signer);

    const send = await this.server.sendTransaction(tx);
    if (send.status === rpc.Api.SendTransactionStatus.ERROR) {
      throw new Error(`sendTransaction failed: ${send.errorResultXdr}`);
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
        throw new Error(`transaction ${hash} failed: ${res.resultXdr}`);
      }
      await sleep(1000);
    }
    throw new Error(`transaction ${hash} not confirmed within timeout`);
  }
}
