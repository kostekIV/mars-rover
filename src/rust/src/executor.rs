use crate::model::SimulateHostFunctionResult;
use crate::network_config::default_network_config;
use crate::{
    ledger_info::NETWORK_PASSPHRASE,
    memory::Memory,
    model::{
        SimulateTransactionErrorResponse, SimulateTransactionResponse,
        SimulateTransactionSuccessResponse,
    },
    module_cache::new_module_cache,
};
use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use soroban_env_common::xdr::{DiagnosticEvent, ReadXdr};
use soroban_env_common::xdr::LedgerEntryChangeType;
use soroban_env_common::xdr::{
    ContractCostParams, HostFunction, LedgerEntry, LedgerEntryData, LedgerKey,
    LedgerKeyContractCode, LedgerKeyContractData, Limits, OperationBody,
    TransactionEnvelope, TransactionExt, VecM,
};
use soroban_env_host::{
    budget::AsBudget,
    budget::Budget,
    e2e_invoke::InvokeHostFunctionResult,
    e2e_invoke::{self, RecordingInvocationAuthMode},
    e2e_invoke::LedgerEntryChange,
    fees::{compute_rent_fee, compute_transaction_resource_fee, FeeConfiguration},
    storage::SnapshotSource,
    vm::VersionedContractCodeCostInputs,
    xdr::SorobanResources,
    xdr::WriteXdr,
    xdr::{
        AccountId, ContractCodeEntryExt, ContractCostType, Hash, SorobanAuthorizationEntry,
        SorobanTransactionDataExt, TtlEntry,
    },
    HostError, LedgerInfo, ModuleCache,
};
use soroban_simulation::simulation::LedgerEntryDiff;
use soroban_simulation::{simulation::SimulationAdjustmentConfig, NetworkConfig};
use std::collections::HashSet;
use std::rc::Rc;

fn compute_key_hash(key: &LedgerKey) -> Vec<u8> {
    let key_xdr = key.to_xdr(Limits::none()).unwrap();
    let hash: [u8; 32] = Sha256::digest(&key_xdr).into();
    hash.to_vec()
}

pub fn sha256_hash_from_bytes_raw(bytes: &[u8], budget: impl AsBudget) -> Result<[u8; 32]> {
    budget
        .as_budget()
        .charge(
            ContractCostType::ComputeSha256Hash,
            Some(bytes.len() as u64),
        )
        .context("Failed to charge budget for SHA256 hash")?;
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
) -> Result<ModuleCache> {
    let (cache, ctx) = new_module_cache()
        .context("Failed to create new module cache")?;

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
                .context("Failed to parse and cache module")?;
        }
    }
    Ok(cache)
}

fn changes_from_simulation(changes: Vec<LedgerEntryDiff>) -> Vec<crate::model::LedgerEntryChange> {
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

            crate::model::LedgerEntryChange {
                change_type,
                key,
                before: diff.state_before,
                after: diff.state_after,
            }
        })
        .collect()
}

pub struct Executor {
    memory: Rc<Memory>,
    ledger_info: LedgerInfo,
}

impl Executor {
    pub fn new(memory: Rc<Memory>, ledger_info: LedgerInfo) -> Self {
        Self {
            memory,
            ledger_info,
        }
    }

