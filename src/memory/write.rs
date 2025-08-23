use crate::error::IntoMemOpResult;
// use crate::hooks::hook::HookOps;
use crate::hooks::ZholHook;
use crate::memory::read::read_value;
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

/// Top-level write function.
///
/// Use this for writing types directly to game memory.
/// Value must implement bytemuck::Pod.
pub fn write<T: ZholTyped<T>>(
    hook: &ZholHook,
    value: T,
    context: &MemOpContext,
) -> MemOpResult<()> {
    let data = hook.data().read();
    let ptr: usize = match context.at_pointer {
        true => read_value::<i32>(&hook, data.var_mem.addr, context.timeout)? as usize,
        false => data.var_mem.addr,
    };

    drop(data);

    write_value::<T>(&hook, ptr + context.offset, value, context.timeout)
}
