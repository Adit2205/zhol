use std::{
    char::ParseCharError,
    fmt,
    num::{ParseFloatError, ParseIntError},
    str::ParseBoolError,
    time::Duration,
};

use windows::Win32::System::{
    // Diagnostics::Debug,
    Memory::{PAGE_PROTECTION_FLAGS, PAGE_TYPE, VIRTUAL_ALLOCATION_TYPE},
};
use windows_result::HRESULT;

pub const INVALID_ALLOCATION_TYPE: u8 = 0b001;
pub const INVALID_PROTECTION_FLAGS: u8 = 0b010;
pub const INVALID_PAGE_TYPE: u8 = 0b100;

/// Represents errors that can occur during a given memory operation
#[derive(Debug)]
pub enum MemOpError {
    /// Operation timed out
    TimeoutReached((Option<Duration>, Option<anyhow::Error>)),
    /// Memory is in an invalid state for the requested operation
    MemoryStateInvalid(
        (
            VIRTUAL_ALLOCATION_TYPE,
            PAGE_PROTECTION_FLAGS,
            PAGE_TYPE,
            u8, // Bitflag telling which mem states were wrong
            Option<anyhow::Error>,
        ),
    ),
    PatternNotFound,
    /// WinAPI errors
    WinAPI((windows_result::Error, Option<anyhow::Error>)),
    /// Generic error that wraps an anyhow::Error
    Other(anyhow::Error),
}

impl MemOpError {
    /// Creates a new `MemOpError::Other` from anything that implements `std::error::Error`
    pub fn new<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        MemOpError::Other(anyhow::Error::new(error))
    }

    /// Convert a boxed error into a MemOpError
    pub fn from_boxed(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        let anyhow_err = anyhow::anyhow!("{}", err);
        MemOpError::Other(anyhow_err)
    }

    /// Returns the inner anyhow::Error if this is an `Other` variant
    pub fn into_inner(self) -> Option<anyhow::Error> {
        match self {
            MemOpError::Other(err) => Some(err),
            _ => None,
        }
    }

    /// Returns true if this is a `TimeoutReached` error
    pub fn is_timeout(&self) -> bool {
        matches!(self, MemOpError::TimeoutReached(_))
    }

    /// Returns true if this is a `MemoryStateInvalid` error
    pub fn is_memory_state_invalid(&self) -> bool {
        matches!(self, MemOpError::MemoryStateInvalid(_))
    }

    /// Returns true if this is a `WinAPI` error
    pub fn is_winapi(&self) -> bool {
        matches!(self, MemOpError::WinAPI(_))
    }

    /// Converts this error to its underlying root cause string
    pub fn root_cause_string(&self) -> String {
        match self {
            MemOpError::TimeoutReached((timeout, err)) => {
                // let mut root_cause = String::new();
                match (timeout, err) {
                    (Some(t), Some(e)) => {
                        format!("Timeout operation of context \"{e}\" failed to complete within timeout \"{:#?}\".", t)
                    }
                    (Some(t), None) => {
                        format!(
                            "Timeout operation failed to complete within timeout \"{:#?}\".",
                            t
                        )
                    }
                    (None, None) => {
                        "Timeout operation failed to complete within timeout.".to_string()
                    }
                    (None, Some(e)) => {
                        format!("Timeout operation of context \"{e}\" failed to complete with its timeout.")
                    }
                }
            }
            MemOpError::MemoryStateInvalid((state_flag, prot_flag, type_flag, mem_flag, err)) => {
                let mut bad_attrs: Vec<Box<dyn fmt::Debug>> = Vec::new();

                if mem_flag & INVALID_ALLOCATION_TYPE != 0 {
                    bad_attrs.push(Box::new(state_flag));
                }

                if mem_flag & INVALID_PROTECTION_FLAGS != 0 {
                    bad_attrs.push(Box::new(prot_flag));
                }

                if mem_flag & INVALID_PAGE_TYPE != 0 {
                    bad_attrs.push(Box::new(type_flag));
                }

                let mut attr_err = String::new();
                for (i, attr) in bad_attrs.iter().enumerate() {
                    attr_err.push_str(&format!("{:#?}", attr));
                    if i != (bad_attrs.len() - 1) && bad_attrs.len() != 0 {
                        attr_err.push_str(", ");
                    }
                }

                match err {
                    Some(e) => format!("Memory operation of context \"{e}\" failed with following invalid states: \"{attr_err}\""),
                    None => format!("Memory operation failed with the following invalid states: \"{attr_err}\"")
                }
            }
            MemOpError::WinAPI((api_err, err)) => {
                let api_res: HRESULT = api_err.code();
                let code = api_res.0;
                // let code: i32 = 1;
                match err {
                    Some(e) => format!("Windows API call with context \"{e}\" failed with: \"Windows Error: {:08X} - {}\"", code, api_err),
                    None => format!("Windows API call failed with: \"Windows Error: {:08X} - {}\"", code, api_err)
                }
            }
            MemOpError::PatternNotFound => format!("Pattern not found"),
            MemOpError::Other(err) => format!("{:#}", err),
        }
    }
}

