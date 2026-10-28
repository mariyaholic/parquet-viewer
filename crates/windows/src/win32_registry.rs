// SPDX-License-Identifier: Apache-2.0
//! Win32 HKCU registry implementation.

use crate::ffi;
use crate::registration::Registry;

pub struct Win32Registry;

impl Registry for Win32Registry {
    fn get_string(&self, key: &str, name: &str) -> Result<Option<String>, String> {
        let key = ffi::wide(key);
        let name = if name.is_empty() {
            None
        } else {
            Some(ffi::wide(name))
        };
        let name_ptr = name.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        let mut kind = 0u32;
        let mut size = 0u32;
        let status = unsafe {
            ffi::RegGetValueW(
                ffi::HKEY_CURRENT_USER,
                key.as_ptr(),
                name_ptr,
                ffi::RRF_RT_REG_SZ,
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status == ffi::ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ffi::ERROR_SUCCESS && status != ffi::ERROR_MORE_DATA {
            return Err(format!("RegGetValueW failed with status {status}"));
        }
        if kind != ffi::REG_SZ {
            return Err("registry value is not a string".to_string());
        }
        if size == 0 {
            return Ok(Some(String::new()));
        }
        let mut buffer = vec![0u16; (size as usize + 1) / 2 + 1];
        let mut size = (buffer.len() * 2) as u32;
        let status = unsafe {
            ffi::RegGetValueW(
                ffi::HKEY_CURRENT_USER,
                key.as_ptr(),
                name_ptr,
                ffi::RRF_RT_REG_SZ,
                &mut kind,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut size,
            )
        };
        if status != ffi::ERROR_SUCCESS {
            return Err(format!("RegGetValueW read failed with status {status}"));
        }
        let mut length = (size as usize) / 2;
        while length > 0 && buffer[length - 1] == 0 {
            length -= 1;
        }
        String::from_utf16(&buffer[..length])
            .map(Some)
            .map_err(|_| "registry string is not valid UTF-16".to_string())
    }

    fn set_string(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
        let key = ffi::wide(key);
        let name = if name.is_empty() {
            None
        } else {
            Some(ffi::wide(name))
        };
        let name_ptr = name.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        let value = ffi::wide(value);
        let status = unsafe {
            ffi::RegSetKeyValueW(
                ffi::HKEY_CURRENT_USER,
                key.as_ptr(),
                name_ptr,
                ffi::REG_SZ,
                value.as_ptr() as *const std::ffi::c_void,
                (value.len() * 2) as u32,
            )
        };
        if status != ffi::ERROR_SUCCESS {
            return Err(format!("RegSetKeyValueW failed with status {status}"));
        }
        Ok(())
    }

    fn delete_tree(&self, key: &str) -> Result<(), String> {
        let key = ffi::wide(key);
        let status = unsafe { ffi::RegDeleteTreeW(ffi::HKEY_CURRENT_USER, key.as_ptr()) };
        if status == ffi::ERROR_SUCCESS || status == ffi::ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("RegDeleteTreeW failed with status {status}"))
        }
    }
}

/// Handle of the DLL that contains this code (not the host executable).
pub fn current_module_handle() -> Result<ffi::HMODULE, String> {
    let mut module: ffi::HMODULE = 0;
    let address = current_module_handle as *const () as *const u16;
    let ok = unsafe {
        ffi::GetModuleHandleExW(
            ffi::GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                | ffi::GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            address,
            &mut module,
        )
    };
    if ok == 0 || module == 0 {
        return Err("GetModuleHandleExW failed for the handler module".to_string());
    }
    Ok(module)
}

/// Full path of the loaded DLL, queried from the module handle.
pub fn current_module_path() -> Result<String, String> {
    let module = current_module_handle()?;
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe {
        ffi::GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32)
    };
    if length == 0 {
        return Err("GetModuleFileNameW failed".to_string());
    }
    let length = length as usize;
    if length >= buffer.len() {
        return Err("module path is too long".to_string());
    }
    String::from_utf16(&buffer[..length]).map_err(|_| "module path is not UTF-16".to_string())
}
