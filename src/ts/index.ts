import { Transaction } from '@stellar/stellar-sdk';
import { rpc } from '@stellar/stellar-sdk/';

export * from '../../index';

export async function operation(server: rpc.Server, tx: Transaction) {
  server.sendTransaction(tx);
}
