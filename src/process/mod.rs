// pub mod handle;
// pub mod input;
// pub mod module;
// pub mod pattern;
// pub mod utils;

/// A macro for safely acquiring and using a handle with timeout support.
/// 
/// This macro provides a convenient way to acquire a handle with an optional timeout,
/// execute code with the acquired handle guard, and properly handle timeout errors.
/// 
/// # Arguments
/// 
/// * `$handle` - A reference to a `SafeHandle` instance
/// * `$timeout` - An `Option<Duration>` specifying the timeout for acquiring the handle
/// * `$guard` - The identifier for the handle guard variable in the code block
/// * `$ret` - The return type of the code block
/// * `$block` - The code block to execute with the acquired handle
/// 
/// # Returns
/// 
/// Returns a `MemOpResult<$ret>` where success contains the result of the code block,
/// or an error if the timeout is reached or other operation fails.
/// 
/// # Examples
/// 
/// ```rust,norun
/// let handle = SafeHandle::new(some_windows_handle);
/// let result = with_handle!(&handle, Some(Duration::from_secs(5)), |guard| -> windows::Win32::Foundation::HANDLE, {
///     // Use the handle through guard
///     Ok(*guard)
/// });
/// ```
#[macro_export]
macro_rules! with_handle {
    ($handle:expr, $timeout:expr, |$guard:ident| -> $ret:ty, $block:expr) => {{
        let safe_handle: &$crate::process::SafeHandle = $handle;
        let result: crate::MemOpResult<$ret> = match safe_handle.acquire_with_timeout($timeout) {
            Some($guard) => $block,
            None => Err(crate::MemOpError::TimeoutReached(($timeout, None))),
        };
        result
    }};
}

use std::sync::Arc;
use windows::Win32::Foundation::HANDLE;

/// A wrapper around Windows API handles that provides thread-safe storage and conversion.
/// 
/// `RawHandle` stores a Windows `HANDLE` as a `usize` value, allowing it to be safely
/// shared between threads while maintaining the ability to convert back to the original
/// `HANDLE` type when needed for Windows API calls.
/// 
/// This type implements `Send` and `Sync` to allow cross-thread usage, though care
/// must be taken to ensure the underlying handle remains valid across thread boundaries.
pub struct RawHandle {
    /// The handle value stored as a usize for thread-safe storage
    handle_value: usize,
}

impl RawHandle {
    /// Creates a new `RawHandle` from a Windows API handle.
    /// 
    /// **This is only intended for library use but has been made available to suit more custom solutions.**
    pub fn new(handle: windows::Win32::Foundation::HANDLE) -> Self {
        // Convert HANDLE to its integer representation
        let handle_value = handle.0 as usize;
        RawHandle { handle_value }
    }

    /// Converts the stored handle value back to a Windows API `HANDLE`.
    /// 
    /// This is only intended for library use but has been made available to suit more custom solutions.
    pub fn as_handle(&self) -> windows::Win32::Foundation::HANDLE {
        windows::Win32::Foundation::HANDLE(self.handle_value as *mut std::ffi::c_void)
    }
}

/// # Safety
/// 
/// `RawHandle` can be safely sent between threads as it only stores the handle value
/// as an integer. However, the caller must ensure the underlying Windows handle
/// remains valid across thread boundaries.
unsafe impl Send for RawHandle {}

/// # Safety
/// 
/// `RawHandle` can be safely shared between threads as it only provides read access
/// to the stored handle value. The underlying handle value is immutable after creation.
unsafe impl Sync for RawHandle {}

impl std::ops::Deref for RawHandle { // BAD, rework sometime :) -S
    type Target = windows::Win32::Foundation::HANDLE;

    /// Provides deref access to the underlying handle using thread-local storage.
    /// 
    /// This implementation uses thread-local storage to provide a reference to the handle
    /// without creating lifetime issues. Each thread gets its own storage for the handle value.
    /// 
    /// # Safety
    /// 
    /// This implementation uses unsafe code to cast the thread-local storage address.
    /// The safety relies on the thread-local storage remaining valid for the duration
    /// of the reference's lifetime within the same thread.
    fn deref(&self) -> &Self::Target {
        // We can't return a reference to a temporary handle
        thread_local! {
            static HANDLE_STORAGE: std::cell::Cell<windows::Win32::Foundation::HANDLE> =
                std::cell::Cell::new(windows::Win32::Foundation::HANDLE::default());
        }

        HANDLE_STORAGE.with(|storage| {
            storage.set(self.as_handle());
            unsafe { &*(storage as *const _ as *const windows::Win32::Foundation::HANDLE) }
        })
    }
}

