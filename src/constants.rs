//! VM configuration constants.

/// Number of general-purpose registers.
pub const NUM_REGISTERS: usize = 16;

/// Maximum stack depth.
pub const MAX_STACK: usize = 1024;

/// Maximum call depth.
pub const MAX_CALL_DEPTH: usize = 256;

/// Maximum heap size in 64-bit words.
pub const MAX_HEAP: usize = 4096;
