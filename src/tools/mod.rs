/// Top-level function for performing memory AOB scans.
///
/// # Arguments
/// * `handle`: WinAPI handle (*mut c_void) with PROCESS_VM_READ
/// * `pattern`: IDA byte pattern
/// * `origin`: Address to begin searching at
/// * `size`: Size (in bytes) of search area
/// # Returns
/// * `anyhow::Result<Vec<(usize, Vec<u8>)>>`: Anyhow result of a vector of addresses where a match was found, including bytes found at matches
pub fn pattern_scan(
    handle: &crate::process::SafeHandle,
    pattern: &str,
    origin: usize,
    size: usize,
) -> anyhow::Result<Vec<(usize, Vec<u8>)>> {
    use crate::process::pattern::{find_pattern_in_bytes, prepare_pattern};


    let bytes = crate::memory::read::read_bytes(handle, origin, size - 0x04, None)?;
    let pattern_bytes = prepare_pattern(pattern);
    find_pattern_in_bytes(bytes, pattern_bytes)
}