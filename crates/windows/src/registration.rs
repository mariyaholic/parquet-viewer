// SPDX-License-Identifier: Apache-2.0
//! Per-user preview-handler registration logic, independent of the registry API.
//!
//! Registration writes under `HKCU\Software\Classes` so it is opt-in and does
//! not require elevation. The prior `.parquet` association is backed up and is
//! only restored on uninstall when the current association is still ours.

use serde::{Deserialize, Serialize};

pub const CLSID: &str = "{7F2C9E4A-2B6D-4B7E-9C3A-5D1E8A0F6B21}";
pub const PREVIEW_HANDLER_SHELLEXT: &str = "{8895B1C6-B41F-4C1C-A562-0D564250836F}";
pub const EXTENSION: &str = ".parquet";
const DISPLAY_NAME: &str = "Parquet viewer and preview";
const PREVHOST_APPID: &str = "{6D2B5079-2F0B-48DD-AB7F-97CEC514D30B}";
const PREVIEW_HANDLERS: &str = r"Software\Microsoft\Windows\CurrentVersion\PreviewHandlers";

const BACKUP: &str = r"Software\ParquetPreview\Backup";

fn shell_ex_key() -> String {
    format!(r"Software\Classes\{EXTENSION}\ShellEx\{PREVIEW_HANDLER_SHELLEXT}")
}

fn clsid_key() -> String {
    format!(r"Software\Classes\CLSID\{CLSID}")
}

fn inproc_key() -> String {
    format!(r"Software\Classes\CLSID\{CLSID}\InprocServer32")
}

pub fn shell_ex_key_name() -> String {
    shell_ex_key()
}

pub fn clsid_key_name() -> String {
    clsid_key()
}

pub fn inproc_key_name() -> String {
    inproc_key()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationReport {
    pub handler_clsid: String,
    pub extension: String,
    pub previous_handler: Option<String>,
    pub overwrote_existing: bool,
    pub dll_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    Registry(String),
    Conflict(String),
    InvalidDllPath,
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::Registry(message) => {
                write!(f, "registry error: {message}")
            }
            RegistrationError::Conflict(existing) => write!(
                f,
                "another preview handler is already registered for {EXTENSION}: {existing}"
            ),
            RegistrationError::InvalidDllPath => write!(f, "DLL path is empty"),
        }
    }
}

impl std::error::Error for RegistrationError {}

/// Minimal registry surface used by registration. Default values are addressed
/// by an empty `name`.
pub trait Registry {
    fn get_string(&self, key: &str, name: &str) -> Result<Option<String>, String>;
    fn set_string(&self, key: &str, name: &str, value: &str) -> Result<(), String>;
    fn delete_tree(&self, key: &str) -> Result<(), String>;
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String>;
}

/// In-memory registry for tests, mutable via interior mutability.
#[derive(Debug, Clone, Default)]
pub struct TestRegistry {
    values: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<(String, String), String>>>,
}

impl TestRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_extension_handler(handler: &str) -> Self {
        let registry = Self::new();
        registry
            .set_string(&shell_ex_key(), "", handler)
            .expect("test registry");
        registry
    }

    pub fn value(&self, key: &str, name: &str) -> Option<String> {
        self.values
            .lock()
            .unwrap()
            .get(&(key.to_string(), name.to_string()))
            .cloned()
    }

    pub fn key_exists(&self, key: &str) -> bool {
        self.values
            .lock()
            .unwrap()
            .keys()
            .any(|(existing, _)| existing == key || existing.starts_with(&format!("{key}\\")))
    }

    pub fn set_raw(&self, key: &str, name: &str, value: &str) {
        self.values
            .lock()
            .unwrap()
            .insert((key.to_string(), name.to_string()), value.to_string());
    }

    pub fn remove_raw(&self, key: &str, name: &str) {
        self.values
            .lock()
            .unwrap()
            .remove(&(key.to_string(), name.to_string()));
    }
}

