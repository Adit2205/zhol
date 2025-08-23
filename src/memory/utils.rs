use crate::error::IntoMemOpResult;
use crate::memory::MemoryRegion;
use crate::process::SafeHandle;
use crate::{with_handle, MemOpError};

use crate::error::{INVALID_ALLOCATION_TYPE, INVALID_PAGE_TYPE, INVALID_PROTECTION_FLAGS};
use crate::MemOpResult;
use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};
use windows::core::PWSTR;

use std::thread::park_timeout;
use std::time::Duration;

use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualProtectEx, VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT,
    MEM_MAPPED, MEM_PRIVATE, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_PROTECTION_FLAGS, PAGE_READONLY,
    PAGE_READWRITE, PAGE_WRITECOPY,
};

use windows::Win32::Foundation::HANDLE;

pub fn change_memory_protection(
    handle: &SafeHandle,
    addr: usize,
    size: usize,
    timeout: Option<Duration>,
    protection: PAGE_PROTECTION_FLAGS,
) -> MemOpResult<PAGE_PROTECTION_FLAGS> {
    let mut old_protect: PAGE_PROTECTION_FLAGS = PAGE_PROTECTION_FLAGS(0);

    with_handle!(handle, timeout, |guard| -> (), {
        unsafe {
            VirtualProtectEx(
                *guard,
                addr as *mut _,
                size,
                protection,
                &mut old_protect,
            ).into_memop_result(Some(anyhow!("VirtualProtectEx in change_memory_protection()")))?
        };
        Ok(())
        // Ok(MemOpResult::Ok(()))
        //
    })?;

    Ok(old_protect)
}

fn is_readable(protection: PAGE_PROTECTION_FLAGS) -> bool {
    // Define all readable protection constants
    const READABLE_FLAGS: &[u32] = &[
        PAGE_READONLY.0,          // 0x02
        PAGE_READWRITE.0,         // 0x04
        PAGE_WRITECOPY.0,         // 0x08
        PAGE_EXECUTE_READ.0,      // 0x20
        PAGE_EXECUTE_READWRITE.0, // 0x40
        PAGE_EXECUTE_WRITECOPY.0, // 0x80
    ];

    // Check if any readable flag is present in the protection
    READABLE_FLAGS
        .iter()
        .any(|&flag| (protection.0 & flag) == flag)
}

fn is_writable(protection: PAGE_PROTECTION_FLAGS) -> bool {
    // Check if any write access bit is set
    (protection.0
        & (PAGE_READWRITE.0
            | PAGE_WRITECOPY.0
            | PAGE_EXECUTE_READWRITE.0
            | PAGE_EXECUTE_WRITECOPY.0))
        != 0
}

fn mbi_safe_write(mbi: MEMORY_BASIC_INFORMATION) -> u8 {
    let mut mem_err_flag: u8 = 0b000;

    if mbi.State != MEM_COMMIT {
        mem_err_flag |= INVALID_ALLOCATION_TYPE;
    }

    if mbi.Type.0 == 0 {
        mem_err_flag |= INVALID_PAGE_TYPE;
    }

    // If it's mapped or private and has WRITECOPY, skip (whatever your logic demands)
    if (mbi.Type == MEM_MAPPED || mbi.Type == MEM_PRIVATE)
        && (mbi.Protect & PAGE_WRITECOPY) != PAGE_PROTECTION_FLAGS(0)
    {
        mem_err_flag |= INVALID_PAGE_TYPE;
        mem_err_flag |= INVALID_PROTECTION_FLAGS;
    }

    if (mbi.Protect.0 & PAGE_GUARD.0) != 0
        || mbi.Protect == PAGE_GUARD
        || mbi.Protect == PAGE_NOACCESS
    {
        mem_err_flag |= INVALID_PROTECTION_FLAGS;
    }

    if !is_writable(mbi.Protect) {
        mem_err_flag |= INVALID_PROTECTION_FLAGS;
    }

    mem_err_flag
}

fn mbi_safe_read(mbi: MEMORY_BASIC_INFORMATION) -> u8 {
    let mut mem_err_flag: u8 = 0b000;

    if (mbi.Protect.0 & PAGE_GUARD.0) != 0
        || mbi.Protect == PAGE_GUARD
        || mbi.Protect == PAGE_NOACCESS
    {
        mem_err_flag |= INVALID_PROTECTION_FLAGS;
    }

    if !is_readable(mbi.Protect) {
        mem_err_flag |= INVALID_PROTECTION_FLAGS;
    }

    mem_err_flag
}

