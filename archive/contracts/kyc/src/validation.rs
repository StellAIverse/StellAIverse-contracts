use soroban_sdk::{Address, Bytes, Env, Symbol};

#[derive(Debug, PartialEq, Eq)]
pub enum KycValidationError {
    MissingField(&'static str),
    InvalidFormat(&'static str),
    MalformedData(&'static str),
}

pub struct KycSubmission<'a> {
    pub user: &'a Address,
    pub name: &'a str,
    pub dob: &'a str, // ISO 8601: YYYY-MM-DD
    pub nationality: &'a str,
    pub id_number: &'a str,
    pub id_type: &'a str, // e.g., "passport", "national_id"
    pub selfie: &'a Bytes,
}

impl<'a> KycSubmission<'a> {
    pub fn validate(&self) -> Result<(), KycValidationError> {
        if self.name.trim().is_empty() {
            return Err(KycValidationError::MissingField("name"));
        }
        if self.dob.trim().is_empty() {
            return Err(KycValidationError::MissingField("dob"));
        }
        if !self.dob.chars().all(|c| c.is_ascii() && (c.is_digit(10) || c == '-')) || self.dob.len() != 10 {
            return Err(KycValidationError::InvalidFormat("dob"));
        }
        if self.nationality.trim().is_empty() {
            return Err(KycValidationError::MissingField("nationality"));
        }
        if self.id_number.trim().is_empty() {
            return Err(KycValidationError::MissingField("id_number"));
        }
        if self.id_type.trim().is_empty() {
            return Err(KycValidationError::MissingField("id_type"));
        }
        if self.selfie.len() < 1000 {
            // Arbitrary minimum size for a valid image
            return Err(KycValidationError::MalformedData("selfie"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Bytes, Env};

    #[test]
    fn test_valid_kyc_submission() {
        let env = Env::default();
        let user = Address::generate(&env);
        let selfie = Bytes::from_array(&env, &[1u8; 2000]);
        let kyc = KycSubmission {
            user: &user,
            name: "Alice Smith",
            dob: "1990-01-01",
            nationality: "US",
            id_number: "A1234567",
            id_type: "passport",
            selfie: &selfie,
        };
        assert_eq!(kyc.validate(), Ok(()));
    }

    #[test]
    fn test_missing_name() {
        let env = Env::default();
        let user = Address::generate(&env);
        let selfie = Bytes::from_array(&env, &[1u8; 2000]);
        let kyc = KycSubmission {
            user: &user,
            name: " ",
            dob: "1990-01-01",
            nationality: "US",
            id_number: "A1234567",
            id_type: "passport",
            selfie: &selfie,
        };
        assert_eq!(kyc.validate(), Err(KycValidationError::MissingField("name")));
    }

    #[test]
    fn test_invalid_dob_format() {
        let env = Env::default();
        let user = Address::generate(&env);
        let selfie = Bytes::from_array(&env, &[1u8; 2000]);
        let kyc = KycSubmission {
            user: &user,
            name: "Alice Smith",
            dob: "01-01-1990",
            nationality: "US",
            id_number: "A1234567",
            id_type: "passport",
            selfie: &selfie,
        };
        assert_eq!(kyc.validate(), Err(KycValidationError::InvalidFormat("dob")));
    }

    #[test]
    fn test_malformed_selfie() {
        let env = Env::default();
        let user = Address::generate(&env);
        let selfie = Bytes::from_array(&env, &[1u8; 10]);
        let kyc = KycSubmission {
            user: &user,
            name: "Alice Smith",
            dob: "1990-01-01",
            nationality: "US",
            id_number: "A1234567",
            id_type: "passport",
            selfie: &selfie,
        };
        assert_eq!(kyc.validate(), Err(KycValidationError::MalformedData("selfie")));
    }
}
