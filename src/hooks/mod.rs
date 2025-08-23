use crate::asm::{handle_x86_asm_build, newmem_jmp};
use crate::memory::utils::allocate_memory;

use crate::memory::{
    read::read_bytes, utils::change_memory_protection, write::write_bytes, Byte, MemOpContext,
    MemoryRegion,
};
use crate::process::module::{get_module_info, module_by_name};
use crate::process::pattern::{create_unhook_bytes, find_pattern_in_bytes};
use crate::process::SafeHandle;

use anyhow::{anyhow, Result};
use std::time::Duration;

use windows::Win32::System::{Memory::PAGE_READWRITE, ProcessStatus::MODULEINFO};

pub type ZholHook = std::sync::Arc<dyn HookOps>;

/// Copies clone implementation for hooking to be used with discrete process memory hooks.
#[macro_export]
macro_rules! impl_hook_clone {
    ($type:ty) => {
        impl $type {
            fn clone_box_impl(&self) -> Box<dyn HookImpl> {
                Box::new(self.clone())
            }
        }

        impl HookImpl for $type {
            fn clone_box(&self) -> Box<dyn HookImpl> {
                self.clone_box_impl()
            }
        }
    };
}


/// Top-level structure for a process memory hook.
/// 
/// Runtime data is separated from compile-time, which is separated from implementation.
#[derive(Clone)]
pub struct Hook {
    pub handle: SafeHandle,
    pub data: std::sync::Arc<parking_lot::RwLock<HookData>>,
    pub hook_impl: Box<dyn HookImpl>,
}

pub trait CloneHookImpl {
    fn clone_hook_impl(&self) -> Box<dyn HookImpl>;
}

