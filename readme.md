# Zhol

A Rust library for Windows process manipulation, memory operations, and function hooking.

## Features

- Process module enumeration and information retrieval
- Memory reading and writing operations
- Function hooking support with x86 assembly generation
- Pattern scanning capabilities
- Safe handle management for Windows processes

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
zhol = { git = "https://github.com/yourusername/zhol" }
```

### Examples

#### Working with Process Modules

```rust
use zhol::process::{SafeHandle, module};
use std::time::Duration;

// Find a module in a process
let process_handle = get_process_handle(process_id)?;
let timeout = Some(Duration::from_secs(1));

// Get all modules
let modules = module::get_named_modules(&process_handle, timeout)?;
for (name, handle, info) in modules {
    println!("Module: {}, Base: {:?}, Size: {}", name, info.lpBaseOfDll, info.SizeOfImage);
}

// Find specific module
if let Some(kernel32) = module::module_by_name(&process_handle, "kernel32.dll", true, None)? {
    println!("Found kernel32.dll!");
}
```

#### Memory Operations

```rust
use zhol::memory::{read, write};

// Read value from process memory
let value: u32 = read::read_value(&process_handle, address)?;

// Write value to process memory
write::write_value(&process_handle, address, &value)?;
```

## Contributing

Contributions are welcome. Please reach out if you have any questions.