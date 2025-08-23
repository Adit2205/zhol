use crate::error::IntoMemOpResult;
// use crate::hooks::hook::HookOps;
use crate::hooks::ZholHook;
use crate::memory::utils::{change_memory_protection, wait_for_safe_mem};
use crate::process::SafeHandle;
use crate::{with_handle, MemOpResult};
use anyhow::anyhow;
use std::time::Duration;

use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Memory::PAGE_EXECUTE_READWRITE;

// use crate::memory::transmute::Transmutable;

use super::transmute::ZholTyped;
use super::MemOpContext;

/// Writes a given byte slice to an address in process memory.
pub fn write_bytes(
    handle: &SafeHandle,
    addr: usize,
    bytes: &[u8],
    timeout: Option<Duration>,
) -> MemOpResult<()> {
    let mut bytes_written: usize = 0;
    let size = bytes.len();

    let old_protect =
        change_memory_protection(handle, addr, size, timeout, PAGE_EXECUTE_READWRITE)?;

    wait_for_safe_mem(handle, addr, timeout, true)?;
    with_handle!(&handle, timeout, |guard| -> (), {
        unsafe {
            // Write the bytes
            WriteProcessMemory(
                *guard,
                addr as *mut _,
                bytes.as_ptr() as *const _,
                bytes.len(),
                Some(&mut bytes_written),
            ).into_memop_result(Some(anyhow!("WriteProcessMemory in write_bytes()")))?
        };
        Ok(())
    })?;

    change_memory_protection(handle, addr, size, timeout, old_protect)?;

    // Verify all bytes were written
    if bytes_written != bytes.len() {
        return Err(anyhow!("An error prevented all bytes from being written.").into());
    }

    wait_for_safe_mem(handle, addr, timeout, true)?;

    std::thread::sleep(Duration::from_nanos(1));

    Ok(())
}

/// Transmutes a value to a byte slice and writes it to a given address in process memory.
pub fn write_value<T: ZholTyped<T>>(
    hook: &ZholHook,
    address: usize,
    value: T,
    timeout: Option<Duration>,
) -> MemOpResult<()> {
    // Use bytemuck to safely convert value to bytes
    // let bytes = T::byte_repr(&value);
    //
    let context = MemOpContext::new(address, 0x0, false, timeout);
    let bytes = &value.byte_repr(hook, &context)?;

    // Write the bytes to the process
    write_bytes(&hook.handle(), address, &bytes.to_vec(), timeout)?;

    Ok(())
}
