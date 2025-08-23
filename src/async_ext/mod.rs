#[cfg(feature = "async")]
#[macro_export]
/// Clones a SafeHandle and passes it into a smol::unblock closure, awaited.
/// Effectively, this lets you run synchronous operations that use WinAPI handles asynchronously.
macro_rules! await_memop {
    (
        $handle:expr,
        $body:expr
    ) => {{
        let handle_clone = $handle.clone();

        smol::unblock(move || -> MemOpResult<_> { $body(handle_clone) }).await
    }};
}

#[cfg(feature = "async")]
#[macro_export]
/// Same as await_memop!() but does not handle the JoinError or await the future created by the sync closure.
macro_rules! async_memop {
    (
        $handle:expr,
        $body:expr
    ) => {{
        let handle_clone = $handle.clone();
        smol::unblock(move || -> MemOpResult<_> { $body(handle_clone) })
    }};
}