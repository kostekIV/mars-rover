import { MarsRover } from '../index';
import { Account, Address, Contract, Keypair, Operation, Transaction, xdr } from '@stellar/stellar-sdk';
import { readFileSync } from 'fs';
import { StellarRpcClient } from './utils';
import { SorobanDataBuilder } from '@stellar/stellar-base';

describe('MarsRover Stellar Sandbox', () => {
  let rover: MarsRover;
  let rpc: StellarRpcClient;

  beforeEach(() => {
    rover = new MarsRover();
    rpc = new StellarRpcClient();
  });

  const createFundedAccount = (balance: number = 1_000_000_000): { keypair: Keypair; accountKey: string } => {
    const keypair = Keypair.random();
    const accountKey = keypair.xdrPublicKey().toXDR('base64');
    rover.fundAccount(accountKey, balance);
    return { keypair, accountKey };
  };

  const getAccountProvider = (accountKey: string) => () => {
    const accountData: { account_id: string; seq_num: string } = JSON.parse(rover.getAccount(accountKey));
    return new Account(accountData.account_id, accountData.seq_num);
  };

  const simulateTransaction = (tx: Transaction) => {
    const simulation = JSON.parse(rover.simulateTx(tx.toEnvelope().toXDR('base64')));

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
  };

  const executeTransaction = (tx: Transaction, signerKeypair: Keypair): Buffer => {
    tx.sign(signerKeypair);
    const result = rover.sendTransaction(tx.toEnvelope().toXDR('base64'));
    return Buffer.from(result);
  };

  const decodeScVal = (buffer: Buffer): xdr.ScVal => {
    return xdr.ScVal.fromXDR(buffer);
  };

  describe('Basic Operations', () => {
    it('should handle transaction without funded account (expect failure)', () => {
      const invalidTxXdr =
        'AAAAAgAAAADMhyUr2DTDvFw70TSRmUhm52A7PuMt8uIOjFhC0uBuQAADJYEABOVfAAAABAAAAAEAAAAAAAAAAAAAAABo0rExAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABq4P5a+MLZ/WiVyampwIfs6crA21Ih8/p1VIFkMe4clcAAAAMY2hhbmdlX293bmVyAAAAAQAAABIAAAAAAAAAAPqS9Q/j4wXhAhrzZpNIu33tjelksUUC2T/fWnuxWO1pAAAAAQAAAAEAAAAAAAAAAPqS9Q/j4wXhAhrzZpNIu33tjelksUUC2T/fWnuxWO1pBztbQQm6H94AAAAAAAAAAQAAAAAAAAABq4P5a+MLZ/WiVyampwIfs6crA21Ih8/p1VIFkMe4clcAAAAMY2hhbmdlX293bmVyAAAAAQAAABIAAAAAAAAAAPqS9Q/j4wXhAhrzZpNIu33tjelksUUC2T/fWnuxWO1pAAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAA+pL1D+PjBeECGvNmk0i7fe2N6WSxRQLZP99ae7FY7WkAAAAHDOxN+5wG3QW5dPtODYSdkZ7trvqVPuHZRWiNsaFO32EAAAACAAAABgAAAAAAAAAA+pL1D+PjBeECGvNmk0i7fe2N6WSxRQLZP99ae7FY7WkAAAAVBztbQQm6H94AAAAAAAAABgAAAAGrg/lr4wtn9aJXJqanAh+zpysDbUiHz+nVUgWQx7hyVwAAABQAAAABABH2gwAAAJAAAAEcAAAAAAADJR0AAAAA';

      expect(() => {
        rover.sendTransaction(invalidTxXdr);
      }).toThrow();
    });

    it('xxx', () => {
      console.dir(rover.getLedgerInfo());
    });

    it('should return network information', () => {
      const networkInfo = rover.getNetworkInfo();
      const parsedInfo = JSON.parse(networkInfo);

      expect(parsedInfo).toHaveProperty('passphrase');
      expect(parsedInfo).toHaveProperty('protocolVersion');
    });

    it('should fund account and retrieve balance', () => {
      const { accountKey } = createFundedAccount(1000);
      const balance = Number(rover.getBalance(accountKey));

      expect(balance).toBe(1000);
    });

    it('should fund account and retrieve account details', () => {
      const { accountKey } = createFundedAccount(1000);
      const accountInfo = rover.getAccount(accountKey);
      const parsedAccount = JSON.parse(accountInfo);

      expect(parsedAccount).toHaveProperty('account_id');
      expect(parsedAccount).toHaveProperty('seq_num');
      expect(parsedAccount.balance).toBe('1000');
    });
  });

  describe('Contract Operations', () => {
    let contractWasm: Buffer;

    beforeAll(() => {
      contractWasm = readFileSync('./test/redstone_adapter.wasm');
    });

    it('should deploy contract and execute operations', async () => {
      const { keypair: ownerKeypair, accountKey: ownerAccountKey } = createFundedAccount();
      const { keypair: userKeypair, accountKey: userAccountKey } = createFundedAccount();

      const getOwnerAccount = getAccountProvider(ownerAccountKey);
      const getUserAccount = getAccountProvider(userAccountKey);

      const uploadTx = await rpc.transactionFromOperation(
        Operation.uploadContractWasm({ wasm: contractWasm }),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const uploadResult = executeTransaction(uploadTx, ownerKeypair);
      const wasmHashScVal = decodeScVal(uploadResult);
      const wasmHash = wasmHashScVal.bytes();

      expect(wasmHash).toBeInstanceOf(Buffer);
      expect(wasmHash.length).toBe(32);

      const createContractTx = await rpc.transactionFromOperation(
        Operation.createCustomContract({
          wasmHash,
          address: Address.fromString(ownerKeypair.publicKey()),
        }),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const createResult = executeTransaction(createContractTx, ownerKeypair);
      const contractAddressScVal = decodeScVal(createResult);
      const contractAddress = Address.fromScVal(contractAddressScVal);
      const contract = new Contract(contractAddress.toString());

      expect(contractAddress.toString()).toMatch(/^C[A-Z0-9]{55}$/);

      const initTx = await rpc.transactionFromOperation(
        contract.call('init', xdr.ScVal.scvAddress(new Address(ownerKeypair.publicKey()).toScAddress())),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      executeTransaction(initTx, ownerKeypair);

      const changeOwnerTx = await rpc.transactionFromOperation(
        contract.call('change_owner', xdr.ScVal.scvAddress(new Address(ownerKeypair.publicKey()).toScAddress())),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      executeTransaction(changeOwnerTx, ownerKeypair);

      const unauthorizedChangeOwnerTx = await rpc.transactionFromOperation(
        contract.call('change_owner', xdr.ScVal.scvAddress(new Address(ownerKeypair.publicKey()).toScAddress())),
        getUserAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      expect(() => {
        executeTransaction(unauthorizedChangeOwnerTx, userKeypair);
      }).toThrow();
    });

    it('should handle contract deployment with proper WASM hash format', async () => {
      const { keypair, accountKey } = createFundedAccount();
      const getAccount = getAccountProvider(accountKey);

      const uploadTx = await rpc.transactionFromOperation(
        Operation.uploadContractWasm({ wasm: contractWasm }),
        getAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const result = executeTransaction(uploadTx, keypair);
      const wasmHashScVal = decodeScVal(result);

      expect(wasmHashScVal.switch()).toBe(xdr.ScValType.scvBytes());
      expect(wasmHashScVal.bytes().length).toBe(32);
    });

    it('should handle contract address creation properly', async () => {
      const { keypair, accountKey } = createFundedAccount();
      const getAccount = getAccountProvider(accountKey);

      const uploadTx = await rpc.transactionFromOperation(
        Operation.uploadContractWasm({ wasm: contractWasm }),
        getAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const uploadResult = executeTransaction(uploadTx, keypair);
      const wasmHashScVal = decodeScVal(uploadResult);
      const wasmHash = wasmHashScVal.bytes();

      const createTx = await rpc.transactionFromOperation(
        Operation.createCustomContract({
          wasmHash,
          address: Address.fromString(keypair.publicKey()),
        }),
        getAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const createResult = executeTransaction(createTx, keypair);
      const contractAddressScVal = decodeScVal(createResult);

      expect(contractAddressScVal.switch()).toBe(xdr.ScValType.scvAddress());

      const contractAddress = Address.fromScVal(contractAddressScVal);
      expect(contractAddress.toString()).toMatch(/^C[A-Z0-9]{55}$/);
    });

    it('should fail when simulating non-existing contract function', async () => {
      const { keypair: ownerKeypair, accountKey: ownerAccountKey } = createFundedAccount();
      const getOwnerAccount = getAccountProvider(ownerAccountKey);

      const uploadTx = await rpc.transactionFromOperation(
        Operation.uploadContractWasm({ wasm: contractWasm }),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const uploadResult = executeTransaction(uploadTx, ownerKeypair);
      const wasmHashScVal = decodeScVal(uploadResult);
      const wasmHash = wasmHashScVal.bytes();

      const createContractTx = await rpc.transactionFromOperation(
        Operation.createCustomContract({
          wasmHash,
          address: Address.fromString(ownerKeypair.publicKey()),
        }),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const createResult = executeTransaction(createContractTx, ownerKeypair);
      const contractAddressScVal = decodeScVal(createResult);
      const contractAddress = Address.fromScVal(contractAddressScVal);
      const contract = new Contract(contractAddress.toString());

      const initTx = await rpc.transactionFromOperation(
        contract.call('init', xdr.ScVal.scvAddress(new Address(ownerKeypair.publicKey()).toScAddress())),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      executeTransaction(initTx, ownerKeypair);

      expect(async () => {
        await rpc.transactionFromOperation(
          contract.call('nonExistentFunction', xdr.ScVal.scvString('test')),
          getOwnerAccount,
          simulateTransaction,
          rover.networkPassphrase(),
        );
      }).toThrow();
    });
  });

  describe('Error Handling', () => {
    it('should handle invalid account keys', () => {
      expect(() => {
        rover.fundAccount('invalid-key', 1000);
      }).toThrow();
    });

    it('should handle requests for non-existent accounts', () => {
      const randomKey = Keypair.random().xdrPublicKey().toXDR('base64');

      expect(() => {
        rover.getBalance(randomKey);
      }).toThrow();
    });

    it('should handle malformed transaction XDR', () => {
      expect(() => {
        rover.sendTransaction('invalid-xdr');
      }).toThrow();
    });
  });

  describe('Authorization', () => {
    it.only('should reject unauthorized operations', async () => {
      const { keypair: ownerKeypair, accountKey: ownerAccountKey } = createFundedAccount();
      const { keypair: unauthorizedKeypair, accountKey: unauthorizedAccountKey } = createFundedAccount();

      const getOwnerAccount = getAccountProvider(ownerAccountKey);
      const getUnauthorizedAccount = getAccountProvider(unauthorizedAccountKey);

      const uploadTx = await rpc.transactionFromOperation(
        Operation.uploadContractWasm({ wasm: readFileSync('./test/redstone_adapter.wasm') }),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const uploadResult = executeTransaction(uploadTx, ownerKeypair);
      const wasmHashScVal = decodeScVal(uploadResult);
      const wasmHash = wasmHashScVal.bytes();

      const createTx = await rpc.transactionFromOperation(
        Operation.createCustomContract({
          wasmHash,
          address: Address.fromString(ownerKeypair.publicKey()),
        }),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      const createResult = executeTransaction(createTx, ownerKeypair);
      const contractAddressScVal = decodeScVal(createResult);
      const contractAddress = Address.fromScVal(contractAddressScVal);
      const contract = new Contract(contractAddress.toString());

      const initTx = await rpc.transactionFromOperation(
        contract.call('init', xdr.ScVal.scvAddress(new Address(ownerKeypair.publicKey()).toScAddress())),
        getOwnerAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      executeTransaction(initTx, ownerKeypair);

      const unauthorizedTx = await rpc.transactionFromOperation(
        contract.call('change_owner', xdr.ScVal.scvAddress(new Address(unauthorizedKeypair.publicKey()).toScAddress())),
        getUnauthorizedAccount,
        simulateTransaction,
        rover.networkPassphrase(),
      );

      executeTransaction(unauthorizedTx, unauthorizedKeypair);

      expect(() => {
        executeTransaction(unauthorizedTx, unauthorizedKeypair);
      }).toThrow();
    });
  });
});
