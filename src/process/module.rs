use anyhow::{anyhow, Result};
use std::time::Duration;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::ProcessStatus::{
    EnumProcessModules, GetModuleFileNameExA, GetModuleInformation, MODULEINFO,
};

use crate::process::SafeHandle;
use crate::with_handle;


/// Retrieves the name and associated information for all modules in a given process.
/// 
/// # Arguments
/// * `handle` - A safe handle to the target process
/// * `timeout` - Optional timeout duration for the operation
/// 
/// # Returns
/// Returns a vector of tuples containing (module name, module handle, module information)
/// 
/// # Example
/// ```rust,norun
/// use std::time::Duration;
/// 
/// let process_handle = get_process_handle(process_id)?;
/// let timeout = Some(Duration::from_secs(1));
/// let modules = get_named_modules(&process_handle, timeout)?;
/// 
/// for (name, handle, info) in modules {
///     println!("Module: {}, Base: {:?}, Size: {}", name, info.lpBaseOfDll, info.SizeOfImage);
/// }
/// ```
pub fn get_named_modules(
    handle: &SafeHandle,
    timeout: Option<Duration>,
) -> Result<Vec<(String, HMODULE, MODULEINFO)>> {
    let mut modules = Vec::with_capacity(1024);
    let mut bytes_needed = 0;

    with_handle!(handle, timeout, |guard| -> (), {
        unsafe {
            EnumProcessModules(
                *guard,
                modules.as_mut_ptr(),
                (modules.capacity() * std::mem::size_of::<HMODULE>()) as u32,
                &mut bytes_needed,
            )?;

            modules.set_len(bytes_needed as usize / std::mem::size_of::<HMODULE>());
        }

        Ok(())
    })?;

    let mut module_names: Vec<(String, HMODULE, MODULEINFO)> = Vec::with_capacity(modules.len());

    for &module in &modules {
        let mut name_raw = [0u8; 260];

        let length: u32 = with_handle!(handle, timeout, |guard| -> u32, {
            unsafe {
                Ok(GetModuleFileNameExA(*guard, module, &mut name_raw))
            }
        })?;

        let info: MODULEINFO = get_module_info(handle, module, timeout)?;

        if length > 0 {
            if let Ok(name) = String::from_utf8(
                name_raw[..name_raw
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(length as usize)]
                    .to_vec(),
            ) {
                module_names.push((name, module, info))
            }
        }
    }

    Ok(module_names)
}

/// Retrieves module location information from a given module.
/// 
/// # Arguments
/// * `handle` - A safe handle to the target process
/// * `module` - Handle to the module to get information about
/// * `timeout` - Optional timeout duration for the operation
/// 
/// # Returns
/// Returns a MODULEINFO structure containing the base address and size of the module
/// 
/// # Example
/// ```rust,norun
/// use std::time::Duration;
/// 
/// let process_handle = get_process_handle(process_id)?;
/// let module_handle = module_by_name(&process_handle, "kernel32.dll", true, None)?;
/// if let Some(module) = module_handle {
///     let info = get_module_info(&process_handle, module, None)?;
///     println!("Module base: {:?}, size: {}", info.lpBaseOfDll, info.SizeOfImage);
/// }
/// ```
pub fn get_module_info(
    handle: &SafeHandle,
    module: HMODULE,
    timeout: Option<Duration>,
) -> Result<MODULEINFO> {
    let mut info = MODULEINFO::default();

    with_handle!(handle, timeout, |guard| -> (), {
        unsafe {
            GetModuleInformation(
                *guard,
                module,
                &mut info,
                std::mem::size_of::<MODULEINFO>() as u32,
            )?;

            Ok(())
        }
    })?;

    Ok(info)
}

/// Retrieves a module from a given process by searching for its name.
/// 
/// # Arguments
/// * `handle` - A safe handle to the target process
/// * `name` - The name of the module to find
/// * `stem` - If true, matches only the filename part of the module path
/// * `timeout` - Optional timeout duration for the operation
/// 
/// # Returns
/// Returns Some(HMODULE) if the module is found, None otherwise
/// 
/// # Example
/// ```rust,norun
/// use std::time::Duration;
/// 
/// let process_handle = get_process_handle(process_id)?;
/// // Search for kernel32.dll by filename only
/// let kernel32 = module_by_name(&process_handle, "kernel32.dll", true, None)?;
/// if let Some(module) = kernel32 {
///     println!("Found kernel32.dll module: {:?}", module);
/// }
/// ```
pub fn module_by_name(
    handle: &SafeHandle,
    name: &str,
    stem: bool,
    timeout: Option<Duration>,
) -> Result<Option<HMODULE>> {
    let modules = match get_named_modules(handle, timeout) {
        Ok(m) => m,
        Err(e) => {
            return Err(anyhow!(
                "Obtaining module with name \"{name}\" failed with the following error: \"{e}\""
            ))
        }
    };

    for (mut module_name, module, _) in modules {
        if stem {
            module_name = module_name
                .split("\\")
                .last()
                .unwrap_or(&module_name)
                .to_string();
        }

        if module_name == name {
            return Ok(Some(module));
        }
    }

    Ok(None)
}