    pub fn simulate_transaction(&self, transaction_envelope: TransactionEnvelope) -> Result<SimulateTransactionResponse> {
        let tx = match transaction_envelope {
            TransactionEnvelope::Tx(tx) => tx,
            _ => return Err(anyhow::anyhow!("Unsupported transaction envelope type")),
        };

        let host_function_op = match &tx.tx.operations[0].body {
            OperationBody::InvokeHostFunction(host) => host,
            _ => return Err(anyhow::anyhow!("Expected InvokeHostFunction operation")),
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
            .context("Failed to simulate invoke host function operation")?;

        if simulation.invoke_result.is_err() {
            let err = simulation.invoke_result.unwrap_err();
            return Ok(SimulateTransactionResponse::Error(SimulateTransactionErrorResponse {
                error: err.to_string(),
                id: "1".into(),
                parsed: true,
                events: simulation.diagnostic_events,
                latest_ledger: self.ledger_info.sequence_number,
            }));
        }

        let changes = changes_from_simulation(simulation.modified_entries);
        let tx_data = simulation.transaction_data
            .ok_or_else(|| anyhow::anyhow!("Transaction data missing from simulation"))?;

        let response = SimulateTransactionResponse::Success(SimulateTransactionSuccessResponse {
            id: "1".into(),
            latest_ledger: self.ledger_info.sequence_number,
            events: simulation.diagnostic_events,
            min_resource_fee: tx_data.resource_fee.to_string(),
            parsed: true,
            result: Some(SimulateHostFunctionResult {
                retval: simulation.invoke_result?,
                auth: simulation.auth
                    .into_iter()
                    .map(|auth| auth.to_xdr_base64(Limits::none()))
                    .collect::<Result<Vec<_>, _>>()
                    .context("Failed to convert auth to XDR base64")?,
            }),
            state_changes: Some(changes),
            transaction_data: tx_data.to_xdr_base64(Limits::none())
                .context("Failed to convert transaction data to XDR base64")?,
        });

        Ok(response)
    }

    pub fn send_transaction(&self, transaction_envelope: TransactionEnvelope) -> Result<Result<Vec<u8>, HostError>> {
        let tx = match transaction_envelope {
            TransactionEnvelope::Tx(tx) => tx,
            _ => return Err(anyhow::anyhow!("Unsupported transaction envelope type")),
        };

        let host_function_op = match &tx.tx.operations[0].body {
            OperationBody::InvokeHostFunction(host) => host,
            _ => return Err(anyhow::anyhow!("Expected InvokeHostFunction operation")),
        };

        let soroban_data = match tx.tx.ext {
            TransactionExt::V1(ext) => ext,
            _ => return Err(anyhow::anyhow!("Expected transaction extension V1")),
        };

        let resources = &soroban_data.resources;

        let restored_entry_indices = match soroban_data.ext {
            SorobanTransactionDataExt::V1(ext) => ext.archived_soroban_entries.into_vec(),
            _ => vec![],
        };


        let result = self.invoke_host_function(
            &host_function_op.host_function,
            resources,
            &tx.tx.source_account.account_id(),
            host_function_op.auth.clone().into_vec(),
            &restored_entry_indices,
            [0; 32],
            true,
        )?;

        self.apply_ledger_changes(result.ledger_changes)?;

        Ok(result.encoded_invoke_result)
    }

    pub fn apply_ledger_changes(&self, changes: Vec<LedgerEntryChange>) -> Result<()> {
        for change in changes {
            let key = LedgerKey::from_xdr(change.encoded_key, Limits::none())
                .context("Failed to decode ledger key from XDR")?;

            let ttl = change.ttl_change.map(|ttl| ttl.new_live_until_ledger);

            match change.encoded_new_value {
                Some(encoded_entry) => {
                    let entry = LedgerEntry::from_xdr(encoded_entry, Limits::none())
                        .context("Failed to decode ledger entry from XDR")?;
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
        Ok(())
    }

    pub fn invoke_host_function(
        &self,
        host_fn: &HostFunction,
        resources: &SorobanResources,
        source_account: &AccountId,
        auth_entries: Vec<SorobanAuthorizationEntry>,
        restored_entry_indices: &[u32],
        prng_seed: [u8; 32],
        enable_diagnostics: bool,
    ) -> Result<InvokeHostFunctionResult> {
        let limits = Limits::none();

        let encoded_host_fn = host_fn.to_xdr(limits.clone())
            .context("Failed to encode host function to XDR")?;
        let encoded_resources = resources.to_xdr(limits.clone())
            .context("Failed to encode resources to XDR")?;
        let encoded_source_account = source_account.to_xdr(limits.clone())
            .context("Failed to encode source account to XDR")?;

        let encoded_auth_entries: Result<Vec<Vec<u8>>> = auth_entries
            .iter()
            .map(|entry| entry.to_xdr(limits.clone()).context("Failed to encode auth entry to XDR"))
            .collect();
        let encoded_auth_entries = encoded_auth_entries?;

        let mut entries_with_ttl = Vec::new();
        let all_keys = resources
            .footprint
            .read_only
            .iter()
            .chain(resources.footprint.read_write.iter());

        for key in all_keys {
            if let Some((entry_rc, ttl)) = self.memory.get(&Rc::new(key.clone()))
                .context("Failed to get entry from memory")? {
                entries_with_ttl.push((entry_rc, ttl));
            }
        }

        let encoded_ledger_entries: Result<Vec<Vec<u8>>> = entries_with_ttl
            .iter()
            .map(|(entry, _)| entry.to_xdr(limits.clone()).context("Failed to encode ledger entry to XDR"))
            .collect();
        let encoded_ledger_entries = encoded_ledger_entries?;

        let encoded_ttl_entries: Result<Vec<Vec<u8>>, _> = entries_with_ttl
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
                    ttl.map(|ttl_value| ttl_entry(&k, ttl_value).to_xdr(limits.clone()))
                })
            })
            .collect();

        let encoded_ttl_entries = encoded_ttl_entries?;

        let mut restored_contracts = HashSet::new();
        for index in restored_entry_indices {
            if let Some(key) = resources.footprint.read_write.get(*index as usize) {
                if let LedgerKey::ContractCode(code) = key {
                    restored_contracts.insert(code.hash.clone());
                }
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
        )?;

        let unlimited_budget = Budget::try_from_configs(
            u64::MAX,
            u64::MAX,
            ContractCostParams(vec![].try_into().unwrap()),
            ContractCostParams(vec![].try_into().unwrap()),
        )
            .context("Failed to create unlimited budget")?;

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
            .context("Failed to invoke host function")?;

        Ok(result)
    }
}