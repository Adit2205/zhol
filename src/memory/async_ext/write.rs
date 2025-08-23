use crate::hooks::async_ext::AsyncZholHook;
use crate::{await_memop, process::SafeHandle, MemOpResult};
use std::time::Duration;

#[cfg(feature = "async")]
pub async fn write_bytes(
    handle: &SafeHandle,
    addr: usize,
    bytes: &Vec<u8>,
    timeout: Option<Duration>,
) -> MemOpResult<()> {
    // Could use lifetimes here but writing isn't really an operation we need optimized for memory or ops; The winapi call will always be slowest
    // As the value we're writing will be most likely dropped soon after anyways
    // Lifetimes would also be a tiny savings for a bunch of work here - high S
    let bytes_to_write = bytes.clone();
    await_memop!(handle, |h| -> MemOpResult<()> {
        crate::memory::write::write_bytes(&h, addr, &bytes_to_write, timeout)
    })
}

#[cfg(feature = "async")]
pub async fn write_value<T: crate::memory::transmute::ZholTyped<T> + Send + Sync>(
    hook: &AsyncZholHook,
    address: usize,
    value: T,
    timeout: Option<Duration>,
) -> MemOpResult<()> {
    // use crate::memory::MemOpContext;

    use crate::hooks::async_ext::to_hook_ops;
    await_memop!(to_hook_ops(hook), |h| -> MemOpResult<()> {
        crate::memory::write::write_value(&h, address, value, timeout)
    })
}
