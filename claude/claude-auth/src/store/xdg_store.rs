//! XDG file-based secure storage fallback with encryption

use super::SecureStore;
use crate::error::AuthError;
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

const NONCE_SIZE: usize = 12;

/// XDG-compliant file-based secret storage with encryption (fallback when keyring unavailable)
#[derive(Clone)]
pub struct XdgFileStore {
    base_path: PathBuf,
}

impl XdgFileStore {
    /// Create a new XDG file store for the given application name
    pub fn new(app_name: &str) -> Self {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(app_name);
        fs::create_dir_all(&base).ok();
        Self { base_path: base }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.base_path.join(format!("{name}.secret"))
    }

    /// Derive an encryption key from machine-specific data
    fn derive_key(&self) -> Key {
        // Use machine-id as the primary key material
        let machine_id = std::fs::read_to_string("/etc/machine-id")
            .unwrap_or_else(|_| "fallback-machine-id-for-testing".to_string());
        
        // Combine with the base path for additional uniqueness
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.base_path.to_string_lossy().as_bytes());
        hasher.update(machine_id.trim().as_bytes());
        let hash = hasher.finalize();
        
        // Use first 32 bytes of hash as key
        *Key::from_slice(hash.as_bytes())
    }

    /// Encrypt plaintext using ChaCha20Poly1305
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AuthError> {
        let key = self.derive_key();
        let cipher = ChaCha20Poly1305::new(&key);
        
        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AuthError::Storage(format!("Encryption failed: {e}")))?;
        
        // Prepend nonce to ciphertext
        let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        
        Ok(output)
    }

    /// Decrypt ciphertext using ChaCha20Poly1305
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, AuthError> {
        if data.len() < NONCE_SIZE {
            return Err(AuthError::Storage("Corrupt secret: too short".into()));
        }
        
        let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let key = self.derive_key();
        let cipher = ChaCha20Poly1305::new(&key);
        
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AuthError::Storage(format!("Decryption failed: {e}")))
    }
}

impl SecureStore for XdgFileStore {
    fn set_secret(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        let path = self.file_path(name);
        let encrypted = self.encrypt(value)?;
        fs::write(&path, encrypted).map_err(|e| AuthError::Storage(e.to_string()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|e| AuthError::Storage(e.to_string()))
    }

    fn get_secret(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        let path = self.file_path(name);
        match fs::read(&path) {
            Ok(data) => Ok(Some(self.decrypt(&data)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    fn delete_secret(&self, name: &str) -> Result<(), AuthError> {
        let path = self.file_path(name);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let dir = tempdir().unwrap();
        let store = XdgFileStore {
            base_path: dir.path().to_path_buf(),
        };
        
        let plaintext = b"test secret value";
        let encrypted = store.encrypt(plaintext).unwrap();
        let decrypted = store.decrypt(&encrypted).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypted_storage_roundtrip() {
        let dir = tempdir().unwrap();
        let store = XdgFileStore {
            base_path: dir.path().to_path_buf(),
        };
        
        let secret = b"my secret token";
        store.set_secret("test_key", secret).unwrap();
        
        let retrieved = store.get_secret("test_key").unwrap();
        assert_eq!(retrieved.as_deref(), Some(secret.as_slice()));
    }

    #[test]
    fn test_wrong_key_fails() {
        let dir = tempdir().unwrap();
        let store1 = XdgFileStore {
            base_path: dir.path().to_path_buf(),
        };
        
        let plaintext = b"test secret";
        let encrypted = store1.encrypt(plaintext).unwrap();
        
        // Create store with different base path (different key)
        let store2 = XdgFileStore {
            base_path: dir.path().join("different").to_path_buf(),
        };
        
        // Decryption should fail with different key
        assert!(store2.decrypt(&encrypted).is_err());
    }
}
