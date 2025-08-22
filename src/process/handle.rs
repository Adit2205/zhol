use parking_lot::{Mutex, MutexGuard};
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::Foundation::HANDLE;

/// Thread-safe wrapper for Windows API handles with timeout-based locking.
/// 
/// Provides safe concurrent access to Windows handles by wrapping them in
/// an Arc<Mutex<T>> and offering timeout-based acquisition.
pub struct SafeHandle {
    handle: Arc<Mutex<HANDLE>>,
    // This may have more fields in the future or when we go crossplatform; That's why this is a struct. -S
}

impl Clone for SafeHandle {
    fn clone(&self) -> Self {
        SafeHandle {
            handle: Arc::clone(&self.handle),
        }
    }
}

/// RAII guard that provides exclusive access to the underlying handle.
/// 
/// The handle is automatically released when this guard goes out of scope.
pub struct SafeHandleGuard<'a> {
    _guard: MutexGuard<'a, HANDLE>,
}

impl SafeHandle {
    /// Creates a new SafeHandle wrapping the given Windows handle.
    pub fn new(handle: HANDLE) -> Self {
        SafeHandle {
            handle: Arc::new(Mutex::new(handle)),
        }
    }

    /// Attempts to acquire exclusive access to the handle with an optional timeout.
    /// 
    /// # Arguments
    /// * `timeout` - Maximum time to wait for the lock. If None, blocks indefinitely.
    /// 
    /// # Returns
    /// * `Some(SafeHandleGuard)` if the lock was acquired
    /// * `None` if the timeout expired before acquiring the lock
    pub fn acquire_with_timeout(&self, timeout: Option<Duration>) -> Option<SafeHandleGuard<'_>> {
        match timeout {
            Some(duration) => self.handle.try_lock_for(duration),
            None => Some(self.handle.lock()),
        }
        .map(|guard| SafeHandleGuard { _guard: guard })
    }
}

impl<'a> std::ops::Deref for SafeHandleGuard<'a> {
    type Target = HANDLE;

    fn deref(&self) -> &Self::Target {
        &*self._guard
    }
}

/// Convenience macro for acquiring a handle and executing a block of code.
/// 
/// # Arguments
/// * `$handle` - Reference to a SafeHandle
/// * `$timeout` - Optional timeout duration
/// * `$guard` - Identifier for the guard variable in the block
/// * `$ret` - Return type of the block
/// * `$block` - Code block to execute with the acquired handle
/// 
/// # Returns
/// * `anyhow::Result<$ret>` - Success with block result or timeout error
/// 
/// # Example
/// ```rust
/// use std::time::Duration;
/// use windows::Win32::Foundation::HANDLE;
/// use anyhow::anyhow;
/// use zhol::{with_handle, process::handle::SafeHandle};
/// 
/// let safe_handle = SafeHandle::new(HANDLE(std::ptr::null_mut()));
/// 
/// let result = with_handle!(&safe_handle, Some(Duration::from_secs(1)), |guard| -> i32, {
///     // Use *guard to access the HANDLE
///     // In a real scenario, you'd call Windows API functions with *guard
///     println!("Handle value: {:?}", *guard);
///     Ok(42)
/// });
/// 
/// match result {
///     Ok(value) => println!("Operation succeeded with value: {}", value),
///     Err(e) => println!("Operation failed: {}", e),
/// }
/// ```
#[macro_export]
macro_rules! with_handle {
    ($handle:expr, $timeout:expr, |$guard:ident| -> $ret:ty, $block:expr) => {{
        let safe_handle: &SafeHandle = $handle;
        let result: anyhow::Result<$ret> = match safe_handle.acquire_with_timeout($timeout) {
            Some($guard) => $block,
            None => Err(anyhow!("Failed to acquire lock within timeout period")),
        };
        result
    }};
}