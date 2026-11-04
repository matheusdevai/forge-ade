//! Secret storage for MCP credentials. Production uses the OS keychain
//! (macOS Keychain, Windows Credential Manager, Secret Service); tests use an
//! in-memory store. Values never touch SQLite, logs or the frontend.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};

pub trait SecretStore: Send + Sync {
    fn set(&self, account: &str, value: &str) -> AppResult<()>;
    fn get(&self, account: &str) -> AppResult<Option<String>>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

pub type Secrets = Arc<dyn SecretStore>;

const SERVICE: &str = "dev.forge.desktop.mcp";

pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn set(&self, account: &str, value: &str) -> AppResult<()> {
        keyring::Entry::new(SERVICE, account)
            .and_then(|e| e.set_password(value))
            .map_err(|e| AppError::Internal(format!("cofre do sistema: {e}")))
    }
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        match keyring::Entry::new(SERVICE, account).and_then(|e| e.get_password()) {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Internal(format!("cofre do sistema: {e}"))),
        }
    }
    fn delete(&self, account: &str) -> AppResult<()> {
        match keyring::Entry::new(SERVICE, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Internal(format!("cofre do sistema: {e}"))),
        }
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn set(&self, account: &str, value: &str) -> AppResult<()> {
        self.0.lock().map_err(|_| AppError::Internal("lock".into()))?.insert(account.into(), value.into());
        Ok(())
    }
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().map_err(|_| AppError::Internal("lock".into()))?.get(account).cloned())
    }
    fn delete(&self, account: &str) -> AppResult<()> {
        self.0.lock().map_err(|_| AppError::Internal("lock".into()))?.remove(account);
        Ok(())
    }
}

pub fn account(server_id: &str, key: &str) -> String {
    format!("{server_id}:{key}")
}