use parking_lot::{Mutex, MutexGuard};
use std::time::Duration;

/// A thread-safe wrapper for Windows handles with timeout-based locking.
/// 
/// `SafeHandle` provides synchronized access to a Windows handle across multiple threads
/// using a mutex. It supports both blocking and timeout-based acquisition of the handle,
/// making it suitable for scenarios where handle access needs to be coordinated between
/// multiple threads or where deadlock prevention is important.
/// 
/// The handle is wrapped in an `Arc<RawHandle>` to allow for efficient cloning and
/// shared ownership while maintaining thread safety.
pub struct SafeHandle {
    /// The mutex-protected handle wrapped in an Arc for shared ownership
    inner: Arc<Mutex<Arc<RawHandle>>>,
}

impl Clone for SafeHandle {
    /// Creates a new `SafeHandle` that shares the same underlying handle.
    /// 
    /// Cloning a `SafeHandle` creates a new reference to the same underlying
    /// mutex-protected handle. All clones will synchronize access to the same handle.
    fn clone(&self) -> Self {
        SafeHandle {
            inner: Arc::clone(&self.inner),
        }
    }
}


// We are taking special care to ensure these are actually compatible with Send + Sync, tokio is just an overly restrictive mess :) -S
unsafe impl Send for SafeHandle {}
unsafe impl Sync for SafeHandle {}

/// A RAII guard that provides exclusive access to a Windows handle.
/// 
/// `SafeHandleGuard` is returned by `SafeHandle::acquire_with_timeout()` and ensures
/// that the handle remains locked for the duration of the guard's lifetime. The handle
/// is automatically released when the guard is dropped.
/// 
/// The guard implements `Deref` to provide direct access to the underlying `HANDLE`.
pub struct SafeHandleGuard<'a> {
    /// The mutex guard that maintains exclusive access to the handle
    _guard: MutexGuard<'a, Arc<RawHandle>>,
}

impl SafeHandle {
    /// Creates a new `SafeHandle` from a Windows API handle.
    /// 
    /// # Arguments
    /// 
    /// * `handle` - The Windows `HANDLE` to wrap in a thread-safe container
    /// 
    /// # Examples
    /// 
    /// ```rust,norun
    /// use zhol::process::SafeHandle;
    /// 
    /// let safe_handle = SafeHandle::new(some_windows_handle);
    /// ```
    pub fn new(handle: HANDLE) -> Self {
        let raw_handle = Arc::new(RawHandle::new(handle));
        SafeHandle {
            inner: Arc::new(Mutex::new(raw_handle)),
        }
    }

    /// Attempts to acquire exclusive access to the handle with an optional timeout.
    /// 
    /// # Arguments
    /// 
    /// * `timeout` - Optional timeout duration. If `Some(duration)`, the method will
    ///   wait up to that duration for the handle to become available. If `None`,
    ///   the method will block indefinitely until the handle is available.
    /// 
    /// # Returns
    /// 
    /// Returns `Some(SafeHandleGuard)` if the handle was successfully acquired,
    /// or `None` if the timeout was reached (when a timeout was specified).
    /// 
    /// # Examples
    /// 
    /// ```rust,norun
    /// // Try to acquire with a 5-second timeout
    /// if let Some(guard) = handle.acquire_with_timeout(Some(Duration::from_secs(5))) {
    ///     // Use the handle through the guard
    ///     // Handle is automatically released when guard is dropped
    /// }
    /// 
    /// // Acquire without timeout (blocks until available)
    /// let guard = handle.acquire_with_timeout(None).unwrap();
    /// ```
    pub fn acquire_with_timeout(&self, timeout: Option<Duration>) -> Option<SafeHandleGuard<'_>> {
        match timeout {
            Some(duration) => self.inner.try_lock_for(duration),
            None => Some(self.inner.lock()),
        }
        .map(|guard| SafeHandleGuard { _guard: guard })
    }
}

impl<'a> std::ops::Deref for SafeHandleGuard<'a> {
    type Target = HANDLE;

    /// Provides direct access to the underlying Windows handle.
    /// 
    /// This allows the guard to be used as if it were the handle itself,
    /// enabling transparent usage in Windows API calls while maintaining
    /// the safety guarantees of the mutex protection.
    fn deref(&self) -> &Self::Target {
        &**self._guard
    }
}