impl Registry for TestRegistry {
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
        self.remove_raw(key, name);
        Ok(())
    }
    fn get_string(&self, key: &str, name: &str) -> Result<Option<String>, String> {
        Ok(self.value(key, name))
    }

    fn set_string(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
        self.set_raw(key, name, value);
        Ok(())
    }

    fn delete_tree(&self, key: &str) -> Result<(), String> {
        let prefix = format!("{key}\\");
        self.values
            .lock()
            .unwrap()
            .retain(|(existing, _), _| existing != key && !existing.starts_with(&prefix));
        Ok(())
    }
}

/// Best-effort restore of the `.parquet` association after a failed `register`.
///
/// ShellEx is written last, so this fires only when the association write itself
/// left our CLSID behind. It never overwrites a value that is not ours, and a
/// restore failure is swallowed so the caller sees the original error.
fn restore_shellex(registry: &dyn Registry, shell: &str, previous: Option<&str>) {
    if let Ok(current) = registry.get_string(shell, "") {
        if current
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(CLSID))
        {
            match previous {
                Some(previous) if !previous.is_empty() => {
                    let _ = registry.set_string(shell, "", previous);
                }
                _ => {
                    let _ = registry.delete_value(shell, "");
                }
            }
        }
    }
}

/// Register the DLL for the current user.
///
/// `allow_overwrite` must only be true after the installer has warned about an
/// existing handler. The prior default value is always backed up first.
pub fn register(
    registry: &dyn Registry,
    dll_path: &str,
    allow_overwrite: bool,
) -> Result<RegistrationReport, RegistrationError> {
    let dll_path = dll_path.trim();
    if dll_path.is_empty() {
        return Err(RegistrationError::InvalidDllPath);
    }
    let shell = shell_ex_key();
    let previous = registry
        .get_string(&shell, "")
        .map_err(RegistrationError::Registry)?;
    let overwrote = previous
        .as_deref()
        .map(|value| !value.eq_ignore_ascii_case(CLSID))
        .unwrap_or(false);
    if overwrote && !allow_overwrite {
        return Err(RegistrationError::Conflict(previous.unwrap_or_default()));
    }

    // Back up the association we are about to replace. Our own re-registrations
    // keep the pristine pre-install backup: when `previous` is our CLSID there is
    // nothing new to save. But a foreign association that took over since our
    // last install is the state uninstall must restore, so it overwrites the
    // stale backup; otherwise uninstall would erase the third party's working
    // association even though the installer promised to restore it.
    let backup = registry
        .get_string(BACKUP, "")
        .map_err(RegistrationError::Registry)?;
    if backup.is_none() || overwrote {
        registry
            .set_string(BACKUP, "", previous.as_deref().unwrap_or(""))
            .map_err(RegistrationError::Registry)?;
    }

    // Microsoft, "How to register a preview handler": register the surrogate
    // AppID and list the handler under PreviewHandlers as well as ShellEx.
    //
    // Order matters. The server definition (CLSID + InprocServer32) is written
    // before the `.parquet` association, so a failure part-way through never
    // leaves `.parquet` pointing at a CLSID with no registered server. ShellEx is
    // written last; on any failure the previous association is restored
    // best-effort (a full transactional registry writer is out of scope here).
    let outcome: Result<(), RegistrationError> = (|| {
        registry
            .set_string(&clsid_key(), "AppID", PREVHOST_APPID)
            .map_err(RegistrationError::Registry)?;
        registry
            .set_string(&clsid_key(), "DisplayName", DISPLAY_NAME)
            .map_err(RegistrationError::Registry)?;
        registry
            .set_string(&inproc_key(), "", dll_path)
            .map_err(RegistrationError::Registry)?;
        registry
            .set_string(&inproc_key(), "ThreadingModel", "Apartment")
            .map_err(RegistrationError::Registry)?;
        registry
            .set_string(PREVIEW_HANDLERS, CLSID, DISPLAY_NAME)
            .map_err(RegistrationError::Registry)?;
        registry
            .set_string(&shell, "", CLSID)
            .map_err(RegistrationError::Registry)?;
        Ok(())
    })();
    if let Err(error) = outcome {
        restore_shellex(registry, &shell, previous.as_deref());
        return Err(error);
    }

    Ok(RegistrationReport {
        handler_clsid: CLSID.to_string(),
        extension: EXTENSION.to_string(),
        previous_handler: previous,
        overwrote_existing: overwrote,
        dll_path: dll_path.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnregistrationReport {
    pub restored_handler: Option<String>,
    pub left_foreign_handler: Option<String>,
    pub had_backup: bool,
}

/// Remove the handler and restore the previous `.parquet` association only when
/// it is still safe to do so.
pub fn unregister(registry: &dyn Registry) -> Result<UnregistrationReport, RegistrationError> {
    let shell = shell_ex_key();
    let had_backup = registry
        .get_string(BACKUP, "")
        .map_err(RegistrationError::Registry)?
        .is_some();
    let backup = registry
        .get_string(BACKUP, "")
        .map_err(RegistrationError::Registry)?;
    let current = registry
        .get_string(&shell, "")
        .map_err(RegistrationError::Registry)?;
    let mut restored = None;
    let mut left_foreign = None;

    if let Some(current) = current {
        if current.eq_ignore_ascii_case(CLSID) {
            match backup.as_deref() {
                Some(previous) if !previous.is_empty() => {
                    registry
                        .set_string(&shell, "", previous)
                        .map_err(RegistrationError::Registry)?;
                    restored = Some(previous.to_string());
                }
                _ => {
                    // Only the association value is removed; the empty keys are
                    // left in place (see the uninstall note in the README). There
                    // is no atomic "delete only if empty" registry operation, so
                    // pruning them risks deleting a value another process added
                    // between the emptiness check and the delete.
                    registry
                        .delete_value(&shell, "")
                        .map_err(RegistrationError::Registry)?;
                }
            }
        } else {
            // Someone else registered after us; do not clobber their choice.
            left_foreign = Some(current);
        }
    }

    registry
        .delete_value(PREVIEW_HANDLERS, CLSID)
        .map_err(RegistrationError::Registry)?;
    registry
        .delete_tree(&clsid_key())
        .map_err(RegistrationError::Registry)?;
    registry
        .delete_tree(BACKUP)
        .map_err(RegistrationError::Registry)?;

    Ok(UnregistrationReport {
        restored_handler: restored,
        left_foreign_handler: left_foreign,
        had_backup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DLL: &str = r"C:\Program Files\ParquetPreview\parquet_preview.dll";

    /// Wraps a `TestRegistry` and fails the `set_string` whose key and name
    /// match a target. Key-based (not call-count based) so the test does not
    /// depend on the write order the fix is about to change.
    struct FailsOnWrite {
        inner: TestRegistry,
        target_key: String,
        target_name: String,
    }

    impl FailsOnWrite {
        fn new(inner: TestRegistry, key: &str, name: &str) -> Self {
            Self {
                inner,
                target_key: key.to_string(),
                target_name: name.to_string(),
            }
        }
    }

    impl Registry for FailsOnWrite {
        fn get_string(&self, key: &str, name: &str) -> Result<Option<String>, String> {
            self.inner.get_string(key, name)
        }
        fn set_string(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
            if key == self.target_key && name == self.target_name {
                return Err("injected registry failure".to_string());
            }
            self.inner.set_string(key, name, value)
        }
        fn delete_tree(&self, key: &str) -> Result<(), String> {
            self.inner.delete_tree(key)
        }
        fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
            self.inner.delete_value(key, name)
        }
    }

    #[test]
    fn fresh_registration_sets_values() {
        let registry = TestRegistry::new();
        let report = register(&registry, DLL, false).unwrap();
        assert!(!report.overwrote_existing);
        assert_eq!(report.previous_handler, None);
        assert_eq!(registry.value(&shell_ex_key(), "").as_deref(), Some(CLSID));
        assert_eq!(registry.value(&inproc_key(), "").as_deref(), Some(DLL));
        assert_eq!(
            registry.value(&inproc_key(), "ThreadingModel").as_deref(),
            Some("Apartment")
        );
        assert_eq!(registry.value(BACKUP, "").as_deref(), Some(""));
        assert_eq!(
            registry.value(&clsid_key(), "AppID").as_deref(),
            Some(PREVHOST_APPID)
        );
        assert_eq!(
            registry.value(&clsid_key(), "DisplayName").as_deref(),
            Some(DISPLAY_NAME)
        );
        assert_eq!(
            registry.value(PREVIEW_HANDLERS, CLSID).as_deref(),
            Some(DISPLAY_NAME)
        );
    }

    #[test]
    fn existing_handler_blocks_registration_without_consent() {
        let registry = TestRegistry::with_extension_handler("{OTHER-HANDLER}");
        let error = register(&registry, DLL, false).unwrap_err();
        assert!(matches!(error, RegistrationError::Conflict(_)));
        assert_eq!(
            registry.value(&shell_ex_key(), "").as_deref(),
            Some("{OTHER-HANDLER}")
        );
        assert_eq!(registry.value(&inproc_key(), ""), None);
    }

    #[test]
    fn forced_registration_backs_up_and_restores() {
        let registry = TestRegistry::with_extension_handler("{OTHER-HANDLER}");
        let report = register(&registry, DLL, true).unwrap();
        assert!(report.overwrote_existing);
        assert_eq!(report.previous_handler.as_deref(), Some("{OTHER-HANDLER}"));
        assert_eq!(
            registry.value(BACKUP, "").as_deref(),
            Some("{OTHER-HANDLER}")
        );

        let report = unregister(&registry).unwrap();
        assert_eq!(report.restored_handler.as_deref(), Some("{OTHER-HANDLER}"));
        assert_eq!(
            registry.value(&shell_ex_key(), "").as_deref(),
            Some("{OTHER-HANDLER}")
        );
        assert!(!registry.key_exists(&clsid_key()));
        assert!(!registry.key_exists(BACKUP));
    }

    #[test]
    fn unregister_fresh_installation_removes_association() {
        let registry = TestRegistry::new();
        register(&registry, DLL, false).unwrap();
        registry.set_raw(&shell_ex_key(), "foreign", "keep");
        unregister(&registry).unwrap();
        assert_eq!(
            registry.value(&shell_ex_key(), "foreign").as_deref(),
            Some("keep")
        );
        assert_eq!(registry.value(PREVIEW_HANDLERS, CLSID), None);
        assert_eq!(registry.value(&shell_ex_key(), ""), None);
        assert!(!registry.key_exists(&clsid_key()));
    }

    #[test]
    fn unregister_leaves_foreign_handler_in_place() {
        let registry = TestRegistry::new();
        register(&registry, DLL, false).unwrap();
        registry.set_raw(&shell_ex_key(), "", "{SOMEONE-ELSE}");
        let report = unregister(&registry).unwrap();
        assert_eq!(
            report.left_foreign_handler.as_deref(),
            Some("{SOMEONE-ELSE}")
        );
        assert_eq!(
            registry.value(&shell_ex_key(), "").as_deref(),
            Some("{SOMEONE-ELSE}")
        );
        assert!(!registry.key_exists(&clsid_key()));
    }

    #[test]
    fn repeated_registration_keeps_the_original_backup() {
        let registry = TestRegistry::with_extension_handler("{OTHER-HANDLER}");
        register(&registry, DLL, true).unwrap();
        let report = register(&registry, "C:\\new.dll", true).unwrap();
        assert_eq!(
            report.previous_handler.as_deref(),
            Some(CLSID),
            "second run sees our own handler as previous"
        );
        assert_eq!(
            registry.value(BACKUP, "").as_deref(),
            Some("{OTHER-HANDLER}")
        );
        assert_eq!(
            registry.value(&inproc_key(), "").as_deref(),
            Some("C:\\new.dll")
        );
    }

    /// L1: a third party that takes the association over after our install is
    /// the state uninstall must restore. Re-registering over it refreshes the
    /// backup; the pristine first-only rule still holds for our own updates.
    #[test]
    fn re_registration_after_takeover_restores_the_new_handler_on_unregister() {
        // Initial foreign handler, then our install backs it up.
        let registry = TestRegistry::with_extension_handler("{OTHER-HANDLER}");
        register(&registry, DLL, true).unwrap();
        assert_eq!(
            registry.value(BACKUP, "").as_deref(),
            Some("{OTHER-HANDLER}")
        );

        // A third party takes the association over while we are installed.
        registry.set_raw(&shell_ex_key(), "", "{OTHER-HANDLER-2}");

        // Our re-register must back up the new foreign handler.
        let report = register(&registry, DLL, true).unwrap();
        assert!(report.overwrote_existing);
        assert_eq!(
            report.previous_handler.as_deref(),
            Some("{OTHER-HANDLER-2}")
        );
        assert_eq!(
            registry.value(BACKUP, "").as_deref(),
            Some("{OTHER-HANDLER-2}"),
            "the stale backup must be replaced by the new foreign handler"
        );

        // Uninstall restores the third party, not the first handler and not an
        // empty (deleted) value.
        let report = unregister(&registry).unwrap();
        assert_eq!(
            report.restored_handler.as_deref(),
            Some("{OTHER-HANDLER-2}")
        );
        assert_eq!(
            registry.value(&shell_ex_key(), "").as_deref(),
            Some("{OTHER-HANDLER-2}")
        );
    }

    /// L2: the `.parquet` association is written after the server definition,
    /// so a failure writing the in-process server leaves the existing third
    /// party association untouched instead of pointing `.parquet` at a CLSID
    /// with no registered server.
    #[test]
    fn register_keeps_shellex_unchanged_when_a_server_write_fails() {
        let inner = TestRegistry::with_extension_handler("{OTHER}");
        let registry = FailsOnWrite::new(inner.clone(), &inproc_key(), "");
        let error = register(&registry, DLL, true).unwrap_err();
        assert!(matches!(error, RegistrationError::Registry(_)));
        assert_eq!(
            inner.value(&shell_ex_key(), "").as_deref(),
            Some("{OTHER}"),
            "the association must be untouched when a server write fails"
        );
        assert_eq!(
            inner.value(&inproc_key(), ""),
            None,
            "the failed server default must not be recorded"
        );
    }

    /// L2 rollback: `restore_shellex` puts the previous handler back only when
    /// the current value is ours, and removes our value when there was none.
    #[test]
    fn restore_shellex_puts_the_previous_handler_back() {
        let registry = TestRegistry::new();
        registry.set_raw(&shell_ex_key(), "", CLSID);
        restore_shellex(&registry, &shell_ex_key(), Some("{OTHER}"));
        assert_eq!(
            registry.value(&shell_ex_key(), "").as_deref(),
            Some("{OTHER}")
        );

        let registry = TestRegistry::new();
        registry.set_raw(&shell_ex_key(), "", CLSID);
        restore_shellex(&registry, &shell_ex_key(), None);
        assert_eq!(registry.value(&shell_ex_key(), ""), None);

        // A value that is not ours is never overwritten.
        let registry = TestRegistry::new();
        registry.set_raw(&shell_ex_key(), "", "{SOMEONE-ELSE}");
        restore_shellex(&registry, &shell_ex_key(), Some("{OTHER}"));
        assert_eq!(
            registry.value(&shell_ex_key(), "").as_deref(),
            Some("{SOMEONE-ELSE}")
        );
    }

    #[test]
    fn empty_dll_path_is_rejected() {
        assert!(matches!(
            register(&TestRegistry::new(), "  ", false),
            Err(RegistrationError::InvalidDllPath)
        ));
    }

    #[test]
    fn key_names_are_per_user_and_specific() {
        assert!(shell_ex_key().starts_with(r"Software\Classes"));
        assert!(shell_ex_key().contains(EXTENSION));
        assert!(shell_ex_key().contains(PREVIEW_HANDLER_SHELLEXT));
        assert!(clsid_key().contains(CLSID));
        assert!(inproc_key().ends_with("InprocServer32"));
    }
}
