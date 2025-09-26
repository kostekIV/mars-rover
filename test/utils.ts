import { Account, BASE_FEE, Operation, rpc, Transaction, TransactionBuilder, xdr } from '@stellar/stellar-sdk';
import { Api } from '@stellar/stellar-sdk/lib/minimal/rpc/api';
import SimulateTransactionSuccessResponse = Api.SimulateTransactionSuccessResponse;
export class StellarRpcClient {
  constructor() {}

  async transactionFromOperation(
    operation: xdr.Operation<Operation.InvokeHostFunction>,
    get_account: () => Account,
    simulate: (x: Transaction) => SimulateTransactionSuccessResponse,
    passphrase: string,
    fee = BASE_FEE,
    timeout = 3000,
  ) {
    const tx = new TransactionBuilder(get_account(), {
      fee,
      networkPassphrase: passphrase,
    })
      .addOperation(operation)
      .setTimeout(timeout)
      .build();

    const sim = simulate(tx);

    return rpc.assembleTransaction(tx, sim).build();
  }
  //
  //
  // async getContractData<T>(
  //   contract: string | Address | Contract,
  //   key: xdr.ScVal,
  //   transform: (result: rpc.Api.LedgerEntryResult) => T,
  //   durability?: rpc.Durability
  // ) {
  //   return transform(await this.server.getContractData(contract, key, durability));
  // }
}
