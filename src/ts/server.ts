import { rpc } from '@stellar/stellar-sdk/';
import { MarsRover } from '../../index';
import { Account, Address, Contract, FeeBumpTransaction, Transaction, xdr } from '@stellar/stellar-sdk';
import { Api } from '@stellar/stellar-sdk/lib/minimal/rpc/api';
import { SorobanDataBuilder } from '@stellar/stellar-base';

export class SandboxServer extends rpc.Server {
  constructor(private readonly sandbox: MarsRover) {
    super('NA');
  }

  override getAccount(address: string): Promise<Account> {
    const accountData: { account_id: string; seq_num: string } = JSON.parse(this.sandbox.getAccount(address));

    return Promise.resolve(new Account(accountData.account_id, accountData.seq_num));
  }

  override getNetwork(): Promise<Api.GetNetworkResponse> {
    return Promise.resolve(JSON.parse(this.sandbox.getNetworkInfo()));
  }

  override async simulateTransaction(
    tx: Transaction | FeeBumpTransaction,
    _addlResources?: rpc.Server.ResourceLeeway,
    _authMode?: Api.SimulationAuthMode,
  ): Promise<Api.SimulateTransactionResponse> {
    const simulation = JSON.parse(this.sandbox.simulateTx(tx.toEnvelope().toXDR('base64')));

    if ('error' in simulation) {
      return simulation;
    }

    simulation.transactionData = new SorobanDataBuilder(
      xdr.SorobanTransactionData.fromXDR(simulation.transactionData, 'base64'),
    );

    simulation.result.auth = simulation.result.auth.map((authEntry: string) =>
      xdr.SorobanAuthorizationEntry.fromXDR(authEntry, 'base64'),
    );

    return simulation;
  }

  override async getContractData(
    contract: string | Address | Contract,
    key: xdr.ScVal,
    durability = rpc.Durability.Persistent,
  ): Promise<Api.LedgerEntryResult> {
    let contractAddress: Address;

    if (typeof contract === 'string') {
      contractAddress = Address.fromString(contract);
    } else if (contract instanceof Contract) {
      contractAddress = Address.fromString(contract.contractId());
    } else {
      contractAddress = contract;
    }

    const responseJson = this.sandbox.getContractData(
      contractAddress.toScAddress().toXDR('base64'),
      key.toXDR('base64'),
      durability,
    );

    return await Promise.resolve(JSON.parse(responseJson));
  }

  // dont for now
  override sendTransaction(transaction: Transaction): Promise<Api.SendTransactionResponse> {
    return super.sendTransaction(transaction);
  }

  // this also dont
  override getTransaction(hash: string): Promise<Api.GetTransactionResponse> {
    return super.getTransaction(hash);
  }
}
