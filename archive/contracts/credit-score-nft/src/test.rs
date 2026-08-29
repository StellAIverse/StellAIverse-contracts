#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Address, BytesN, Env, String, Symbol};

    fn create_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn create_test_admin(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_test_user(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_test_verification_data(env: &Env) -> VerificationData {
        VerificationData {
            verification_method: String::from_str(env, "credit_bureau_api"),
            verified_by: Address::generate(env),
            verification_timestamp: env.ledger().timestamp(),
            verification_hash: BytesN::from_array(env, &[1u8; 32]),
            external_reference: String::from_str(env, "CB-12345"),
        }
    }

    fn create_test_metadata(env: &Env) -> CreditScoreMetadata {
        let mut attributes = Vec::new(env);
        attributes.push_back(ScoreAttribute {
            trait_type: String::from_str(env, "credit_score"),
            value: String::from_str(env, "750"),
            display_type: Some(String::from_str(env, "number")),
        });
        attributes.push_back(ScoreAttribute {
            trait_type: String::from_str(env, "score_type"),
            value: String::from_str(env, "FICO"),
            display_type: Some(String::from_str(env, "string")),
        });

        CreditScoreMetadata {
            name: String::from_str(env, "Credit Score NFT"),
            description: String::from_str(env, "Verified credit score NFT"),
            image: String::from_str(env, "https://example.com/credit-score.png"),
            external_url: String::from_str(env, "https://stellai.com/credit-score/1"),
            attributes,
        }
    }

    fn create_test_mint_request(env: &Env, owner: Address) -> MintRequest {
        MintRequest {
            owner,
            credit_score: 750,
            score_type: ScoreType::FICO,
            metadata: create_test_metadata(env),
            verification_data: create_test_verification_data(env),
            expires_at: env.ledger().timestamp() + 365 * 24 * 60 * 60, // 1 year
            royalty_info: None,
        }
    }

    #[test]
    fn test_init_contract() {
        let env = create_test_env();
        let admin = create_test_admin(&env);

        let result = CreditScoreNFT::init_contract(env.clone(), admin.clone());
        assert!(result.is_ok());

        // Test double initialization fails
        let result = CreditScoreNFT::init_contract(env.clone(), admin);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ContractError::AlreadyInitialized);

        println!("✅ Contract initialization test passed!");
    }

    #[test]
    fn test_add_verification_authority() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let authority = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin.clone()).unwrap();

        // Add verification authority
        let result = CreditScoreNFT::add_verification_authority(
            env.clone(),
            admin.clone(),
            authority.clone(),
        );
        assert!(result.is_ok());

        // Test adding duplicate fails
        let result = CreditScoreNFT::add_verification_authority(env.clone(), admin, authority);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ContractError::AlreadyExists);

        // Verify authority was added
        let authorities = CreditScoreNFT::get_verification_authorities(env.clone());
        assert_eq!(authorities.len(), 1);
        assert_eq!(authorities.get(0).unwrap(), authority);

        println!("✅ Verification authority test passed!");
    }

    #[test]
    fn test_mint_credit_score_nft() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin.clone()).unwrap();

        // Create mint request
        let request = create_test_mint_request(&env, owner.clone());

        // Mint NFT
        let result =
            CreditScoreNFT::mint_credit_score_nft(env.clone(), minter.clone(), request.clone());
        assert!(result.is_ok());
        let token_id = result.unwrap();
        assert_eq!(token_id, 1);

        // Verify NFT was created
        let nft = CreditScoreNFT::get_nft(env.clone(), token_id).unwrap();
        assert_eq!(nft.token_id, token_id);
        assert_eq!(nft.owner, owner);
        assert_eq!(nft.credit_score, 750);
        assert_eq!(nft.score_type, ScoreType::FICO);
        assert_eq!(nft.verification_status, VerificationStatus::Pending);

        // Verify metadata was stored
        let metadata = CreditScoreNFT::get_metadata(env.clone(), token_id).unwrap();
        assert_eq!(metadata.name, "Credit Score NFT");
        assert_eq!(metadata.attributes.len(), 2);

        println!("✅ Credit score NFT minting test passed!");
    }

    #[test]
    fn test_invalid_credit_score() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin).unwrap();

        // Create mint request with invalid score (below 300)
        let mut request = create_test_mint_request(&env, owner.clone());
        request.credit_score = 250;

        let result = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter.clone(), request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ContractError::InvalidInput);

        // Create mint request with invalid score (above 850)
        let mut request = create_test_mint_request(&env, owner);
        request.credit_score = 900;

        let result = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ContractError::InvalidInput);

        println!("✅ Invalid credit score validation test passed!");
    }

    #[test]
    fn test_verify_credit_score() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);
        let verifier = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin.clone()).unwrap();

        // Add verification authority
        CreditScoreNFT::add_verification_authority(env.clone(), admin, verifier.clone()).unwrap();

        // Mint NFT
        let request = create_test_mint_request(&env, owner);
        let token_id = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request).unwrap();

        // Verify NFT
        let verification_hash = BytesN::from_array(&env, &[2u8; 32]);
        let result = CreditScoreNFT::verify_credit_score(
            env.clone(),
            verifier.clone(),
            token_id,
            verification_hash,
        );
        assert!(result.is_ok());

        // Check verification status
        let is_verified = CreditScoreNFT::is_verified(env.clone(), token_id).unwrap();
        assert!(is_verified);

        // Get updated NFT
        let nft = CreditScoreNFT::get_nft(env.clone(), token_id).unwrap();
        assert_eq!(nft.verification_status, VerificationStatus::Verified);
        assert_eq!(nft.verification_data.verification_hash, verification_hash);

        println!("✅ Credit score verification test passed!");
    }

    #[test]
    fn test_update_credit_score() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);
        let verifier = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin.clone()).unwrap();

        // Add verification authority
        CreditScoreNFT::add_verification_authority(env.clone(), admin, verifier.clone()).unwrap();

        // Mint NFT
        let request = create_test_mint_request(&env, owner);
        let token_id = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request).unwrap();

        // Update credit score
        let new_score = 780;
        let new_expires_at = env.ledger().timestamp() + 365 * 24 * 60 * 60;
        let result = CreditScoreNFT::update_credit_score(
            env.clone(),
            verifier.clone(),
            token_id,
            new_score,
            new_expires_at,
        );
        assert!(result.is_ok());

        // Verify updated score
        let nft = CreditScoreNFT::get_nft(env.clone(), token_id).unwrap();
        assert_eq!(nft.credit_score, new_score);
        assert_eq!(nft.expires_at, new_expires_at);
        assert_eq!(nft.verification_status, VerificationStatus::Verified);

        println!("✅ Credit score update test passed!");
    }

    #[test]
    fn test_transfer_nft() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);
        let new_owner = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin).unwrap();

        // Mint NFT
        let request = create_test_mint_request(&env, owner.clone());
        let token_id = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request).unwrap();

        // Transfer NFT
        let result =
            CreditScoreNFT::transfer_nft(env.clone(), owner.clone(), new_owner.clone(), token_id);
        assert!(result.is_ok());

        // Verify new ownership
        let current_owner = CreditScoreNFT::owner_of(env.clone(), token_id).unwrap();
        assert_eq!(current_owner, new_owner);

        // Test unauthorized transfer fails
        let result = CreditScoreNFT::transfer_nft(env.clone(), owner, new_owner, token_id);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ContractError::Unauthorized);

        println!("✅ NFT transfer test passed!");
    }

    #[test]
    fn test_get_nfts_by_owner() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner1 = create_test_user(&env);
        let owner2 = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin).unwrap();

        // Mint NFTs for different owners
        let request1 = create_test_mint_request(&env, owner1.clone());
        let token1 =
            CreditScoreNFT::mint_credit_score_nft(env.clone(), minter.clone(), request1).unwrap();

        let request2 = create_test_mint_request(&env, owner2.clone());
        let token2 =
            CreditScoreNFT::mint_credit_score_nft(env.clone(), minter.clone(), request2).unwrap();

        let request3 = create_test_mint_request(&env, owner1.clone());
        let token3 = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request3).unwrap();

        // Get NFTs for owner1
        let owner1_tokens = CreditScoreNFT::get_nfts_by_owner(env.clone(), owner1).unwrap();
        assert_eq!(owner1_tokens.len(), 2);
        assert!(owner1_tokens.contains(&token1));
        assert!(owner1_tokens.contains(&token3));

        // Get NFTs for owner2
        let owner2_tokens = CreditScoreNFT::get_nfts_by_owner(env.clone(), owner2).unwrap();
        assert_eq!(owner2_tokens.len(), 1);
        assert!(owner2_tokens.contains(&token2));

        println!("✅ Get NFTs by owner test passed!");
    }

    #[test]
    fn test_metadata_standards() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin).unwrap();

        // Create comprehensive metadata
        let mut attributes = Vec::new(&env);
        attributes.push_back(ScoreAttribute {
            trait_type: String::from_str(&env, "credit_score"),
            value: String::from_str(&env, "750"),
            display_type: Some(String::from_str(&env, "number")),
        });
        attributes.push_back(ScoreAttribute {
            trait_type: String::from_str(&env, "score_type"),
            value: String::from_str(&env, "FICO"),
            display_type: Some(String::from_str(&env, "string")),
        });
        attributes.push_back(ScoreAttribute {
            trait_type: String::from_str(&env, "verification_date"),
            value: String::from_str(&env, "2024-01-01"),
            display_type: Some(String::from_str(&env, "date")),
        });
        attributes.push_back(ScoreAttribute {
            trait_type: String::from_str(&env, "issuing_bureau"),
            value: String::from_str(&env, "Experian"),
            display_type: Some(String::from_str(&env, "string")),
        });

        let metadata = CreditScoreMetadata {
            name: String::from_str(&env, "Premium Credit Score NFT"),
            description: String::from_str(
                &env,
                "A verified credit score NFT with comprehensive metadata",
            ),
            image: String::from_str(&env, "https://ipfs.io/ipfs/QmHash123"),
            external_url: String::from_str(&env, "https://stellai.com/token/1"),
            attributes,
        };

        let request = MintRequest {
            owner,
            credit_score: 750,
            score_type: ScoreType::FICO,
            metadata: metadata.clone(),
            verification_data: create_test_verification_data(&env),
            expires_at: env.ledger().timestamp() + 365 * 24 * 60 * 60,
            royalty_info: None,
        };

        // Mint NFT with comprehensive metadata
        let token_id = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request).unwrap();

        // Verify metadata standards compliance
        let stored_metadata = CreditScoreNFT::get_metadata(env.clone(), token_id).unwrap();
        assert_eq!(stored_metadata.name, metadata.name);
        assert_eq!(stored_metadata.description, metadata.description);
        assert_eq!(stored_metadata.image, metadata.image);
        assert_eq!(stored_metadata.external_url, metadata.external_url);
        assert_eq!(stored_metadata.attributes.len(), 4);

        // Verify attribute structure
        for i in 0..stored_metadata.attributes.len() {
            let attr = stored_metadata.attributes.get(i).unwrap();
            assert!(!attr.trait_type.is_empty());
            assert!(!attr.value.is_empty());
        }

        println!("✅ Metadata standards compliance test passed!");
    }

    #[test]
    fn test_score_types() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin).unwrap();

        // Test different score types
        let score_types = vec![
            ScoreType::FICO,
            ScoreType::VantageScore,
            ScoreType::Experian,
            ScoreType::Equifax,
            ScoreType::TransUnion,
            ScoreType::Custom,
        ];

        for (i, score_type) in score_types.iter().enumerate() {
            let mut request = create_test_mint_request(&env, owner.clone());
            request.score_type = score_type.clone();

            let token_id =
                CreditScoreNFT::mint_credit_score_nft(env.clone(), minter.clone(), request.clone())
                    .unwrap();

            let nft = CreditScoreNFT::get_nft(env.clone(), token_id).unwrap();
            assert_eq!(nft.score_type, *score_type);
        }

        println!("✅ Score types test passed!");
    }

    #[test]
    fn test_royalty_info() {
        let env = create_test_env();
        let admin = create_test_admin(&env);
        let minter = create_test_user(&env);
        let owner = create_test_user(&env);
        let royalty_recipient = create_test_user(&env);

        // Initialize contract
        CreditScoreNFT::init_contract(env.clone(), admin).unwrap();

        // Create mint request with royalty info
        let mut request = create_test_mint_request(&env, owner);
        request.royalty_info = Some(RoyaltyInfo {
            recipient: royalty_recipient.clone(),
            percentage: 250, // 2.5%
        });

        let token_id = CreditScoreNFT::mint_credit_score_nft(env.clone(), minter, request).unwrap();

        // Verify royalty info is stored
        let nft = CreditScoreNFT::get_nft(env.clone(), token_id).unwrap();
        assert!(nft.royalty_info.is_some());
        let royalty = nft.royalty_info.unwrap();
        assert_eq!(royalty.recipient, royalty_recipient);
        assert_eq!(royalty.percentage, 250);

        println!("✅ Royalty info test passed!");
    }
}
