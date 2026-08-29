# AIverse-contracts (Soroban / Stellar)

AIverse — Core Soroban smart contracts in Rust

Legacy Notice

- `contracts_backup/` is deprecated legacy snapshot code.
- Use `contracts/` for all active development and deployment.

Purpose
This repository contains the on-chain logic for AIverse: tokenized AI Agents, an execution hub for agent actions, a marketplace, an evolution/upgrades system (staking-driven), oracle integration, and developer tooling & tests for Soroban on Stellar.

Badges
[![Build Status](https://img.shields.io/badge/build-unknown-lightgrey)](https://github.com/redux-space/AIverse-contracts/actions)
[![Coverage Status](https://img.shields.io/badge/coverage-unknown-lightgrey)](#)
[![Audit](https://img.shields.io/badge/audit-pending-orange)](#)
[![License](https://img.shields.io/badge/license-MIT-blue)](#license)

Table of contents

- Overview
- Key contracts & responsibilities
- Data model & metadata examples
- Architecture & flows
- Developer quickstart
- Build / Test / Deploy
- Interacting with contracts (examples)
- Oracle & off-chain integration pattern
- Security considerations
- Upgradeability & governance
- Contributing
- Roadmap & TODOs
- License & contact

Overview
AIverse tokenizes AI Agents so they can be owned, traded, leased, and upgraded on Stellar (Soroban). Agents are represented as NFTs or custom assets with structured metadata (model hashes, capabilities, evolution level). Off-chain compute (AI training, inference) is coordinated with on-chain state through secure attestations / oracles.

Key contracts & responsibilities
Replace names below with the repository's actual contract filenames and modules.

- PredictionMarket (prediction_market)
  - Advanced prediction market with automated market making (AMM)
  - Features binary and multi-outcome markets with dynamic pricing
  - Includes dispute resolution, reward distribution, and agent integration
  - Reputation-weighted betting for AI agents with enhanced rewards
  - Comprehensive oracle bridge integration for real-world data

- AgentNFT (agent_nft)
  - Minting and management of non-fungible Agent assets.
  - Stores agent metadata pointer (IPFS/CID, model hash) and minimal on-chain attributes (owner, evolution_level).
  - Supports lease/rent states and royalty splits.

- AgentToken (agent_token) — optional
  - If tokenizing agents as custom single-token assets instead of NFTs, implements fungible/custom behavior and metadata.

- AgentExecutionHub (execution_hub)
  - On-chain rules engine that records agent actions, emits events, enforces permissions and rate limits.
  - Defines canonical action types (e.g., ExecuteTrade, GenerateText, QueryOracle).
  - Records proofs/receipts of actions and tracks claims for off-chain processors.

- Marketplace (marketplace)
  - List/buy/sell/auction and lease logic.
  - Encodes royalty logic and marketplace fees.
  - Supports instant purchases and escrowed trades.

- MultiSigWallet (multisig-wallet)
  - M-of-N treasury wallet for Soroban-compatible token custody.
  - Supports signer management, transaction submission, confirmation, revocation, nonce-ordered execution, daily spending limits, whitelisted recipients, and execution history.

- TokenVesting (token-vesting)
  - Custodies Soroban-compatible tokens for linear vesting schedules.
  - Supports cliffs, batch beneficiary schedules, beneficiary claims, admin schedule edits, revocation, and early withdrawal penalties.
  - Emits lifecycle events for schedule creation, claims, batch funding, modifications, revocations, and early exits.

- FeeDistribution (fee-distribution)
  - Collects protocol fees from transactions and holds them in custody for distribution.
  - Supports configurable recipient categories (treasury, stakers, developers, etc.) with percentage-based shares, batched distribution and claims, fee exemptions, and distribution history for analytics.
  - Fee rate is governance-adjustable within a bounded step per call, with a separate emergency override and a hard-coded ceiling.

- MembershipSubscription (membership-subscription)
  - Tiered membership contract for recurring Soroban token subscriptions.
  - Supports trials, pull-based renewals, cancellation refunds, pause/resume, tier changes with proration, referral rewards, benefit entitlement checks, and subscription analytics.

- EvolutionManager (evolution)
  - Staking-driven upgrade system: users stake XLM or platform tokens to initiate training.
  - Tracks staking positions, upgrade requests, cooldowns and issuance of upgrade attestations.

- OracleBridge (oracle)
  - Verifies signed messages/attestations from approved off-chain oracles and relays external data (prices, news, AI results) into contracts.
  - Integrates with trusted relayers and supports merkle/nonce-based replay protection.

- Faucet (faucet) — testnet convenience
  - Issue test agents or tokens on Testnet for development and demos.

Data model & metadata examples
A consistent metadata format is crucial. Example Agent metadata (store JSON on IPFS / Arweave; store CID on-chain):

```json
{
  "name": "Agent-Atlas-v1",
  "description": "Trading agent trained for short-term commodity trades.",
  "model_hash": "sha256:3a5f...b7c9", // hash of the model/artifact
  "version": "1.0.0",
  "capabilities": ["trade_execution", "sentiment_analysis", "news_scrape"],
  "evolution_level": 2,
  "origin": {
    "training_data": "ipfs://Qm...",
    "trained_by": "ResearchLabX"
  },
  "royalties": {
    "recipient": "G...STELL",
    "bps": 500
  },
  "external_metadata_url": "https://metadata.stellai.example/agent/1234"
}
```

Events & actions
Define structured events so off-chain components can reliably react:

- AgentMinted { agent_id, owner, metadata_cid }
- AgentTransferred { agent_id, from, to }
- AgentListed { agent_id, seller, price, currency }
- AgentBought { agent_id, buyer, price, seller }
- EvolutionRequested { agent_id, staker, stake_amount, request_id }
- EvolutionCompleted { agent_id, request_id, new_level, attestation_id }
- OracleDataPosted { source, payload_hash, nonce }

Architecture & typical flow examples

1. Mint + Marketplace sale

- Creator uploads metadata to IPFS -> mints AgentNFT with metadata_cid.
- Seller lists agent on Marketplace with price and optional royalty.
- Buyer pays via Marketplace -> escrow releases NFT to buyer; royalties distributed.

2. Lease workflow

- Owner lists agent for lease; marketplace records lease terms.
- Lessee pays deposit + periodic fees; Marketplace marks agent as "leased" for duration.
- ExecutionHub enforces "lease-only" access to calls for that lessee.

3. Evolution (train/upgrade) workflow (high-level)

- Owner or community stakes XLM/tokens into EvolutionManager to request upgrade.
- Off-chain training system picks up request and runs training jobs (may cost off-chain compute).
- Off-chain trainer produces a signed attestation: { agent_id, new_model_hash, new_level, metadata_cid, trainer_signature }.
- Attestation is submitted to OracleBridge as a transaction; OracleBridge verifies signature and forwards to EvolutionManager.
- EvolutionManager validates attestation, updates agent metadata and evolution_level, emits EvolutionCompleted.

Developer quickstart
Prerequisites

- Rust (stable) + wasm target: rustup component add rust-src && rustup target add wasm32-unknown-unknown
- soroban-cli: cargo install --locked soroban-cli (or follow latest docs)
- Node.js & npm/yarn (optional for JS clients)
- Docker (optional for local sandbox)

Clone and prepare

```bash
git clone https://github.com/StellAIverse/stellAIverse-contracts.git
cd stellAIverse-contracts
rustup target add wasm32-unknown-unknown
cargo install --locked soroban-cli  # if needed
```

Build

```bash
cargo build --target wasm32-unknown-unknown --release
# wasm artifacts in:
# target/wasm32-unknown-unknown/release/<contract>.wasm
```

Run tests

```bash
cargo test
# or run any repo-provided test script:
# ./scripts/test.sh
```

Local sandbox (example)

- Use Soroban's sandbox or official docker image for integration tests:

```bash
# example, adapt to the repo's recommended image
docker run --rm -it -p 8000:8000 soroban/sandbox:latest
# configure soroban-cli to talk to local sandbox
soroban config network add local http://127.0.0.1:8000 --default
```

Deploy (example via soroban-cli)

```bash
# build wasms first
soroban contract deploy --wasm target/wasm32-unknown-unknown/release/agent_nft.wasm --network testnet
# note returned contract id -> use for subsequent calls and as canonical address
```

Interacting with contracts (examples)

soroban-cli: mint agent

```bash
soroban contract invoke --id <AGENT_NFT_CONTRACT_ID> --fn "mint" \
  --args <owner_address> <metadata_cid> --source <signer_secret>
```

soroban-cli: request evolution (stake)

```bash
soroban contract invoke --id <EVOLUTION_MANAGER_ID> --fn "request_evolution" \
  --args <agent_id> <stake_amount> --source <staker_secret>
```

Rust pseudo-interface examples
Below are illustrative trait-like signatures — implement according to your modules and idioms.

```rust
// Agent NFT
pub fn mint(env: Env, owner: Address, metadata_cid: String) -> Result<(), ContractError>;
pub fn transfer(env: Env, from: Address, to: Address, agent_id: u128) -> Result<(), ContractError>;
pub fn get_metadata(env: Env, agent_id: u128) -> Result<String, ContractError>;

// Evolution manager
pub fn request_evolution(env: Env, agent_id: u128, stake_amount: i128, requester: Address) -> Result<u128, ContractError>; // returns request_id
pub fn complete_evolution(env: Env, request_id: u128, attestation: Attestation) -> Result<(), ContractError>;

// Oracle bridge
pub fn post_attestation(env: Env, att: Attestation) -> Result<(), ContractError>;
```

Oracle & off-chain integration pattern

- Off-chain systems (training oracles, price oracles) sign attestation messages containing:
  - unique nonce, contract request_id, payload_hash (e.g., new model hash), timestamp, signer_id
- Attestations must be verifiable on-chain:
  - Contract maintains a registry of approved oracle public keys.
  - OracleBridge verifies signature and nonce, then forwards payload to target contract (EvolutionManager, ExecutionHub).
- Use replay protection (nonces, merkle roots), expiry times, and signer whitelists.
- For public oracles, prefer well-known providers or design a decentralized oracle aggregate (multi-signer).

Security considerations (critical)

- Access control: limit who can call admin functions. Consider multisig/timelock for upgrades and critical operations.
- Replay protection: nonces & expiration for oracle attestations.
- Validate off-chain metadata hashes on-chain where appropriate.
- Royalty & marketplace funds: escrow pattern with explicit withdraw flows to avoid reentrancy classes (apply Soroban safety patterns).
- Rate-limits on ExecutionHub to avoid abuse and DoS.
- Deterministic behavior: keep on-chain logic deterministic; off-chain compute should only influence state via signed attestations.
- Tests & static analysis: run cargo clippy, cargo fmt, cargo audit; add fuzz tests for critical logic.

Upgradeability & governance

- Soroban contract IDs are computed from wasm+salt; plan an upgrade path:
  - Registry + Proxy pattern (registry holds current implementation contract ID; users call registry which forwards).
  - Or keep immutability and deploy new contract versions with migration scripts (safer but requires migration).
- Protect upgrades with multisig/timelock and governance votes where appropriate.
- Document governance process (who can propose, how are votes counted, timelock durations).

Testing & CI recommendations

- Unit tests covering token, marketplace, evolution, and oracle flows.
- Integration tests using sandbox or local network (end-to-end: mint → list → buy → stake → evolve).
- Mock oracles in tests to simulate signed attestations and edge cases (invalid signature, stale attestation).
- Run lints, clippy, and cargo audit in CI; fail builds on warnings for critical contracts.

Gas & cost considerations

- Measure WASM size and function costs; optimize hot paths.
- Prefer off-chain heavy compute and only store/verifiy succinct attestations on-chain.
- Batch operations where possible to reduce per-action transaction costs (but mind atomicity implications).

Marketplace & royalties

- Ensure royalties are encoded in metadata and enforced by Marketplace on sale.
- Track royalty splits and ensure funds are distributed atomically when a sale executes.
- Consider configurable fee tiers (protocol fee + royalty fee) with clear recipients.

Faucet (testnet)

- Provide a faucet contract or script for issuing test agents/tokens. Guard faucet to limit spam and abuse (captcha, rate limit, admin controls).

Contributing
We welcome contributions. Please:

- Fork the repository
- Create a branch: git checkout -b feat/your-feature
- Add tests for new functionality
- Run linters & tests: cargo fmt && cargo clippy && cargo test
- Open a PR with a clear description, rationale, and test evidence
- Label PRs and link related issues

Roadmap & TODOs

- [ ] Finalize Agent metadata standard and ABI
- [ ] Implement AgentExecutionHub action registry & rate limiting
- [ ] Implement EvolutionManager with oracle flow
- [ ] Implement Marketplace with royalties & lease support
- [ ] Add mock oracle & integration tests
- [ ] Add audit & static analysis CI jobs
- [ ] Provide deployment scripts for Testnet and Mainnet
- [ ] Implement Faucet for Testnet

License
MIT — see LICENSE file.

Contact & maintainers
Maintainers: @OthmanImam (primary)
For security issues, please use: security@stellai.verse (replace with actual contact or follow GitHub security advisory)

Appendix: Example attestation format (JSON)

```json
{
  "request_id": 12345,
  "agent_id": 9876,
  "new_model_hash": "sha256:3a5f...b7c9",
  "new_metadata_cid": "ipfs://Qm...",
  "new_evolution_level": 3,
  "timestamp": 1672531200,
  "oracle_signature": "0x..."
}
```
