// SPDX-License-Identifier: Apache-2.0
//! Per-user settings. Local only; no network access and no telemetry.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::baseline::BaselineSpec;
use crate::market::MarketProfile;
use crate::Result;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Settings {
    /// Optional diagnostics profile. Diagnostics stay disabled when absent.
    pub market_profile: Option<MarketProfile>,
    /// Optional local, dated reference-price baseline.
    pub baseline: Option<BaselineSpec>,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if let Some(profile) = &self.market_profile {
            profile.validate()?;
        }
        if let Some(baseline) = &self.baseline {
            if baseline.path.trim().is_empty() {
                return Err(crate::PreviewError::Baseline(
                    "settings baseline path is empty".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Load a settings file, returning defaults when it does not exist.
    pub fn load_from(path: &Path) -> Result<Self> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(crate::PreviewError::Limit(
                "settings file exceeds 1 MiB".to_string(),
            ));
        }
        let settings: Settings = serde_json::from_slice(&bytes)
            .map_err(|e| crate::PreviewError::Profile(format!("invalid settings JSON: {e}")))?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| crate::PreviewError::Profile(format!("cannot encode settings: {e}")))?;
        if json.len() > 1024 * 1024 {
            return Err(crate::PreviewError::Limit(
                "settings file exceeds 1 MiB".into(),
            ));
        }
        atomic_save_with(path, |file| file.write_all(&json))?;
        Ok(())
    }

    /// Load from `PARQUET_PREVIEW_SETTINGS` when set, otherwise the per-user
    /// config location. Missing files are not an error.
    pub fn load_default() -> Result<Self> {
        match default_settings_path() {
            Some(path) => Self::load_from(&path),
            None => Ok(Self::default()),
        }
    }
}

// Fully prepare a same-directory unique file before replacing existing bytes.
// No fallback truncation or destination deletion on failure.
fn atomic_save_with(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let (temp, mut file) = loop {
        let temp = parent.join(format!(
            ".parquet-settings-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = write(&mut file).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub fn default_settings_path() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("PARQUET_PREVIEW_SETTINGS") {
        if !value.trim().is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|dir| {
            PathBuf::from(dir)
                .join("ParquetPreview")
                .join("settings.json")
        })
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(
                PathBuf::from(dir)
                    .join("parquet-preview")
                    .join("settings.json"),
            );
        }
        std::env::var_os("HOME").map(|dir| {
            PathBuf::from(dir)
                .join(".config")
                .join("parquet-preview")
                .join("settings.json")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "parquet-preview-settings-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_errors_are_not_defaults_and_failed_save_preserves_previous_bytes() {
        let dir = temp_dir();
        assert!(Settings::load_from(&dir).is_err());
        let path = dir.join("settings.json");
        let good = b"{}";
        std::fs::write(&path, good).unwrap();
        let error = atomic_save_with(&path, |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write/flush failure"))
        });
        assert!(error.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), good);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let blocked = dir.join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(atomic_save_with(&blocked, |file| file.write_all(b"complete")).is_err());
        assert!(blocked.is_dir());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn defaults_round_trip() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        let settings = Settings::default();
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), settings);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_is_default() {
        let dir = temp_dir();
        assert_eq!(
            Settings::load_from(&dir.join("absent.json")).unwrap(),
            Settings::default()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_unknown_fields_and_bad_profiles() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"unknown": true}"#).unwrap();
        assert!(Settings::load_from(&path).is_err());
        std::fs::write(
            &path,
            r#"{"market_profile":{"time_field":"","open_field":"o","high_field":"h","low_field":"l","close_field":"c"}}"#,
        )
        .unwrap();
        assert!(Settings::load_from(&path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn serializes_market_profile() {
        let settings = Settings {
            market_profile: Some(MarketProfile {
                time_field: "ts".into(),
                open_field: "open".into(),
                high_field: "high".into(),
                low_field: "low".into(),
                close_field: "close".into(),
                spread_field: Some("spread".into()),
                instrument: Some("EURUSD".into()),
                thresholds: Default::default(),
            }),
            baseline: Some(BaselineSpec {
                path: "baseline.csv".into(),
                instrument: Some("EURUSD".into()),
                licence: Some("internal research".into()),
            }),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, settings);
    }
}
