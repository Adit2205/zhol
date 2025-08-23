use crate::hooks::ZholHook;
use crate::memory::MemOpContext;

/// Top-level trait for determining if a type can be used directly from game memory.
///
/// # Info
/// This trait is a supertrait of Transmutable and bytemuck's Pod.
///
/// This is done because Transmutable contains custom to/from byte ops and bytemuck Pod contains the memory-relevant traits.
///
/// With this design, a type that is merely a pointer can be read in the same way as a regular Transmutable/Pod type.
pub trait ZholTyped<T>: Transmutable<T> + bytemuck::Pod {}

/// Trait for custom "to/from bytes" methods.
pub trait Transmutable<T> {
    fn transmute_from(
        bytes: &Vec<u8>,
        _hook: &ZholHook,
        _context: &MemOpContext,
    ) -> anyhow::Result<Option<T>>;

    fn byte_repr(&self, _hook: &ZholHook, _context: &MemOpContext) -> anyhow::Result<Vec<u8>>;
}

impl<T: Transmutable<T> + bytemuck::Pod> ZholTyped<T> for T {}

/// Marker trait for traits that should use the default impl of Transmutable.
pub trait AutoImplTransmutable {}
impl AutoImplTransmutable for i32 {}
impl AutoImplTransmutable for i64 {}
impl AutoImplTransmutable for f32 {}
impl AutoImplTransmutable for f64 {}
impl AutoImplTransmutable for u32 {}
impl AutoImplTransmutable for u64 {}

impl<T: bytemuck::Pod + AutoImplTransmutable> Transmutable<T> for T {
    fn transmute_from(
        bytes: &Vec<u8>,
        _hook: &ZholHook,
        _context: &MemOpContext,
    ) -> anyhow::Result<Option<T>> {
        let value = bytemuck::try_pod_read_unaligned::<T>(&bytes)
            .map_err(|e| anyhow::anyhow!("Failed to convert bytes to type: {}", e))?;
        Ok(Some(value))
    }

    fn byte_repr(
        &self,
        _hook_opt: &ZholHook,
        _context: &MemOpContext,
    ) -> anyhow::Result<Vec<u8>> {
        Ok(bytemuck::bytes_of::<T>(self).to_vec())
    }
}
