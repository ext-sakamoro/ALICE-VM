//! VM error types.

use std::fmt;

/// Errors that can occur during VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    StackOverflow,
    StackUnderflow,
    DivisionByZero,
    InvalidRegister(u8),
    InvalidJump(usize),
    CallStackOverflow,
    CallStackUnderflow,
    InvalidLocal(usize),
    HeapOutOfMemory,
    HeapInvalidAddress(usize),
    HeapDoubleFree(usize),
    ProgramCounterOutOfBounds,
    InvalidHeapSize,
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StackOverflow => write!(f, "stack overflow"),
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::InvalidRegister(r) => write!(f, "invalid register: {r}"),
            Self::InvalidJump(addr) => write!(f, "invalid jump target: {addr}"),
            Self::CallStackOverflow => write!(f, "call stack overflow"),
            Self::CallStackUnderflow => write!(f, "call stack underflow"),
            Self::InvalidLocal(off) => write!(f, "invalid local offset: {off}"),
            Self::HeapOutOfMemory => write!(f, "heap out of memory"),
            Self::HeapInvalidAddress(a) => write!(f, "invalid heap address: {a}"),
            Self::HeapDoubleFree(a) => write!(f, "double free at address: {a}"),
            Self::ProgramCounterOutOfBounds => write!(f, "program counter out of bounds"),
            Self::InvalidHeapSize => write!(f, "invalid heap allocation size"),
        }
    }
}

impl std::error::Error for VmError {}