impl<T> CloneHookImpl for T
where
    T: HookImpl + Clone + 'static,
{
    fn clone_hook_impl(&self) -> Box<dyn HookImpl> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn HookImpl> {
    fn clone(&self) -> Self {
        self.clone_hook_impl()
    }
}

impl Hook {
    pub fn new(
        handle: SafeHandle,
        hook_impl: impl HookImpl + 'static,
    ) -> MemOpResult<std::sync::Arc<Self>> {
        // Initialize with default HookData
        let maybe_module = module_by_name(&handle, hook_impl.module_name(), true, None)?;
        let module = maybe_module.ok_or(anyhow!("Could not get module Zhol.exe."))?;
        let data = HookData {
            module_addr: module.0 as usize,
            hook_mem: allocate_memory(&handle, hook_impl.hook_alloc_size())?,
            var_mem: allocate_memory(&handle, hook_impl.var_size())?,
            pattern: hook_impl.pattern().to_vec(),
            var_size: hook_impl.var_size(),
            hook_alloc_size: hook_impl.hook_alloc_size(),
            addr: None,
            found_bytes: None,
        };

        let hook_self = Self {
            handle,
            data: std::sync::Arc::new(parking_lot::RwLock::new(data)),
            hook_impl: Box::new(hook_impl),
        };

        Ok(std::sync::Arc::new(hook_self))
    }
}

unsafe impl Send for Hook {}
unsafe impl Sync for Hook {}

/// Hook-agnostic operations so the hook can be meaningfully interacted with in top-level logic.
/// 
/// Provides common functionality like hooking, unhooking, and inner specification retreival.
pub trait HookOps: Send + Sync {
    fn handle(&self) -> SafeHandle;
    fn data(&self) -> &std::sync::Arc<parking_lot::RwLock<HookData>>;
    fn hook_impl(&self) -> &Box<dyn HookImpl>;

    // #[cfg(feature = "async")]
    // async fn async_hook(&self, timeout: Duration) -> crate::MemOpResult<()>;
    fn hook(&self, timeout: Duration) -> MemOpResult<()>;

    // #[cfg(feature = "async")]
    // async fn async_unhook(&self, timeout: Duration) -> crate::MemOpResult<()>;
    fn unhook(&self, timeout: Duration) -> MemOpResult<()>;
    // pub struct MemOpContext {
    //     pub addr: usize,
    //     pub offset: usize,
    //     pub at_pointer: bool,
    //     pub timeout: Option<Duration>,
    // }
    //
    /// Creates MemOpContext for a default memory operation originating from the base of the hook
    fn ctx(&self, offset: usize, at_pointer: bool, timeout: Option<Duration>) -> MemOpContext {
        let data = self.data().read();
        MemOpContext::new(data.var_mem.addr, offset, at_pointer, timeout)
    }
}
use crate::{memop_err, MemOpError, MemOpResult};
impl HookOps for Hook {
    fn data(&self) -> &std::sync::Arc<parking_lot::RwLock<HookData>> {
        &self.data
    }

    fn handle(&self) -> SafeHandle {
        self.handle.clone()
    }

    fn hook_impl(&self) -> &Box<dyn HookImpl> {
        &self.hook_impl
    }

    // Modified to take &self instead of &mut self
    fn hook(&self, timeout: Duration) -> MemOpResult<()> {
        let module = match module_by_name(
            &self.handle,
            &self.hook_impl.module_name(),
            true,
            Some(timeout),
        )? {
            Some(m) => m,
            None => {
                return Err(crate::memop_err!(
                    "No module named \"{}\".",
                    &self.hook_impl.module_name()
                ))
            }
        };

        let module_info: MODULEINFO = get_module_info(&self.handle, module, None)?;
        change_memory_protection(
            &self.handle,
            module.0 as usize,
            module_info.SizeOfImage as usize,
            None,
            PAGE_READWRITE,
        )?;

        let bytes = read_bytes(
            &self.handle,
            module.0 as usize,
            module_info.SizeOfImage as usize,
            None,
        )?;

        let matches = find_pattern_in_bytes(bytes, self.data.read().pattern.clone())?;

        // Use write lock to modify data
        {
            let mut data = self.data.write();
            (data.addr, data.found_bytes) = match matches.first() {
                Some((a, b)) => (Some(module.0 as usize + a.to_owned()), Some(b.to_owned())),
                None => return MemOpResult::Err(MemOpError::PatternNotFound),
            };
        }

        // Now read the data
        let data_read = self.data.read();
        let hook_bytes = self.hook_impl.build_hook(&data_read)?;
        let jump_bytes = self.hook_impl.build_jmp(&data_read)?;

        let addr = data_read.addr.ok_or(anyhow!(
            "Inject point address was not found. This should not be possible."
        ))?;

        write_bytes(
            &self.handle,
            data_read.hook_mem.addr as usize,
            &hook_bytes,
            Some(timeout),
        )?;

        write_bytes(&self.handle, addr, &jump_bytes, Some(timeout))?;

        Ok(())
    }

    fn unhook(&self, timeout: Duration) -> MemOpResult<()> {
        let data_read = self.data().read();

        let inject_addr = match data_read.addr {
            None => return Ok(()),
            Some(a) => a as usize,
        };

        match &data_read.found_bytes {
            Some(found_bytes) => {
                write_bytes(
                    &self.handle,
                    inject_addr,
                    &create_unhook_bytes(self.hook_impl.pattern(), found_bytes),
                    Some(timeout),
                )?;
            }
            None => {
                return Err(memop_err!(
                    "Unhook called without pattern scanned and match found."
                ))
            }
        }

        Ok(())
    }
}

/// The runtime data for a process memory hook.
/// 
/// Modeled after Cheat Engine to provide parity with common exploit enumeration tools.
#[derive(Clone)]
pub struct HookData {
    // pub handle: SafeHandle,
    pub module_addr: usize,
    pub hook_mem: MemoryRegion,
    pub var_mem: MemoryRegion,
    pub pattern: Vec<Byte>,
    pub var_size: usize,
    pub hook_alloc_size: usize,
    pub addr: Option<usize>,
    pub found_bytes: Option<Vec<u8>>,
}

impl HookData {
    pub fn get_addr(&self) -> Result<usize> {
        self.addr.ok_or(anyhow!(
            "get_addr() called without pattern scanned and injection point found."
        ))
    }

    pub fn get_jmp_size<T: HookImpl + ?Sized>(&self, hook_impl: &T) -> Result<usize> {
        Ok(hook_impl.build_jmp(self)?.len())
    }

    pub fn get_nth_unhook_byte(&self, index: usize) -> Result<u8> {
        let found_bytes = self.found_bytes.as_ref().ok_or(anyhow!(
            "Unhook bytes called without pattern scanning and finding a match."
        ))?;

        found_bytes
            .get(index)
            .ok_or(anyhow!("Index \"{}\" not in range unhookbytes.", index))
            .copied()
    }
}

/// Defines the complile-time behavior of a process memory hook.
pub trait HookImpl: Send + Sync + CloneHookImpl {
    fn pattern(&self) -> &'static [Byte];

    // Configurable parameters with defaults
    fn var_size(&self) -> usize {
        0x4
    }
    fn hook_alloc_size(&self) -> usize {
        0x1000
    }
    fn module_name(&self) -> &'static str {
        "Zhol.exe"
    }

    // Hook building functionality
    fn build_jmp(&self, hook_data: &HookData) -> Result<Vec<u8>> {
        let ops = newmem_jmp(hook_data)?;
        handle_x86_asm_build(ops)
    }

    // Must be implemented by concrete hooks
    fn build_hook(&self, hook_data: &HookData) -> Result<Vec<u8>>;
}
