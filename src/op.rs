//! Bytecode instruction set.

/// Bytecode instructions for the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    // -- Stack operations --
    Push(i64),
    Pop,
    Dup,
    Swap,
    /// Duplicate the value at offset `n` from the top (0 = top).
    Over(usize),

    // -- Arithmetic --
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Abs,

    // -- Bitwise --
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,

    // -- Comparison (push 1 for true, 0 for false) --
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // -- Control flow --
    Jump(usize),
    JumpIf(usize),
    JumpIfZero(usize),

    // -- Register operations --
    LoadReg(u8),
    StoreReg(u8),

    // -- Local variables (relative to frame base) --
    LoadLocal(usize),
    StoreLocal(usize),

    // -- Function calls --
    /// `Call(addr, n_args)`.
    Call(usize, usize),
    Ret,

    // -- Heap memory --
    HeapAlloc,
    HeapFree,
    HeapLoad,
    HeapStore,
    HeapLoadOffset,
    HeapStoreOffset,

    // -- Misc --
    Nop,
    Halt,
    DebugPrint,
    Inc,
    Dec,
}
