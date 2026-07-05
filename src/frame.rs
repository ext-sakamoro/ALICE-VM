//! Function call frame.

/// A call frame representing a function invocation.
#[derive(Debug, Clone)]
pub struct CallFrame {
    /// Return address (instruction index to resume after `Ret`).
    pub return_addr: usize,
    /// Stack base pointer.
    pub base_ptr: usize,
    /// Number of local slots (includes arguments).
    pub _n_locals: usize,
}
