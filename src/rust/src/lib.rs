use crate::executor::Executor;
use crate::ledger_info::{get_initial_ledger_info, NETWORK_PASSPHRASE};
use crate::memory::Memory;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Serialize;
use soroban_env_common::xdr::{
    AccountEntry, AccountEntryExt, AccountId, LedgerEntry, LedgerEntryData, LedgerKey,
    LedgerKeyAccount, Limits, ReadXdr, SequenceNumber, String32, Thresholds, TransactionEnvelope,
};
use soroban_env_host::{
    budget::Budget,
    e2e_invoke::{self, RecordingInvocationAuthMode},
    e2e_testutils::{ledger_entry, upload_wasm_host_fn},
    xdr::{ContractCostParams, ScVal, WriteXdr},
    LedgerInfo,
};
use std::rc::Rc;
use soroban_env_host::storage::SnapshotSource;

mod executor;
mod ledger_info;
mod memory;
mod model;
mod network_config;
mod module_cache;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkInfo {
    passphrase: String,
    protocol_version: String,
}

#[napi]
pub struct MarsRover {
    memory: Rc<Memory>,
    ledger_info: LedgerInfo,
    executor: Executor,
}

#[napi]
impl MarsRover {
    #[napi(constructor)]
    pub fn new() -> Self {
        let memory = Rc::new(Memory::default());
        let ledger_info = get_initial_ledger_info();
        let executor = Executor::new(memory.clone(), ledger_info.clone());

        Self {
            memory,
            ledger_info,
            executor,
        }
    }

    #[napi]
    pub fn fund_account(&self, account: String, balance: i64) {
        let account = AccountEntry {
            account_id: AccountId::from_xdr_base64(account, Limits::none()).unwrap(),
            balance,
            seq_num: SequenceNumber::from(0),
            inflation_dest: None,
            ext: AccountEntryExt::V0,
            flags: 0,
            home_domain: String32::default(),
            thresholds: Thresholds([1, 0, 0, 0]),
            signers: vec![].try_into().unwrap(),
            num_sub_entries: 0,
        };

        let entry = ledger_entry(LedgerEntryData::Account(account));
        self.memory.insert(entry);
    }

    #[napi]
    pub fn get_account(&self, account: String) -> Result<String> {
        let account_id = AccountId::from_xdr_base64(account.clone(), Limits::none()).
            map_err(|e| Error::from_reason(format!("not an account: {}", e)))?;
        let key = LedgerKey::from(LedgerKeyAccount { account_id });
        let entry = self.memory.get(&Rc::new(key)).unwrap().unwrap().0;

        Ok(match &entry.data {
            LedgerEntryData::Account(account_entry) => {
                serde_json::to_string(&account_entry).unwrap()
            }
            _ => return Err(Error::from_reason(format!("not an account: {account}"))),
        })
    }

    #[napi]
    pub fn get_balance(&self, account: String) -> i64 {
        let account_id = AccountId::from_xdr_base64(account, Limits::none()).unwrap();
        let key = LedgerKey::from(LedgerKeyAccount { account_id });
        let entry = self.memory.get(&Rc::new(key)).unwrap().unwrap().0;

        match &entry.data {
            LedgerEntryData::Account(account_entry) => account_entry.balance,
            _ => panic!("not an account"),
        }
    }

    #[napi]
    pub fn deploy_code(&self, account_id: String, code: Vec<u8>) -> String {
        let host_fn = upload_wasm_host_fn(&code);
        let source_account = AccountId::from_xdr_base64(account_id, Limits::none()).unwrap();

        let unlimited_budget = Budget::try_from_configs(
            u64::MAX,
            u64::MAX,
            ContractCostParams(vec![].try_into().unwrap()),
            ContractCostParams(vec![].try_into().unwrap()),
        )
            .unwrap();

        let mut diagnostic_events = vec![];
        let result = e2e_invoke::invoke_host_function_in_recording_mode(
            &unlimited_budget,
            true,
            &host_fn,
            &source_account,
            RecordingInvocationAuthMode::Recording(true),
            self.ledger_info.clone(),
            self.memory.clone(),
            [0; 32],
            &mut diagnostic_events,
        )
            .unwrap();

        let hash = match result.invoke_result {
            Ok(ScVal::Bytes(bytes)) => bytes.to_xdr_base64(Limits::none()).unwrap(),
            Ok(val) => panic!("Unexpected result type: {:?}", val),
            Err(e) => panic!("Failed to deploy code: {:?}", e),
        };

        let invoke_result = self.executor
            .invoke_host_function(
                &host_fn,
                &result.resources,
                &source_account,
                vec![],
                &result.restored_rw_entry_indices,
                [0; 32],
                true,
            )
            .unwrap();

        self.executor.apply_ledger_changes(invoke_result.ledger_changes).unwrap();

        hash
    }

    #[napi]
    pub fn simulate_tx(&self, transaction_envelope: String) -> String {
        let te = TransactionEnvelope::from_xdr_base64(&transaction_envelope, Limits::none()).unwrap();

        let response = self.executor.simulate_transaction(te).unwrap();
        serde_json::to_string(&response).unwrap()
    }

    #[napi]
    pub fn send_transaction(&self, transaction_envelope: String) -> Vec<u8> {
        let te = TransactionEnvelope::from_xdr_base64(&transaction_envelope, Limits::none()).unwrap();

        self.executor.send_transaction(te).unwrap().unwrap()
    }

    #[napi]
    pub fn network_passphrase(&self) -> String {
        NETWORK_PASSPHRASE.to_string()
    }

    #[napi]
    pub fn get_network_info(&self) -> String {
        serde_json::to_string(&NetworkInfo {
            passphrase: NETWORK_PASSPHRASE.to_string(),
            protocol_version: self.ledger_info.protocol_version.to_string(),
        })
            .unwrap()
    }
}