use p256::ecdsa::{VerifyingKey, Signature, signature::Verifier};

pub fn verify_challenge_response(public_key_hex: &str, challenge: &[u8], signature_hex: &str) -> bool {
    let pk_bytes = match hex::decode(public_key_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let verifying_key = match VerifyingKey::from_sec1_bytes(&pk_bytes) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let sig_bytes = match hex::decode(signature_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let signature = match Signature::from_der(&sig_bytes) {
        Ok(s) => s,
        Err(_) => {
            match Signature::from_slice(&sig_bytes) {
                Ok(s) => s,
                Err(_) => return false,
            }
        }
    };

    verifying_key.verify(challenge, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecdsa_verification_success() {
        use p256::ecdsa::{SigningKey, signature::Signer};
        use rand::rngs::OsRng;

        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);

        let pk_hex = hex::encode(verifying_key.to_sec1_bytes());
        let challenge = b"HIGH_ENTROPY_CHALLENGE_TOKEN_32B";
        
        let signature: Signature = signing_key.sign(challenge);
        let sig_hex = hex::encode(signature.to_der());

        assert!(verify_challenge_response(&pk_hex, challenge, &sig_hex));
    }

    #[test]
    fn test_ecdsa_verification_failure_modified_challenge() {
        use p256::ecdsa::{SigningKey, signature::Signer};
        use rand::rngs::OsRng;

        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);

        let pk_hex = hex::encode(verifying_key.to_sec1_bytes());
        let challenge = b"HIGH_ENTROPY_CHALLENGE_TOKEN_32B";
        
        let signature: Signature = signing_key.sign(challenge);
        let sig_hex = hex::encode(signature.to_der());

        assert!(!verify_challenge_response(&pk_hex, b"MODIFIED_CHALLENGE_TOKEN_32B", &sig_hex));
    }
}
