use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::TryRngCore;
use zeroize::Zeroize;

use crate::VaultError;

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// AES-256-GCM encryption/decryption for secrets at rest.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct VaultEncryption {
    key: [u8; KEY_LEN],
}

impl VaultEncryption {
    /// Create a new encryption instance with a randomly generated key.
    pub fn new() -> Self {
        Self {
            key: generate_key(),
        }
    }

    /// Create from an existing key (e.g., loaded from environment or file).
    pub fn from_key(key: [u8; KEY_LEN]) -> Self {
        Self { key }
    }

    /// Export the current key as hex string (for backup).
    pub fn export_key(&self) -> String {
        hex::encode(self.key)
    }

    /// Encrypt plaintext bytes. Returns `nonce || ciphertext`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| VaultError::Encryption(e.to_string()))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        let mut os_rng = rand::rngs::OsRng;
        os_rng
            .try_fill_bytes(&mut nonce_bytes)
            .expect("OsRng failure: system entropy source unavailable");
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| VaultError::Encryption(e.to_string()))?;

        let mut result = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        result.extend_from_slice(nonce);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypt bytes produced by `encrypt`.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, VaultError> {
        if data.len() < NONCE_LEN {
            return Err(VaultError::Encryption("data too short".into()));
        }

        let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| VaultError::Encryption(e.to_string()))?;

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| VaultError::Encryption("decryption failed".into()))
    }

    /// Encrypt a JSON-serializable value.
    pub fn encrypt_json<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>, VaultError> {
        let json = serde_json::to_vec(value)
            .map_err(|e| VaultError::Encryption(format!("serialization error: {e}")))?;
        self.encrypt(&json)
    }

    /// Decrypt into a JSON-deserializable value.
    pub fn decrypt_json<T: serde::de::DeserializeOwned>(
        &self,
        data: &[u8],
    ) -> Result<T, VaultError> {
        let plaintext = self.decrypt(data)?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| VaultError::Encryption(format!("deserialization error: {e}")))
    }
}

impl Default for VaultEncryption {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for VaultEncryption {
    fn clone(&self) -> Self {
        Self { key: self.key }
    }
}

impl std::fmt::Debug for VaultEncryption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultEncryption").finish()
    }
}

fn generate_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    let mut os_rng = rand::rngs::OsRng;
    os_rng
        .try_fill_bytes(&mut key)
        .expect("OsRng failure: system entropy source unavailable");
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let enc = VaultEncryption::new();
        let data = b"Hello, AgentOS vault!";
        let encrypted = enc.encrypt(data).unwrap();
        assert_ne!(encrypted, data);
        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_encrypt_json_roundtrip() {
        let enc = VaultEncryption::new();
        let value = serde_json::json!({
            "agent_id": "agent-1",
            "api_key": "sk-123456",
            "permissions": ["read", "write"]
        });
        let encrypted = enc.encrypt_json(&value).unwrap();
        let decrypted: serde_json::Value = enc.decrypt_json(&encrypted).unwrap();
        assert_eq!(decrypted, value);
    }

    #[test]
    fn test_different_keys_fail() {
        let enc1 = VaultEncryption::new();
        let enc2 = VaultEncryption::new();
        let data = b"secret data";
        let encrypted = enc1.encrypt(data).unwrap();
        let result = enc2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_key_and_export() {
        let key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let enc = VaultEncryption::from_key(key);
        let exported = enc.export_key();
        assert_eq!(exported.len(), 64);
        let data = b"test";
        let encrypted = enc.encrypt(data).unwrap();
        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_empty_data_encrypt() {
        let enc = VaultEncryption::new();
        let encrypted = enc.encrypt(b"").unwrap();
        assert!(encrypted.len() > NONCE_LEN);
        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_decrypt_invalid_data() {
        let enc = VaultEncryption::new();
        let result = enc.decrypt(b"too short");
        assert!(result.is_err());
    }
}
