use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct Capability {
    pub name: String,
    pub val_type: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct NodeProfile {
    pub node_id: String,
    pub os_platform: String,
    pub capabilities: Vec<Capability>,
    pub last_seen: u64,
    pub public_key: String,
}

#[derive(Clone)]
pub struct EphemeralRegistry {
    nodes: Arc<RwLock<HashMap<String, NodeProfile>>>,
}

impl EphemeralRegistry {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_node(&self, profile: NodeProfile) {
        if let Ok(mut lock) = self.nodes.write() {
            lock.insert(profile.node_id.clone(), profile);
        }
    }

    pub fn authenticate_and_register_node(
        &self,
        profile: NodeProfile,
        challenge: &[u8],
        signature_hex: &str,
        trusted_thumbprints: &[String],
    ) -> Result<(), String> {
        use sha2::Digest;

        let pk_bytes = hex::decode(&profile.public_key).map_err(|e| e.to_string())?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&pk_bytes);
        let derived_thumbprint = hex::encode(hasher.finalize());

        if !trusted_thumbprints.contains(&derived_thumbprint) {
            return Err("Node public key thumbprint not authorized in database".to_string());
        }

        if !crate::auth::verify_challenge_response(&profile.public_key, challenge, signature_hex) {
            return Err("Cryptographic ECDSA verification failed".to_string());
        }

        self.register_node(profile);
        Ok(())
    }

    pub fn unregister_node(&self, node_id: &str) {
        if let Ok(mut lock) = self.nodes.write() {
            lock.remove(node_id);
        }
    }

    pub fn get_node(&self, node_id: &str) -> Option<NodeProfile> {
        self.nodes.read().ok()?.get(node_id).cloned()
    }

    pub fn list_nodes(&self) -> Vec<NodeProfile> {
        match self.nodes.read() {
            Ok(lock) => lock.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for EphemeralRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get_node() {
        let registry = EphemeralRegistry::new();
        let profile = NodeProfile {
            node_id: "test-node".to_string(),
            os_platform: "Linux".to_string(),
            capabilities: vec![
                Capability {
                    name: "cores".to_string(),
                    val_type: "int".to_string(),
                    value: "4".to_string(),
                }
            ],
            last_seen: 100,
            public_key: "".to_string(),
        };

        registry.register_node(profile.clone());
        let retrieved = registry.get_node("test-node").unwrap();
        assert_eq!(retrieved.node_id, "test-node");
        assert_eq!(retrieved.os_platform, "Linux");
        assert_eq!(retrieved.capabilities.len(), 1);
        assert_eq!(retrieved.capabilities[0].name, "cores");
    }

    #[test]
    fn test_unregister_node() {
        let registry = EphemeralRegistry::new();
        let profile = NodeProfile {
            node_id: "test-node".to_string(),
            os_platform: "Linux".to_string(),
            capabilities: vec![],
            last_seen: 100,
            public_key: "".to_string(),
        };

        registry.register_node(profile);
        assert!(registry.get_node("test-node").is_some());

        registry.unregister_node("test-node");
        assert!(registry.get_node("test-node").is_none());
    }

    #[test]
    fn test_list_nodes() {
        let registry = EphemeralRegistry::new();
        assert_eq!(registry.list_nodes().len(), 0);

        let p1 = NodeProfile {
            node_id: "node-1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 100,
            public_key: "".to_string(),
        };
        let p2 = NodeProfile {
            node_id: "node-2".to_string(),
            os_platform: "Windows".to_string(),
            capabilities: vec![],
            last_seen: 200,
            public_key: "".to_string(),
        };

        registry.register_node(p1);
        registry.register_node(p2);

        let list = registry.list_nodes();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|n| n.node_id == "node-1"));
        assert!(list.iter().any(|n| n.node_id == "node-2"));
    }

    #[test]
    fn test_authenticate_and_register_node_scenarios() {
        use p256::ecdsa::{SigningKey, signature::Signer};
        use rand::rngs::OsRng;
        use sha2::Digest;

        let registry = EphemeralRegistry::new();
        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = p256::ecdsa::VerifyingKey::from(&signing_key);
        let pk_bytes = verifying_key.to_sec1_bytes();
        let pk_hex = hex::encode(&pk_bytes);

        let mut hasher = sha2::Sha256::new();
        hasher.update(&pk_bytes);
        let thumbprint = hex::encode(hasher.finalize());

        let challenge = b"AUTH_CHALLENGE_01";
        let signature: p256::ecdsa::Signature = signing_key.sign(challenge);
        let sig_hex = hex::encode(signature.to_der());

        let profile = NodeProfile {
            node_id: "secured-node".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 100,
            public_key: pk_hex.clone(),
        };

        let bad_thumbprints = vec!["unauthorized_thumbprint".to_string()];
        let res_unauth = registry.authenticate_and_register_node(
            profile.clone(),
            challenge,
            &sig_hex,
            &bad_thumbprints,
        );
        assert!(res_unauth.is_err());
        assert_eq!(
            res_unauth.unwrap_err(),
            "Node public key thumbprint not authorized in database"
        );

        let good_thumbprints = vec![thumbprint];
        let res_bad_sig = registry.authenticate_and_register_node(
            profile.clone(),
            challenge,
            "bad_signature_hex",
            &good_thumbprints,
        );
        assert!(res_bad_sig.is_err());

        let res_ok = registry.authenticate_and_register_node(
            profile,
            challenge,
            &sig_hex,
            &good_thumbprints,
        );
        assert!(res_ok.is_ok());
        assert!(registry.get_node("secured-node").is_some());
    }
}
