use soroban_env_common::xdr::{LedgerEntry, Limits, ReadXdr, StellarMessage, TransactionEnvelope};

pub fn simulate(msg: StellarMessage, envelope: TransactionEnvelope) {
    serde_json::to_string(&msg);

    let entry = LedgerEntry::from_xdr([9], Limits::none()).unwrap();

    serde_json::to_string(&entry);
}