impl fmt::Display for MemOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemOpError: \"{}\"", &self.root_cause_string())
    }
}

impl std::error::Error for MemOpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MemOpError::Other(err) => err.source(),
            _ => None,
        }
    }
}

// From implementations for better interoperability with anyhow
impl From<anyhow::Error> for MemOpError {
    fn from(err: anyhow::Error) -> Self {
        MemOpError::Other(err)
    }
}

// Implement From for common standard library errors
impl From<std::io::Error> for MemOpError {
    fn from(err: std::io::Error) -> Self {
        MemOpError::Other(anyhow::Error::new(err))
    }
}

impl From<std::fmt::Error> for MemOpError {
    fn from(err: std::fmt::Error) -> Self {
        MemOpError::Other(anyhow::Error::new(err))
    }
}

impl From<std::str::Utf8Error> for MemOpError {
    fn from(err: std::str::Utf8Error) -> Self {
        MemOpError::Other(anyhow::Error::new(err))
    }
}

impl From<std::string::FromUtf8Error> for MemOpError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        MemOpError::Other(anyhow::Error::new(err))
    }
}

// Create a Result type alias with methods similar to anyhow::Result
pub type MemOpResult<T> = Result<T, MemOpError>;

/// Extension trait to add anyhow-like methods to MemOpResult
pub trait MemOpResultExt<T> {
    /// Add context to an error
    fn context<C>(self, context: C) -> MemOpResult<T>
    where
        C: fmt::Display + Send + Sync + 'static;

    /// Add context to an error with a lazy closure
    fn with_context<C, F>(self, f: F) -> MemOpResult<T>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;

    /// Similar to unwrap but maps to a custom error
    fn ok_or_else<E, F>(self, err: F) -> MemOpResult<T>
    where
        E: std::error::Error + Send + Sync + 'static,
        F: FnOnce() -> E;

    /// Chain an operation that returns a Result after this one
    fn and_then<U, F>(self, op: F) -> MemOpResult<U>
    where
        F: FnOnce(T) -> MemOpResult<U>;

    /// Convert an Option to a MemOpResult with custom error
    fn ok_or<E>(self, err: E) -> MemOpResult<T>
    where
        Self: Sized,
        E: Into<MemOpError>;

    /// Map the success value of a result
    fn map_ok<U, F>(self, op: F) -> MemOpResult<U>
    where
        F: FnOnce(T) -> U;
}

impl<T> MemOpResultExt<T> for MemOpResult<T> {
    fn context<C>(self, context: C) -> MemOpResult<T>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|err| {
            if let MemOpError::Other(inner) = err {
                MemOpError::Other(inner.context(context))
            } else {
                let new_err = anyhow::anyhow!("{}: {}", context, err);
                MemOpError::Other(new_err)
            }
        })
    }

    fn with_context<C, F>(self, f: F) -> MemOpResult<T>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|err| {
            if let MemOpError::Other(inner) = err {
                MemOpError::Other(inner.context(f()))
            } else {
                let new_err = anyhow::anyhow!("{}: {}", f(), err);
                MemOpError::Other(new_err)
            }
        })
    }

    fn ok_or_else<E, F>(self, err: F) -> MemOpResult<T>
    where
        E: std::error::Error + Send + Sync + 'static,
        F: FnOnce() -> E,
    {
        match self {
            Ok(value) => Ok(value),
            Err(_) => Err(MemOpError::new(err())),
        }
    }

    fn and_then<U, F>(self, op: F) -> MemOpResult<U>
    where
        F: FnOnce(T) -> MemOpResult<U>,
    {
        match self {
            Ok(value) => op(value),
            Err(err) => Err(err),
        }
    }

    fn ok_or<E>(self, err: E) -> MemOpResult<T>
    where
        Self: Sized,
        E: Into<MemOpError>,
    {
        match self {
            Ok(value) => Ok(value),
            Err(_) => Err(err.into()),
        }
    }

    fn map_ok<U, F>(self, op: F) -> MemOpResult<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Ok(value) => Ok(op(value)),
            Err(err) => Err(err),
        }
    }
}

