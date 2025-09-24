use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use soroban_env_host::xdr::{DiagnosticEvent, LedgerEntry, LedgerKey, ScVal, SorobanAuthorizationEntry, SorobanTransactionData};

#[derive(  Serialize, Deserialize)]
#[serde(untagged)]
pub enum SimulateTransactionResponse {
    Success(SimulateTransactionSuccessResponse),
    Error(SimulateTransactionErrorResponse),
}

#[serde_as]
#[derive(  Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionSuccessResponse {
    pub id: String,
    pub latest_ledger: u32,
    pub events: Vec<DiagnosticEvent>,
    #[serde(rename = "_parsed")]
    pub parsed: bool,
    pub transaction_data: SorobanTransactionData,
    pub min_resource_fee: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<SimulateHostFunctionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_changes: Option<Vec<LedgerEntryChange>>,
}

#[derive(  Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionErrorResponse {
    pub id: String,
    pub latest_ledger: u32,
    pub events: Vec<DiagnosticEvent>,
    #[serde(rename = "_parsed")]
    pub parsed: bool,
    pub error: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryChange {
    #[serde(rename = "type")]
    pub change_type: u32,
    pub key: LedgerKey,
    pub before: Option<LedgerEntry>,
    pub after: Option<LedgerEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct SimulateHostFunctionResult {
    pub auth: Vec<SorobanAuthorizationEntry>,
    pub retval: ScVal,
}
