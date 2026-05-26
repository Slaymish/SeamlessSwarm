use p256::ecdsa::{SigningKey, VerifyingKey, Signature, signature::Signer};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub struct SimulatedSecureElement {
    signing_key: SigningKey,
}

impl SimulatedSecureElement {
    pub fn new() -> Self {
        let key_path = Path::new("simulated_node_key.der");
        let signing_key = if key_path.exists() {
            match fs::read(key_path) {
                Ok(bytes) => {
                    match SigningKey::from_bytes((&bytes[..]).into()) {
                        Ok(k) => k,
                        Err(_) => Self::generate_and_save_key(key_path),
                    }
                }
                Err(_) => Self::generate_and_save_key(key_path),
            }
        } else {
            Self::generate_and_save_key(key_path)
        };

        Self { signing_key }
    }

    fn generate_and_save_key(path: &Path) -> SigningKey {
        let signing_key = SigningKey::random(&mut OsRng);
        let bytes = signing_key.to_bytes();
        let _ = fs::write(path, bytes);
        signing_key
    }

    pub fn get_public_key(&self) -> Vec<u8> {
        let verifying_key = VerifyingKey::from(&self.signing_key);
        verifying_key.to_sec1_bytes().to_vec()
    }

    pub fn get_static_thumbprint(&self) -> Vec<u8> {
        let pub_key = self.get_public_key();
        let mut hasher = Sha256::new();
        hasher.update(&pub_key);
        hasher.finalize().to_vec()
    }

    pub fn sign_challenge(&self, challenge: &[u8]) -> Vec<u8> {
        let signature: Signature = self.signing_key.sign(challenge);
        signature.to_der().to_bytes().to_vec()
    }
}
