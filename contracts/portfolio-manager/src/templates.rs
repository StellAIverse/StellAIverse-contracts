use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::types::*;

/// Pre-configured portfolio template with audited allocations
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PortfolioTemplate {
    pub template_type: PortfolioType,
    pub name: Symbol,
    pub description: Symbol,
    pub weighting_strategy: WeightingStrategy,
    pub rebalance_frequency: RebalanceFrequency,
    pub drift_tolerance_bps: u32,
    pub max_slippage_bps: u32,
    /// Pre-defined asset template slots (token, weight_bps, feed_id)
    pub allocation_template: Vec<AllocationSlot>,
}

/// A slot in a template allocation: stores weight and optional feed,
/// but NOT the token address (set at creation time).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AllocationSlot {
    pub weight_bps: u32,
    pub feed_id: Option<Symbol>,
    pub label: Symbol,
}

pub struct Templates;

impl Templates {
    /// Create a conservative portfolio template (60% bonds / 40% equities)
    pub fn conservative_template(env: &Env) -> PortfolioTemplate {
        let mut slots = Vec::new(env);
        // 60% bonds split across 3 bond types
        slots.push_back(AllocationSlot {
            weight_bps: 2000, // 20%
            feed_id: Some(Symbol::new(env, "us_bonds")),
            label: Symbol::new(env, "us_treasuries"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 2000, // 20%
            feed_id: Some(Symbol::new(env, "corp_bonds")),
            label: Symbol::new(env, "corp_bonds"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 2000, // 20%
            feed_id: Some(Symbol::new(env, "intl_bonds")),
            label: Symbol::new(env, "intl_bonds"),
        });
        // 40% equities split across 4 equity types
        slots.push_back(AllocationSlot {
            weight_bps: 1500, // 15%
            feed_id: Some(Symbol::new(env, "us_large_cap")),
            label: Symbol::new(env, "us_large_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1000, // 10%
            feed_id: Some(Symbol::new(env, "us_small_cap")),
            label: Symbol::new(env, "us_small_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1000, // 10%
            feed_id: Some(Symbol::new(env, "intl_developed")),
            label: Symbol::new(env, "intl_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 500, // 5%
            feed_id: Some(Symbol::new(env, "emerging_markets")),
            label: Symbol::new(env, "em_eq"),
        });

        PortfolioTemplate {
            template_type: PortfolioType::Conservative,
            name: Symbol::new(env, "Conservative"),
            description: Symbol::new(env, "60bonds_40eq"),
            weighting_strategy: WeightingStrategy::CustomWeight,
            rebalance_frequency: RebalanceFrequency::Quarterly,
            drift_tolerance_bps: 200,
            max_slippage_bps: 300,
            allocation_template: slots,
        }
    }

    /// Create a balanced portfolio template (50% equities / 30% bonds / 20% alternatives)
    pub fn balanced_template(env: &Env) -> PortfolioTemplate {
        let mut slots = Vec::new(env);
        // 50% equities
        slots.push_back(AllocationSlot {
            weight_bps: 2000,
            feed_id: Some(Symbol::new(env, "us_large_cap")),
            label: Symbol::new(env, "us_large_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1000,
            feed_id: Some(Symbol::new(env, "us_small_cap")),
            label: Symbol::new(env, "us_small_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1000,
            feed_id: Some(Symbol::new(env, "intl_developed")),
            label: Symbol::new(env, "intl_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1000,
            feed_id: Some(Symbol::new(env, "emerging_markets")),
            label: Symbol::new(env, "em_eq"),
        });
        // 30% bonds
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "us_bonds")),
            label: Symbol::new(env, "us_treasuries"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "corp_bonds")),
            label: Symbol::new(env, "corp_bonds"),
        });
        // 20% alternatives
        slots.push_back(AllocationSlot {
            weight_bps: 1000,
            feed_id: Some(Symbol::new(env, "gold")),
            label: Symbol::new(env, "gold"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1000,
            feed_id: Some(Symbol::new(env, "real_estate")),
            label: Symbol::new(env, "reit"),
        });

        PortfolioTemplate {
            template_type: PortfolioType::Balanced,
            name: Symbol::new(env, "Balanced"),
            description: Symbol::new(env, "50eq_30bond_20alt"),
            weighting_strategy: WeightingStrategy::CustomWeight,
            rebalance_frequency: RebalanceFrequency::Quarterly,
            drift_tolerance_bps: 200,
            max_slippage_bps: 400,
            allocation_template: slots,
        }
    }

    /// Create an aggressive portfolio template (80% equities / 20% alternatives)
    pub fn aggressive_template(env: &Env) -> PortfolioTemplate {
        let mut slots = Vec::new(env);
        // 80% equities
        slots.push_back(AllocationSlot {
            weight_bps: 3000,
            feed_id: Some(Symbol::new(env, "us_large_cap")),
            label: Symbol::new(env, "us_large_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 2000,
            feed_id: Some(Symbol::new(env, "us_small_cap")),
            label: Symbol::new(env, "us_small_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "intl_developed")),
            label: Symbol::new(env, "intl_eq"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "emerging_markets")),
            label: Symbol::new(env, "em_eq"),
        });
        // 20% alternatives
        slots.push_back(AllocationSlot {
            weight_bps: 1000,
            feed_id: Some(Symbol::new(env, "gold")),
            label: Symbol::new(env, "gold"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 500,
            feed_id: Some(Symbol::new(env, "real_estate")),
            label: Symbol::new(env, "reit"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 500,
            feed_id: Some(Symbol::new(env, "crypto")),
            label: Symbol::new(env, "crypto"),
        });

        PortfolioTemplate {
            template_type: PortfolioType::Aggressive,
            name: Symbol::new(env, "Aggressive"),
            description: Symbol::new(env, "80eq_20alt"),
            weighting_strategy: WeightingStrategy::CustomWeight,
            rebalance_frequency: RebalanceFrequency::Monthly,
            drift_tolerance_bps: 150,
            max_slippage_bps: 500,
            allocation_template: slots,
        }
    }

    /// Get a template by type
    pub fn get_template(env: &Env, template_type: PortfolioType) -> PortfolioTemplate {
        match template_type {
            PortfolioType::Conservative => Self::conservative_template(env),
            PortfolioType::Balanced => Self::balanced_template(env),
            PortfolioType::Aggressive => Self::aggressive_template(env),
            PortfolioType::Thematic => Self::thematic_template(env),
            PortfolioType::Custom => panic!("Custom templates must be built manually"),
        }
    }

    /// Create a thematic template (crypto-focused as an example)
    pub fn thematic_template(env: &Env) -> PortfolioTemplate {
        let mut slots = Vec::new(env);
        slots.push_back(AllocationSlot {
            weight_bps: 3000,
            feed_id: Some(Symbol::new(env, "btc")),
            label: Symbol::new(env, "bitcoin"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 2500,
            feed_id: Some(Symbol::new(env, "eth")),
            label: Symbol::new(env, "ethereum"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "sol")),
            label: Symbol::new(env, "solana"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "defi_index")),
            label: Symbol::new(env, "defi"),
        });
        slots.push_back(AllocationSlot {
            weight_bps: 1500,
            feed_id: Some(Symbol::new(env, "nft_index")),
            label: Symbol::new(env, "nft_tokens"),
        });

        PortfolioTemplate {
            template_type: PortfolioType::Thematic,
            name: Symbol::new(env, "CryptoThematic"),
            description: Symbol::new(env, "crypto_index"),
            weighting_strategy: WeightingStrategy::CustomWeight,
            rebalance_frequency: RebalanceFrequency::Monthly,
            drift_tolerance_bps: 300,
            max_slippage_bps: 800,
            allocation_template: slots,
        }
    }

    /// Validate that target weights sum to BPS_DENOMINATOR
    pub fn validate_weights(allocations: &Vec<AssetAllocation>) -> bool {
        let total: i128 = allocations.iter().map(|a| a.weight_bps as i128).sum();
        total == BPS_DENOMINATOR
    }

    /// Create equal-weight allocations for a list of tokens
    pub fn equal_weight_allocations(env: &Env, tokens: &Vec<Address>) -> Vec<AssetAllocation> {
        let count = tokens.len();
        if count == 0 {
            return Vec::new(env);
        }

        let base_weight = BPS_DENOMINATOR / count as i128;
        let remainder = BPS_DENOMINATOR - base_weight * count as i128;

        let mut allocations = Vec::new(env);
        for i in 0..count {
            // Distribute remainder BPS across first tokens
            let extra = if (i as i128) < remainder { 1 } else { 0 };
            allocations.push_back(AssetAllocation {
                token: tokens.get_unchecked(i),
                weight_bps: (base_weight + extra) as u32,
                feed_id: None,
            });
        }
        allocations
    }

    /// Convert template slots into asset allocations by pairing with token addresses
    pub fn template_to_allocations(
        env: &Env,
        template: &PortfolioTemplate,
        tokens: &Vec<Address>,
    ) -> Vec<AssetAllocation> {
        let slot_count = template.allocation_template.len();
        let token_count = tokens.len();
        if slot_count != token_count {
            panic!("Token count must match template slot count");
        }

        let mut allocations = Vec::new(env);
        for i in 0..slot_count {
            let slot = template.allocation_template.get_unchecked(i);
            allocations.push_back(AssetAllocation {
                token: tokens.get_unchecked(i),
                weight_bps: slot.weight_bps,
                feed_id: slot.feed_id.clone(),
            });
        }
        allocations
    }
}
