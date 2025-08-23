use crate::await_memop;
use crate::hooks::async_ext::to_hook_ops;
use crate::process::SafeHandle;
use crate::MemOpResult;

#[cfg(feature = "async")]
/// Runs zhol::memory::read::read_bytes() on the local thread pool to provide an async version.
pub async fn read_bytes(
    handle: &SafeHandle,
    addr: usize,
    size: usize,
    timeout: Option<std::time::Duration>,
) -> MemOpResult<Vec<u8>> {
    await_memop!(handle, |h| -> MemOpResult<Vec<u8>> {
        crate::memory::read::read_bytes(&h, addr, size, timeout)
    })
}

#[cfg(feature = "async")]
/// Runs zhol::memory::read::read_value::<T>() on the local thread pool to provide an async version.
pub async fn read_value<T: crate::memory::transmute::ZholTyped<T> + Send + Sync>(
    hook: &crate::hooks::async_ext::AsyncZholHook,
    address: usize,
    timeout: Option<std::time::Duration>,
) -> MemOpResult<T> {
    await_memop!(to_hook_ops(hook), |h| -> MemOpResult<T> {
        crate::memory::read::read_value::<T>(&h, address, timeout)
    })
}