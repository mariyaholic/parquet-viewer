// SPDX-License-Identifier: Apache-2.0
//! Per-user settings. Local only; no network access and no telemetry.

use std::path::{Path, PathBuf};

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
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path)?;
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
        std::fs::write(path, json)?;
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