pub fn mbi_safety_check(mbi: MEMORY_BASIC_INFORMATION, needs_write: bool) -> MemOpResult<bool> {
    let safety_flag: u8 = match needs_write {
        true => mbi_safe_write(mbi),
        false => mbi_safe_read(mbi),
    };

    return match safety_flag {
        0 => Ok(true),
        _ => Err(MemOpError::MemoryStateInvalid((
            mbi.State,
            mbi.Protect,
            mbi.Type,
            safety_flag,
            Some(anyhow!("mbi_safety_check, needs_write: {needs_write}")),
        ))),
    };
}

pub unsafe fn wait_for_safe_mem_unsafe(
    handle: HANDLE,
    address: usize,
    timeout: Option<Duration>,
    needs_write: bool,
) -> MemOpResult<()> {
    let timeout_dur = match timeout {
        Some(d) => d,
        None => Duration::from_secs(10), //Useless value - Slack
    };

    let mut timeout_remaining = timeout_dur;
    let beginning_park = std::time::Instant::now();
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    loop {
        unsafe {
            if VirtualQueryEx(
                handle,
                Some(address as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            ) == 0
            {
                Err(get_last_error())?
            }
        }

        if mbi_safety_check(mbi, needs_write)? {
            break;
        }

        park_timeout(timeout_remaining);
        let elapsed = match timeout {
            Some(_) => beginning_park.elapsed(),
            None => Duration::from_secs(0),
        };

        if elapsed >= timeout_dur {
            Err(anyhow!("Reached timeout before memory region was readable"))?
        }

        if timeout.is_some() {
            timeout_remaining = timeout_dur - elapsed
        }
    }

    Ok(())
}

pub fn wait_for_safe_mem(
    handle: &SafeHandle,
    address: usize,
    timeout: Option<Duration>,
    needs_write: bool,
) -> MemOpResult<()> {
    with_handle!(handle, timeout, |guard| -> (), {
        unsafe { wait_for_safe_mem_unsafe(*guard, address, timeout, needs_write) }
    })?;

    Ok(())
}

pub fn allocate_memory(handle: &SafeHandle, size: usize) -> MemOpResult<MemoryRegion> {
    let addr: usize = with_handle!(handle, Some(Duration::from_millis(10)), |guard| -> usize, {
        unsafe {
            let addr = VirtualAllocEx(
                *guard,
                None, // Let Windows decide the address
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            );

            if addr.is_null() {
                return MemOpResult::Err(get_last_error())
            }

            MemOpResult::Ok(addr as usize)
        }
    })?;

    let clone_handle_ref = handle.clone();

    Ok(MemoryRegion {
        handle: clone_handle_ref,
        addr,
        size,
    })
}

use windows::Win32::Foundation::GetLastError;
use windows::Win32::System::Diagnostics::Debug::{
    FormatMessageW, FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM,
    FORMAT_MESSAGE_IGNORE_INSERTS,
};
use windows_result::HRESULT;

use super::transmute::AutoImplTransmutable;

pub fn get_last_error() -> MemOpError {
    unsafe {
        let error_code = GetLastError();
        let buffer = PWSTR::null();

        let format_result = FormatMessageW(
            FORMAT_MESSAGE_ALLOCATE_BUFFER
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_IGNORE_INSERTS,
            None,
            error_code.0,
            0, // Default language
            buffer,
            0,
            None,
        );

        let err = {
            let mut message = String::new();
            if format_result > 0 && !buffer.is_null() {
                message = buffer.to_string().unwrap_or(String::new());
            }

            windows_result::Error::new(HRESULT::from_win32(error_code.0), message)
        };

        MemOpError::WinAPI((err, None))
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CStr256 {
    data: [u8; 256],
}
impl AutoImplTransmutable for CStr256 {}

impl CStr256 {
    pub fn as_str(&self) -> &str {
        // Scan for the first 0 in `data`
        let end = self
            .data
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.data.len());
        std::str::from_utf8(&self.data[..end]).unwrap_or("")
    }
    pub fn try_from_str(s: &str) -> Result<Self, MemOpError> {
        let bytes = s.as_bytes();

        if bytes.len() > 255 {
            return Err(MemOpError::Other(anyhow!("string too long for CStr256")));
        }

        let mut data = [0u8; 256];
        data[..bytes.len()].copy_from_slice(bytes);
        Ok(CStr256 { data })
    }
}

impl From<&str> for CStr256 {
    fn from(s: &str) -> Self {
        let mut data = [0u8; 256];
        let bytes = s.as_bytes();

        // Leave space for null-terminator
        let len = bytes.len().min(255);
        data[..len].copy_from_slice(&bytes[..len]);

        // The array is already zero-initialized — null terminator is guaranteed
        CStr256 { data }
    }
}

impl std::fmt::Display for CStr256 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
