use crate::model::SimulateHostFunctionResult;
use crate::network_config::default_network_config;
use crate::{
    ledger_info::{get_initial_ledger_info, NETWORK_PASSPHRASE},
    memory::Memory,
    model::{
        SimulateTransactionErrorResponse, SimulateTransactionResponse,
        SimulateTransactionSuccessResponse,
    },
    module_cache::new_module_cache,
    network_config::populate_memory_with_config_entries,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use soroban_env_host::xdr::{ScVal, TransactionExt};
use std::collections::HashSet;

use anyhow::Context;
use serde::Serialize;
use sha2::{Digest, Sha256};
use soroban_env_common::xdr::DiagnosticEvent;
use soroban_env_common::xdr::LedgerEntryChangeType;
use soroban_env_common::xdr::{
    AccountEntry, AccountEntryExt, AccountId, ContractCostParams, HostFunction, LedgerEntry,
    LedgerEntryData, LedgerKey, LedgerKeyAccount, Limits, OperationBody, ReadXdr, SequenceNumber,
    String32, Thresholds, TransactionEnvelope, VecM,
};
use soroban_env_host::{
    budget::AsBudget,
    budget::Budget,
    e2e_invoke::InvokeHostFunctionResult,
    e2e_invoke::{self, RecordingInvocationAuthMode},
    e2e_invoke::{InvokeHostFunctionRecordingModeResult, LedgerEntryChange},
    e2e_testutils::ledger_entry,
    e2e_testutils::upload_wasm_host_fn,
    fees::{compute_rent_fee, compute_transaction_resource_fee, FeeConfiguration},
    storage::SnapshotSource,
    vm::VersionedContractCodeCostInputs,
    xdr::LedgerKeyContractCode,
    xdr::LedgerKeyContractData,
    xdr::SorobanResources,
    xdr::WriteXdr,
    xdr::{
        ContractCodeEntryExt, ContractCostType, Hash, SorobanAuthorizationEntry,
        SorobanResourcesExtV0, SorobanTransactionData, SorobanTransactionDataExt, TtlEntry,
    },
    HostError, LedgerInfo, ModuleCache,
};
use soroban_simulation::simulation::LedgerEntryDiff;
use soroban_simulation::{simulation::SimulationAdjustmentConfig, NetworkConfig};
use std::rc::Rc;

mod ledger_info;
mod memory;
mod model;
mod module_cache;
mod network_config;
mod simulation;
mod executor;

pub fn create_budget(config: &NetworkConfig) -> Budget {
    let cpu_shadow_limit = (config.tx_max_instructions as u64).saturating_mul(10);
    let mem_shadow_limit = (config.tx_memory_limit as u64).saturating_mul(2);
    Budget::try_from_configs_with_shadow_limits(
        config.tx_max_instructions as u64,
        config.tx_memory_limit as u64,
        cpu_shadow_limit,
        mem_shadow_limit,
        config.cpu_cost_params.clone(),
        config.memory_cost_params.clone(),
    )
    .unwrap()
}

fn compute_key_hash(key: &LedgerKey) -> Vec<u8> {
    let key_xdr = key.to_xdr(Limits::none()).unwrap();
    let hash: [u8; 32] = Sha256::digest(&key_xdr).into();
    hash.to_vec()
}

pub fn sha256_hash_from_bytes_raw(bytes: &[u8], budget: impl AsBudget) -> Result<[u8; 32], String> {
    budget
        .as_budget()
        .charge(
            ContractCostType::ComputeSha256Hash,
            Some(bytes.len() as u64),
        )
        .unwrap();
    Ok(Sha256::digest(bytes).into())
}

fn ttl_entry(key: &LedgerKey, ttl: u32) -> TtlEntry {
    TtlEntry {
        key_hash: compute_key_hash(key).try_into().unwrap(),
        live_until_ledger_seq: ttl,
    }
}

fn build_module_cache_for_entries(
    ledger_info: &LedgerInfo,
    ledger_entries_with_ttl: Vec<(LedgerEntry, Option<u32>)>,
    restored_contracts: &HashSet<Hash>,
) -> Result<ModuleCache, String> {
    let (cache, ctx) = new_module_cache().unwrap();

    for (e, _) in ledger_entries_with_ttl.iter() {
        if let LedgerEntryData::ContractCode(cd) = &e.data {
            let contract_id = Hash(sha256_hash_from_bytes_raw(&cd.code, ctx.as_budget())?);
            // Restored contracts are not yet in the module cache and need to be
            // compiled during execution.
            if restored_contracts.contains(&contract_id) {
                continue;
            }
            let code_cost_inputs = match &cd.ext {
                ContractCodeEntryExt::V0 => VersionedContractCodeCostInputs::V0 {
                    wasm_bytes: cd.code.len(),
                },
                ContractCodeEntryExt::V1(v1) => {
                    VersionedContractCodeCostInputs::V1(v1.cost_inputs.clone())
                }
            };
            cache
                .parse_and_cache_module(
                    &ctx,
                    ledger_info.protocol_version,
                    &contract_id,
                    &cd.code,
                    code_cost_inputs,
                )
                .unwrap();
        }
    }
    Ok(cache)
}

fn changes_from_simulation(changes: Vec<LedgerEntryDiff>) -> Vec<model::LedgerEntryChange> {
    changes
        .into_iter()
        .map(|diff| {
            let change_type = match (diff.state_before.is_some(), diff.state_after.is_some()) {
                (true, true) => LedgerEntryChangeType::Updated,
                (true, false) => LedgerEntryChangeType::Removed,
                (false, true) => LedgerEntryChangeType::Updated,
                _ => panic!("unexpected"),
            };

            let key = match (&diff.state_after, &diff.state_before) {
                (Some(entry), _) => entry.to_key(),
                (_, Some(entry)) => entry.to_key(),
                _ => panic!("unexpected"),
            };

            model::LedgerEntryChange {
                change_type,
                key,
                before: diff.state_before,
                after: diff.state_after,
            }
        })
        .collect()
}

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
    pub fn get_account(&self, account: String) -> String {
        let account_id = AccountId::from_xdr_base64(account, Limits::none()).unwrap();
        let key = LedgerKey::from(LedgerKeyAccount { account_id });
        let entry = self.memory.get(&Rc::new(key)).unwrap().unwrap().0;

        match &entry.data {
            LedgerEntryData::Account(account_entry) => {
                serde_json::to_string(account_entry).unwrap()
            }
            _ => panic!("not an account"),
        }
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
        self.executor.send_transaction(te).unwrap()
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
#[napi]
impl MarsRover {
    #[napi(constructor)]
    pub fn new() -> Self {
        let memory = Rc::new(Memory::default());

        Self {
            memory,
            ledger_info: get_initial_ledger_info(),
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
    pub fn get_account(&self, account: String) -> String {
        let account_id = AccountId::from_xdr_base64(account, Limits::none()).unwrap();
        let key = LedgerKey::from(LedgerKeyAccount { account_id });
        let entry = self.memory.get(&Rc::new(key)).unwrap().unwrap().0;

        match &entry.data {
            LedgerEntryData::Account(account_entry) => {
                serde_json::to_string(account_entry).unwrap()
            }
            _ => panic!("not an account"),
        }
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

        let result = self
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
        self.apply_ledger_changes(result.ledger_changes);

        hash
    }

    #[napi]
    pub fn simulate_tx(&self, transaction_envelope: String) -> String {
        let te =
            TransactionEnvelope::from_xdr_base64(&transaction_envelope, Limits::none()).unwrap();

        let tx = match te {
            TransactionEnvelope::Tx(tx) => tx,
            _ => panic!("Unsupported transaction envelope type"),
        };

        let host_function_op = match &tx.tx.operations[0].body {
            OperationBody::InvokeHostFunction(host) => host,
            _ => panic!("Expected InvokeHostFunction operation"),
        };

        let network_config = default_network_config();

        let simulation = soroban_simulation::simulation::simulate_invoke_host_function_op(
            self.memory.clone(),
            &network_config,
            &SimulationAdjustmentConfig::no_adjustments(),
            &self.ledger_info,
            host_function_op.host_function.clone(),
            RecordingInvocationAuthMode::Recording(true),
            &tx.tx.source_account.account_id(),
            [1; 32],
            true,
        )
        .unwrap();

        if simulation.invoke_result.is_err() {
            let err = simulation.invoke_result.unwrap_err();
            let error = SimulateTransactionResponse::Error(SimulateTransactionErrorResponse {
                error: err.to_string(),
                id: "1".into(),
                parsed: true,
                events: simulation.diagnostic_events,
                latest_ledger: self.ledger_info.sequence_number,
            });

            return serde_json::to_string(&error).unwrap();
        }

        let changes = changes_from_simulation(simulation.modified_entries);

        let tx_data = simulation.transaction_data.unwrap();

        let response = SimulateTransactionResponse::Success(SimulateTransactionSuccessResponse {
            id: "1".into(),
            latest_ledger: self.ledger_info.sequence_number,
            events: simulation.diagnostic_events,
            min_resource_fee: tx_data.resource_fee.to_string(),
            parsed: true,
            result: Some(SimulateHostFunctionResult {
                retval: simulation.invoke_result.unwrap(),
                auth: simulation.auth.into_iter().map(|auth| auth.to_xdr_base64(Limits::none()).unwrap()).collect(),
            }),
            state_changes: Some(changes),
            transaction_data: tx_data.to_xdr_base64(Limits::none()).unwrap(),
        });

        return serde_json::to_string(&response).unwrap();
    }
    #[napi]
    pub fn send_transaction(&self, transaction_envelope: String) -> Vec<u8> {
        let te =
            TransactionEnvelope::from_xdr_base64(&transaction_envelope, Limits::none()).unwrap();

        let tx = match te {
            TransactionEnvelope::Tx(tx) => tx,
            _ => panic!("todo"),
        };

        let x = match &tx.tx.operations[0].body {
            OperationBody::InvokeHostFunction(host) => host,
            _ => todo!(),
        };

        let data = match tx.tx.ext {
            TransactionExt::V1(ext) => ext,
            _ => panic!("xlxl"),
        };

        let resources = &data.resources;

        let ind = match data.ext {
            SorobanTransactionDataExt::V1(ext) => ext.archived_soroban_entries.into_vec(),
            _ => vec![],
        };

        let p = self
            .invoke_host_function(
                &x.host_function,
                resources,
                &tx.tx.source_account.account_id(),
                x.auth.clone().into_vec(),
                &ind,
                [0; 32],
                true,
            )
            .unwrap();

        self.apply_ledger_changes(p.ledger_changes);

        p.encoded_invoke_result.unwrap()
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

    fn apply_ledger_changes(&self, changes: Vec<LedgerEntryChange>) {
        for change in changes {
            let key = LedgerKey::from_xdr(change.encoded_key, Limits::none()).unwrap();

            let ttl = change.ttl_change.map(|ttl| ttl.new_live_until_ledger);

            match change.encoded_new_value {
                Some(encoded_entry) => {
                    let entry = LedgerEntry::from_xdr(encoded_entry, Limits::none()).unwrap();
                    self.memory.insert_with_ttl(entry, ttl);
                }
                None if !change.read_only => {
                    self.memory.remove(&Rc::new(key));
                }
                _ => {
                    self.memory.update_ttl(&Rc::new(key), ttl);
                }
            }
        }
    }

    fn invoke_host_function(
        &self,
        host_fn: &HostFunction,
        resources: &SorobanResources,
        source_account: &AccountId,
        auth_entries: Vec<SorobanAuthorizationEntry>,
        restored_entry_indices: &[u32],
        prng_seed: [u8; 32],
        enable_diagnostics: bool,
    ) -> Result<InvokeHostFunctionResult, String> {
        let limits = Limits::none();

        let encoded_host_fn = host_fn.to_xdr(limits.clone()).unwrap();
        let encoded_resources = resources.to_xdr(limits.clone()).unwrap();
        let encoded_source_account = source_account.to_xdr(limits.clone()).unwrap();

        let encoded_auth_entries: Vec<Vec<u8>> = auth_entries
            .iter()
            .map(|entry| entry.to_xdr(limits.clone()).unwrap())
            .collect();

        let mut entries_with_ttl = Vec::new();
        let all_keys = resources
            .footprint
            .read_only
            .iter()
            .chain(resources.footprint.read_write.iter());

        for key in all_keys {
            if let Some((entry_rc, ttl)) = self.memory.get(&Rc::new(key.clone())).unwrap() {
                entries_with_ttl.push((entry_rc, ttl));
            }
        }

        let encoded_ledger_entries: Vec<Vec<u8>> = entries_with_ttl
            .iter()
            .map(|(entry, _)| entry.to_xdr(limits.clone()).unwrap())
            .collect();

        let encoded_ttl_entries: Vec<Vec<u8>> = entries_with_ttl
            .iter()
            .filter_map(|(entry, ttl)| {
                let key = match &entry.data {
                    LedgerEntryData::ContractData(cd) => {
                        Some(LedgerKey::ContractData(LedgerKeyContractData {
                            contract: cd.contract.clone(),
                            key: cd.key.clone(),
                            durability: cd.durability,
                        }))
                    }
                    LedgerEntryData::ContractCode(code) => {
                        Some(LedgerKey::ContractCode(LedgerKeyContractCode {
                            hash: code.hash.clone(),
                        }))
                    }
                    _ => None,
                };

                key.and_then(|k| {
                    ttl.map(|ttl_value| ttl_entry(&k, ttl_value).to_xdr(limits.clone()).ok())
                })
                .flatten()
            })
            .collect();

        let mut restored_contracts = HashSet::new();
        for index in restored_entry_indices {
            if let LedgerKey::ContractCode(code) = &resources.footprint.read_write[*index as usize]
            {
                restored_contracts.insert(code.hash.clone());
            }
        }

        let ledger_entries_for_cache: Vec<(LedgerEntry, Option<u32>)> = entries_with_ttl
            .iter()
            .map(|(entry_rc, ttl)| ((**entry_rc).clone(), *ttl))
            .collect();

        let module_cache = build_module_cache_for_entries(
            &self.ledger_info,
            ledger_entries_for_cache,
            &restored_contracts,
        )
        .unwrap();

        let unlimited_budget = Budget::try_from_configs(
            u64::MAX,
            u64::MAX,
            ContractCostParams(vec![].try_into().unwrap()),
            ContractCostParams(vec![].try_into().unwrap()),
        )
        .unwrap();

        let mut diagnostic_events = Vec::new();

        let result = e2e_invoke::invoke_host_function(
            &unlimited_budget,
            enable_diagnostics,
            encoded_host_fn,
            encoded_resources,
            restored_entry_indices,
            encoded_source_account,
            encoded_auth_entries.into_iter(),
            self.ledger_info.clone(),
            encoded_ledger_entries.into_iter(),
            encoded_ttl_entries.into_iter(),
            prng_seed.to_vec(),
            &mut diagnostic_events,
            None,
            Some(module_cache),
        )
        .unwrap();

        Ok(result)
    }
}
