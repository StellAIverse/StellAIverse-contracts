#![cfg(any(test, feature = "testutils"))]

// use soroban_sdk::{Address, Bytes, Env, String};
use soroban_sdk::{Address, Bytes, Env};

use shared::testutils::{create_evolution_attestation, create_oracle_data};
use shared::{EvolutionAttestation, OracleData};

pub struct MockOracle;

impl MockOracle {
    /// Generate a deterministic mock attestation for testing
    pub fn generate_attestation(
        env: &Env,
        request_id: u64,
        agent_id: u64,
        provider: Address,
        new_model_hash: &str,
        nonce: u64,
    ) -> EvolutionAttestation {
        create_evolution_attestation(env, request_id, agent_id, provider, new_model_hash, nonce)
    }

    /// Generate an invalid mock attestation (wrong signature size)
    pub fn generate_invalid_attestation_signature(
        env: &Env,
        request_id: u64,
        agent_id: u64,
        provider: Address,
    ) -> EvolutionAttestation {
        let mut attestation =
            Self::generate_attestation(env, request_id, agent_id, provider, "invalid_hash", 1);
        attestation.signature = Bytes::from_slice(env, &[0u8; 32]); // Invalid size
        attestation
    }

    /// Generate mock oracle data
    pub fn generate_data(env: &Env, key: &str, value: &str, source: &str) -> OracleData {
        create_oracle_data(env, key, value, source)
    }
}
