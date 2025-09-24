import { MarsRover } from '../index';
import { Keypair } from '@stellar/stellar-sdk';
import { readFileSync } from 'fs';

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

  it.only('Does something5', async () => {
    const rover = new MarsRover();
    const accountId = Keypair.random();

    const key = accountId.xdrPublicKey().toXDR('base64');

    const x = readFileSync('./test/redstone_adapter.wasm');

    console.log(rover.deployCode(key, Array.from(x)));
  });
});
