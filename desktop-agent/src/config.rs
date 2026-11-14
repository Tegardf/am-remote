//! Persistent agent configuration: port, paired devices and the autostart flag.
//!
//! Stored in `%APPDATA%\am-remote\config.json`. The file contains tokens, so it is
//! written atomically (temp file, then rename) and never logged.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tokio::sync::broadcast;

pub const DEFAULT_PORT: u16 = 8765;

fn default_port() -> u16 {
    DEFAULT_PORT
}

/// A phone that completed pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub name: String,
    pub token: String,
    /// Unix time in seconds.
    pub paired_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub devices: Vec<PairedDevice>,
    #[serde(default)]
    pub autostart: bool,
    /// Two-letter country of the Apple Music catalog to search (for example `id`). Optional: when
    /// missing, the country of the Windows region settings is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storefront: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            devices: Vec::new(),
            autostart: false,
            storefront: None,
        }
    }
}

impl Config {
    /// Loads the config. A missing file gives the defaults; a corrupt file is an error
    /// (so a typo never silently drops every paired device).
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            // Windows editors (Notepad, PowerShell 5) like to start a UTF-8 file with a byte-order mark.
            Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .with_context(|| format!("config file {} is not valid JSON", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Atomic save: write `<file>.tmp` next to the target, then rename over it.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let json = serde_json::to_string_pretty(self).context("cannot serialize config")?;
        fs::write(&tmp, json).with_context(|| format!("cannot write {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }
}

/// `%APPDATA%\am-remote\config.json`.
pub fn default_path() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("%APPDATA% is not set")?;
    Ok(PathBuf::from(appdata).join("am-remote").join("config.json"))
}

/// Thread-safe holder of the config that saves after every change.
#[derive(Debug)]
pub struct ConfigStore {
    path: PathBuf,
    inner: Mutex<Config>,
    /// Pinged after every revocation, so open WebSockets can re-check their token at once.
    revoked: broadcast::Sender<()>,
}

impl ConfigStore {
    pub fn new(path: PathBuf, config: Config) -> Self {
        Self {
            path,
            inner: Mutex::new(config),
            revoked: broadcast::channel(16).0,
        }
    }

    /// Receives a message each time a device is revoked. Re-check `is_authorized` when it fires.
    pub fn subscribe_revocations(&self) -> broadcast::Receiver<()> {
        self.revoked.subscribe()
    }

    /// Loads `path` (defaults if missing) into a new store.
    pub fn open(path: PathBuf) -> Result<Self> {
        let config = Config::load(&path)?;
        Ok(Self::new(path, config))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Config> {
        // A poisoned lock only means another thread panicked; the data is still valid.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The configured catalog country, if the user set one.
    pub fn storefront(&self) -> Option<String> {
        self.lock().storefront.clone()
    }

    pub fn port(&self) -> u16 {
        self.lock().port
    }

    pub fn devices(&self) -> Vec<PairedDevice> {
        self.lock().devices.clone()
    }

    /// True if `token` belongs to a paired device. Compares against every stored token
    /// in constant time per comparison, without returning early on a match.
    pub fn is_authorized(&self, token: &str) -> bool {
        let config = self.lock();
        let mut found = 0u8;
        for device in &config.devices {
            found |= device.token.as_bytes().ct_eq(token.as_bytes()).unwrap_u8();
        }
        found == 1
    }

    /// Adds a device and saves. If saving fails the device is not kept.
    pub fn add_device(&self, device: PairedDevice) -> Result<()> {
        let mut config = self.lock();
        config.devices.push(device);
        if let Err(e) = config.save(&self.path) {
            config.devices.pop();
            return Err(e);
        }
        Ok(())
    }

    /// Removes the device with this token and saves. Returns whether one was removed.
    /// The token stops working immediately, even if saving fails. In that rare case the returned
    /// error is the only warning: the file still lists the device, so it would be valid again after
    /// a restart unless a later save (any pairing or revoke writes the whole file) succeeds.
    pub fn revoke(&self, token: &str) -> Result<bool> {
        let mut config = self.lock();
        let before = config.devices.len();
        config.devices.retain(|d| d.token != token);
        let removed = config.devices.len() != before;
        if removed {
            // The token is already invalid: tell open sockets before the (slower) save.
            let _ = self.revoked.send(());
            config.save(&self.path)?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, token: &str) -> PairedDevice {
        PairedDevice {
            name: name.into(),
            token: token.into(),
            paired_at: 1_700_000_000,
        }
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&dir.path().join("config.json")).unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.port, 8765);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let config = Config {
            port: 9000,
            devices: vec![device("Pixel 8", "FAKE-TOKEN-1")],
            autostart: true,
            storefront: Some("id".into()),
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file is renamed away"
        );
    }

    #[test]
    fn a_byte_order_mark_at_the_start_is_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "\u{feff}{ \"port\": 9100 }").unwrap();
        assert_eq!(Config::load(&path).unwrap().port, 9100);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{}").unwrap();
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    #[test]
    fn store_authorizes_and_revokes_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::open(path.clone()).unwrap();
        assert!(!store.is_authorized("FAKE-TOKEN-1"));

        store.add_device(device("Pixel 8", "FAKE-TOKEN-1")).unwrap();
        store.add_device(device("Tablet", "FAKE-TOKEN-2")).unwrap();
        assert!(store.is_authorized("FAKE-TOKEN-1"));
        assert!(store.is_authorized("FAKE-TOKEN-2"));
        assert!(!store.is_authorized("FAKE-TOKEN-"));
        assert!(!store.is_authorized(""));

        // A fresh store sees the saved devices.
        let reopened = ConfigStore::open(path).unwrap();
        assert_eq!(reopened.devices().len(), 2);

        assert!(store.revoke("FAKE-TOKEN-1").unwrap());
        assert!(!store.is_authorized("FAKE-TOKEN-1"));
        assert!(store.is_authorized("FAKE-TOKEN-2"));
        assert!(!store.revoke("FAKE-TOKEN-1").unwrap());
    }
}