// Allow macro-based error creation, similar to anyhow::anyhow!
#[macro_export]
macro_rules! memop_err {
    ($msg:literal $(,)?) => {
        $crate::MemOpError::Other(anyhow::anyhow!($msg))
    };
    ($err:expr $(,)?) => {
        $crate::MemOpError::Other(anyhow::anyhow!($err))
    };
    ($fmt:literal, $($arg:tt)*) => {
        $crate::MemOpError::Other(anyhow::anyhow!($fmt, $($arg)*))
    };
}

// Extension trait implementation for Option<T>
impl<T> MemOpResultExt<T> for Option<T> {
    fn context<C>(self, context: C) -> MemOpResult<T>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        self.ok_or_else(|| MemOpError::Other(anyhow::anyhow!("{}", context)))
    }

    fn with_context<C, F>(self, f: F) -> MemOpResult<T>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.ok_or_else(|| MemOpError::Other(anyhow::anyhow!("{}", f())))
    }

    fn ok_or_else<E, F>(self, err: F) -> MemOpResult<T>
    where
        E: std::error::Error + Send + Sync + 'static,
        F: FnOnce() -> E,
    {
        match self {
            Some(value) => Ok(value),
            None => Err(MemOpError::new(err())),
        }
    }

    fn and_then<U, F>(self, op: F) -> MemOpResult<U>
    where
        F: FnOnce(T) -> MemOpResult<U>,
    {
        match self {
            Some(value) => op(value),
            None => Err(memop_err!("Option is None")),
        }
    }

    fn ok_or<E>(self, err: E) -> MemOpResult<T>
    where
        Self: Sized,
        E: Into<MemOpError>,
    {
        match self {
            Some(value) => Ok(value),
            None => Err(err.into()),
        }
    }

    fn map_ok<U, F>(self, op: F) -> MemOpResult<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Some(value) => Ok(op(value)),
            None => Err(memop_err!("Option is None")),
        }
    }
}

// Extension trait to add conversion methods for standard Results
pub trait IntoMemOpResult<T, E> {
    /// Convert a standard Result into a MemOpResult
    fn into_memop_result(self, ctx: Option<anyhow::Error>) -> MemOpResult<T>;
}

// Implementation for standard Result where the error implements std::error::Error
impl<T, E> IntoMemOpResult<T, E> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn into_memop_result(self, _ctx: Option<anyhow::Error>) -> MemOpResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => Err(MemOpError::new(err)),
        }
    }
}
// Extension trait for anyhow::Result to convert to MemOpResult
pub trait FromAnyhow<T> {
    /// Convert an anyhow::Result into a MemOpResult
    fn into_memop_result(self) -> MemOpResult<T>;
}

// Implementation for anyhow::Result
impl<T> FromAnyhow<T> for anyhow::Result<T> {
    fn into_memop_result(self) -> MemOpResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => Err(MemOpError::Other(err)),
        }
    }
}

// First, implement From for the error type
impl From<windows_result::Error> for MemOpError {
    fn from(err: windows_result::Error) -> Self {
        MemOpError::WinAPI((err, None))
    }
}

// Create a specialized trait for windows_result::Result
pub trait FromWindowsResult<T> {
    fn into_memop_result(self, ctx: Option<MemOpError>) -> MemOpResult<T>;
}

// Implement the trait for windows_result::Result
impl<T> FromWindowsResult<T> for windows_result::Result<T> {
    fn into_memop_result(self, ctx: Option<MemOpError>) -> MemOpResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => match ctx {
                Some(c) => Err(c),
                None => Err(MemOpError::new(err)),
            },
        }
    }
}

/// Trait for errors that we don't care about matching down further based on context. This will use Memop::Other().
pub trait GenericError: Send + Sync + Clone + PartialEq + Eq + std::fmt::Debug {}
impl GenericError for ParseIntError {}
impl GenericError for ParseBoolError {}
impl GenericError for ParseCharError {}
impl GenericError for ParseFloatError {}

impl<T: GenericError> From<T> for MemOpError {
    fn from(err: T) -> Self {
        memop_err!("{:#?}", err)
    }
}

// #[cfg(feature = "async")]
// impl GenericError for tokio::task::JoinError {}
