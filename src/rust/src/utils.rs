use sha2::{Digest, Sha256};
use soroban_env_common::xdr::{
    Hash, Limits, TransactionSignaturePayload, TransactionSignaturePayloadTaggedTransaction,
    TransactionV1Envelope,
};
use soroban_env_host::{xdr::WriteXdr, LedgerInfo};

pub fn tx_hash(
    envelope: &TransactionV1Envelope,
    ledger_info: &LedgerInfo,
) -> anyhow::Result<[u8; 32]> {
    let payload = TransactionSignaturePayload {
        network_id: Hash(ledger_info.network_id),
        tagged_transaction: TransactionSignaturePayloadTaggedTransaction::Tx(envelope.tx.clone()),
    };

    let payload = payload.to_xdr(Limits::none())?;

    Ok(Sha256::digest(&payload).into())
}
