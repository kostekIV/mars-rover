import { MarsRover } from '../index';
import { Account, Address, Contract, Keypair, Operation, Transaction, xdr } from '@stellar/stellar-sdk';
import { readFileSync } from 'fs';
import { StellarRpcClient } from './utils';
import { SorobanDataBuilder } from '@stellar/stellar-base';

describe('StellarSandbox', () => {
  it('Does something', async () => {
    const rover = new MarsRover();

    console.log(
      rover.sendTransaction(
        'AAAAAgAAAADMhyUr2DTDvFw70TSRmUhm52A7PuMt8uIOjFhC0uBuQAADJYEABOVfAAAABAAAAAEAAAAAAAAAAAAAAABo0rExAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABq4P5a+MLZ/WiVyampwIfs6crA21Ih8/p1VIFkMe4clcAAAAMY2hhbmdlX293bmVyAAAAAQAAABIAAAAAAAAAAPqS9Q/j4wXhAhrzZpNIu33tjelksUUC2T/fWnuxWO1pAAAAAQAAAAEAAAAAAAAAAPqS9Q/j4wXhAhrzZpNIu33tjelksUUC2T/fWnuxWO1pBztbQQm6H94AAAAAAAAAAQAAAAAAAAABq4P5a+MLZ/WiVyampwIfs6crA21Ih8/p1VIFkMe4clcAAAAMY2hhbmdlX293bmVyAAAAAQAAABIAAAAAAAAAAPqS9Q/j4wXhAhrzZpNIu33tjelksUUC2T/fWnuxWO1pAAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAA+pL1D+PjBeECGvNmk0i7fe2N6WSxRQLZP99ae7FY7WkAAAAHDOxN+5wG3QW5dPtODYSdkZ7trvqVPuHZRWiNsaFO32EAAAACAAAABgAAAAAAAAAA+pL1D+PjBeECGvNmk0i7fe2N6WSxRQLZP99ae7FY7WkAAAAVBztbQQm6H94AAAAAAAAABgAAAAGrg/lr4wtn9aJXJqanAh+zpysDbUiHz+nVUgWQx7hyVwAAABQAAAABABH2gwAAAJAAAAEcAAAAAAADJR0AAAAA',
      ),
    );
    expect(1).toBe(1);
  });
  it('Does something2', async () => {
    const rover = new MarsRover();

    console.log(rover.getNetworkInfo());
  });

  it('Does something3', async () => {
    const rover = new MarsRover();

    const accountId = Keypair.random();

    const key = accountId.xdrPublicKey().toXDR('base64');
    console.log(key);

    rover.fundAccount(key, 1000);

    console.log(rover.getBalance(key));
  });

  it('Does something4', async () => {
    const rover = new MarsRover();

    const accountId = Keypair.random();

    const key = accountId.xdrPublicKey().toXDR('base64');
    console.log(key);

    rover.fundAccount(key, 1000);

    console.log(rover.getAccount(key));
  });

  it('Does something5', async () => {
    const rover = new MarsRover();
    const accountId = Keypair.random();

    const key = accountId.xdrPublicKey().toXDR('base64');

    const x = readFileSync('./test/redstone_adapter.wasm');

    console.log(rover.deployCode(key, Array.from(x)));
  });

  it.only('Does something6', async () => {
    const rover = new MarsRover();

    const accountId = Keypair.random();

    const key = accountId.xdrPublicKey().toXDR('base64');

    rover.fundAccount(key, 1_000_000_000);
    const x = readFileSync('./test/redstone_adapter.wasm');

    const hash = rover.deployCode(key, Array.from(x));

    // console.log(hash);
    // console.log(Buffer.from(hash, 'base64'));

    const xx = xdr.ScBytes.fromXDR(hash, 'base64');

    const rpc = new StellarRpcClient();

    const account: {
      account_id: string;
      seq_num: string;
    } = JSON.parse(rover.getAccount(key));

    // console.dir(account);

    const ac = new Account(account.account_id, account.seq_num);

    const simulateTx = (tx: Transaction) => {
      const simulation = JSON.parse(rover.simulateTx(tx.toEnvelope().toXDR('base64')));

      console.dir(simulation, { depth: 100 });

      simulation.transactionData = new SorobanDataBuilder(
        xdr.SorobanTransactionData.fromXDR(simulation.transactionData, 'base64'),
      );

      simulation.result.auth = simulation.result.auth.map((a: string) =>
        xdr.SorobanAuthorizationEntry.fromXDR(a, 'base64'),
      );

      return simulation;
    };

    const a = await rpc.transactionFromOperation(
      Operation.createCustomContract({
        wasmHash: xx,
        address: Address.fromString(accountId.publicKey()),
      }),
      () => ac,
      simulateTx,
      'mars-rover; sandbox environment',
    );

    //a.sign(accountId);
    const p = rover.sendTransaction(a.toEnvelope().toXDR('base64'));

    const buffer = Buffer.from(p);
    const val = xdr.ScVal.fromXDR(buffer);
    const address = Address.fromScVal(val);
    console.log(address);

    const contract = new Contract(address.toString());

    let b = await rpc.transactionFromOperation(
      contract.call('init', xdr.ScVal.scvAddress(new Address(accountId.publicKey()).toScAddress())),
      () => ac,
      simulateTx,
      'mars-rover; sandbox environment',
    );
    console.log(b);
    b.sign(accountId);

    const p2 = rover.sendTransaction(b.toEnvelope().toXDR('base64'));

    console.log(p2);
    let c = await rpc.transactionFromOperation(
      contract.call('change_owner', xdr.ScVal.scvAddress(new Address(accountId.publicKey()).toScAddress())),
      () => ac,
      simulateTx,
      'mars-rover; sandbox environment',
    );
    console.log(c);
    // c.sign(accountId);

    const p3 = rover.sendTransaction(c.toEnvelope().toXDR('base64'));

    console.log(p3);
  });

  it('Does something7', async () => {
    const rover = new MarsRover();

    const accountId = Keypair.random();

    const key = accountId.xdrPublicKey().toXDR('base64');

    rover.fundAccount(key, 1_000_000_000);
    const x = readFileSync('./test/redstone_adapter.wasm');

    const rpc = new StellarRpcClient();

    const account: {
      account_id: string;
      seq_num: string;
    } = JSON.parse(rover.getAccount(key));

    console.dir(account);

    const ac = new Account(account.account_id, account.seq_num);

    const p = await rpc.transactionFromOperation(
      Operation.uploadContractWasm({
        wasm: x,
      }),
      () => ac,
      (tx) => JSON.parse(rover.simulateTx(tx.toEnvelope().toXDR('base64'))),
      'mars-rover; sandbox environment',
    );

    console.log(p);
  });
});
