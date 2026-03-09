#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

//! ALICE-VM: Bytecode virtual machine with stack machine, register machine,
//! instruction set, memory management, and call frames.

use std::fmt;

/// Number of general-purpose registers.
const NUM_REGISTERS: usize = 16;

/// Maximum stack depth.
const MAX_STACK: usize = 1024;

/// Maximum call depth.
const MAX_CALL_DEPTH: usize = 256;

/// Maximum heap size in 64-bit words.
const MAX_HEAP: usize = 4096;

// ---------------------------------------------------------------------------
// Instruction set
// ---------------------------------------------------------------------------

/// Bytecode instructions for the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    // -- Stack operations --
    /// Push an immediate i64 value onto the stack.
    Push(i64),
    /// Pop the top value and discard it.
    Pop,
    /// Duplicate the top value.
    Dup,
    /// Swap the top two values.
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
    /// Jump to absolute instruction index.
    Jump(usize),
    /// Pop top; jump if non-zero.
    JumpIf(usize),
    /// Pop top; jump if zero.
    JumpIfZero(usize),

    // -- Register operations --
    /// Load register `r` and push its value.
    LoadReg(u8),
    /// Pop top and store into register `r`.
    StoreReg(u8),

    // -- Local variables (relative to frame base) --
    /// Push the value of local variable at `offset`.
    LoadLocal(usize),
    /// Pop top and store into local variable at `offset`.
    StoreLocal(usize),

    // -- Function calls --
    /// Call function at instruction index with `n` arguments already on stack.
    /// `Call(addr, n_args)`.
    Call(usize, usize),
    /// Return from function; top of stack is the return value.
    Ret,

    // -- Heap memory --
    /// Pop `size` from stack, allocate that many words, push base address.
    HeapAlloc,
    /// Pop `addr` from stack, free the allocation.
    HeapFree,
    /// Pop `addr`, push the value at that heap address.
    HeapLoad,
    /// Pop `value` then `addr`, store value at that heap address.
    HeapStore,
    /// Pop `addr` and `offset`, push value at `heap[addr + offset]`.
    HeapLoadOffset,
    /// Pop `value`, `offset`, `addr`; store value at `heap[addr + offset]`.
    HeapStoreOffset,

    // -- Misc --
    /// No operation.
    Nop,
    /// Halt execution.
    Halt,
    /// Print the top of stack (for debugging).
    DebugPrint,
    /// Increment the value on top of the stack.
    Inc,
    /// Decrement the value on top of the stack.
    Dec,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Call frame
// ---------------------------------------------------------------------------

/// A call frame representing a function invocation.
#[derive(Debug, Clone)]
struct CallFrame {
    /// Return address (instruction index to resume after Ret).
    return_addr: usize,
    /// Stack base pointer — index into the stack where this frame's locals begin.
    base_ptr: usize,
    /// Number of local slots (includes arguments).
    _n_locals: usize,
}

// ---------------------------------------------------------------------------
// Heap
// ---------------------------------------------------------------------------

/// Simple heap with bump allocation and free-list.
#[derive(Debug, Clone)]
struct Heap {
    memory: Vec<i64>,
    /// Free list: `(start, size)` pairs.
    free_list: Vec<(usize, usize)>,
    /// Active allocations: address -> size.
    allocations: Vec<(usize, usize)>,
}

impl Heap {
    fn new() -> Self {
        Self {
            memory: vec![0; MAX_HEAP],
            free_list: vec![(0, MAX_HEAP)],
            allocations: Vec::new(),
        }
    }

    fn alloc(&mut self, size: usize) -> Result<usize, VmError> {
        if size == 0 {
            return Err(VmError::InvalidHeapSize);
        }
        // First-fit allocation.
        for i in 0..self.free_list.len() {
            let (start, free_size) = self.free_list[i];
            if free_size >= size {
                let addr = start;
                if free_size == size {
                    self.free_list.remove(i);
                } else {
                    self.free_list[i] = (start + size, free_size - size);
                }
                self.allocations.push((addr, size));
                return Ok(addr);
            }
        }
        Err(VmError::HeapOutOfMemory)
    }

    fn free(&mut self, addr: usize) -> Result<(), VmError> {
        let pos = self
            .allocations
            .iter()
            .position(|&(a, _)| a == addr)
            .ok_or(VmError::HeapDoubleFree(addr))?;
        let (start, size) = self.allocations.remove(pos);
        // Insert back into free list and merge adjacent blocks.
        self.free_list.push((start, size));
        self.free_list.sort_unstable_by_key(|&(s, _)| s);
        self.merge_free_list();
        Ok(())
    }

    fn merge_free_list(&mut self) {
        let mut i = 0;
        while i + 1 < self.free_list.len() {
            let (s1, sz1) = self.free_list[i];
            let (s2, sz2) = self.free_list[i + 1];
            if s1 + sz1 == s2 {
                self.free_list[i] = (s1, sz1 + sz2);
                self.free_list.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    fn load(&self, addr: usize) -> Result<i64, VmError> {
        self.memory
            .get(addr)
            .copied()
            .ok_or(VmError::HeapInvalidAddress(addr))
    }

    fn store(&mut self, addr: usize, value: i64) -> Result<(), VmError> {
        let slot = self
            .memory
            .get_mut(addr)
            .ok_or(VmError::HeapInvalidAddress(addr))?;
        *slot = value;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Virtual machine
// ---------------------------------------------------------------------------

/// The ALICE bytecode virtual machine.
#[derive(Debug)]
pub struct Vm {
    /// Instruction program.
    program: Vec<Op>,
    /// Program counter.
    pc: usize,
    /// Operand stack.
    stack: Vec<i64>,
    /// General-purpose registers.
    registers: [i64; NUM_REGISTERS],
    /// Call stack.
    call_stack: Vec<CallFrame>,
    /// Heap memory.
    heap: Heap,
    /// Debug output collected from `DebugPrint`.
    pub debug_output: Vec<i64>,
    /// Whether the VM has halted.
    halted: bool,
}

impl Vm {
    /// Create a new VM loaded with the given program.
    #[must_use]
    pub fn new(program: Vec<Op>) -> Self {
        Self {
            program,
            pc: 0,
            stack: Vec::with_capacity(64),
            registers: [0; NUM_REGISTERS],
            call_stack: Vec::new(),
            heap: Heap::new(),
            debug_output: Vec::new(),
            halted: false,
        }
    }

    /// Run the program until `Halt` or end of instructions.
    ///
    /// # Errors
    ///
    /// Returns `VmError` if any runtime error occurs.
    pub fn run(&mut self) -> Result<(), VmError> {
        while !self.halted {
            if self.pc >= self.program.len() {
                break;
            }
            self.step()?;
        }
        Ok(())
    }

    /// Execute a single instruction.
    ///
    /// # Errors
    ///
    /// Returns `VmError` if the instruction causes a runtime error.
    pub fn step(&mut self) -> Result<(), VmError> {
        if self.pc >= self.program.len() {
            return Err(VmError::ProgramCounterOutOfBounds);
        }
        let op = self.program[self.pc];
        self.pc += 1;
        self.execute(op)
    }

    /// Return the current top of the stack, if any.
    #[must_use]
    pub fn top(&self) -> Option<i64> {
        self.stack.last().copied()
    }

    /// Return the current stack as a slice.
    #[must_use]
    pub fn stack(&self) -> &[i64] {
        &self.stack
    }

    /// Read a register value.
    ///
    /// # Errors
    ///
    /// Returns `VmError::InvalidRegister` if `r` >= `NUM_REGISTERS`.
    pub const fn get_register(&self, r: u8) -> Result<i64, VmError> {
        if (r as usize) < NUM_REGISTERS {
            Ok(self.registers[r as usize])
        } else {
            Err(VmError::InvalidRegister(r))
        }
    }

    /// Check whether the VM has halted.
    #[must_use]
    pub const fn is_halted(&self) -> bool {
        self.halted
    }

    // -- private helpers --

    fn push(&mut self, val: i64) -> Result<(), VmError> {
        if self.stack.len() >= MAX_STACK {
            return Err(VmError::StackOverflow);
        }
        self.stack.push(val);
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn current_base(&self) -> usize {
        self.call_stack.last().map_or(0, |frame| frame.base_ptr)
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn execute(&mut self, op: Op) -> Result<(), VmError> {
        match op {
            // -- Stack --
            Op::Push(v) => self.push(v)?,
            Op::Pop => {
                self.pop()?;
            }
            Op::Dup => {
                let v = *self.stack.last().ok_or(VmError::StackUnderflow)?;
                self.push(v)?;
            }
            Op::Swap => {
                let len = self.stack.len();
                if len < 2 {
                    return Err(VmError::StackUnderflow);
                }
                self.stack.swap(len - 1, len - 2);
            }
            Op::Over(n) => {
                let len = self.stack.len();
                if n >= len {
                    return Err(VmError::StackUnderflow);
                }
                let v = self.stack[len - 1 - n];
                self.push(v)?;
            }

            // -- Arithmetic --
            Op::Add => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.wrapping_add(b))?;
            }
            Op::Sub => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.wrapping_sub(b))?;
            }
            Op::Mul => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.wrapping_mul(b))?;
            }
            Op::Div => {
                let b = self.pop()?;
                let a = self.pop()?;
                if b == 0 {
                    return Err(VmError::DivisionByZero);
                }
                self.push(a.wrapping_div(b))?;
            }
            Op::Rem => {
                let b = self.pop()?;
                let a = self.pop()?;
                if b == 0 {
                    return Err(VmError::DivisionByZero);
                }
                self.push(a.wrapping_rem(b))?;
            }
            Op::Neg => {
                let a = self.pop()?;
                self.push(a.wrapping_neg())?;
            }
            Op::Abs => {
                let a = self.pop()?;
                self.push(a.wrapping_abs())?;
            }

            // -- Bitwise --
            Op::BitAnd => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a & b)?;
            }
            Op::BitOr => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a | b)?;
            }
            Op::BitXor => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a ^ b)?;
            }
            Op::BitNot => {
                let a = self.pop()?;
                self.push(!a)?;
            }
            Op::Shl => {
                let b = self.pop()?;
                let a = self.pop()?;
                let shift = (b as u64) & 63;
                let result = (a.cast_unsigned() << shift).cast_signed();
                self.push(result)?;
            }
            Op::Shr => {
                let b = self.pop()?;
                let a = self.pop()?;
                let shift = (b as u64) & 63;
                // Arithmetic shift right.
                self.push(a >> shift)?;
            }

            // -- Comparison --
            Op::Eq => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(i64::from(a == b))?;
            }
            Op::Ne => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(i64::from(a != b))?;
            }
            Op::Lt => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(i64::from(a < b))?;
            }
            Op::Le => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(i64::from(a <= b))?;
            }
            Op::Gt => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(i64::from(a > b))?;
            }
            Op::Ge => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(i64::from(a >= b))?;
            }

            // -- Control flow --
            Op::Jump(addr) => {
                if addr > self.program.len() {
                    return Err(VmError::InvalidJump(addr));
                }
                self.pc = addr;
            }
            Op::JumpIf(addr) => {
                let cond = self.pop()?;
                if cond != 0 {
                    if addr > self.program.len() {
                        return Err(VmError::InvalidJump(addr));
                    }
                    self.pc = addr;
                }
            }
            Op::JumpIfZero(addr) => {
                let cond = self.pop()?;
                if cond == 0 {
                    if addr > self.program.len() {
                        return Err(VmError::InvalidJump(addr));
                    }
                    self.pc = addr;
                }
            }

            // -- Registers --
            Op::LoadReg(r) => {
                if (r as usize) >= NUM_REGISTERS {
                    return Err(VmError::InvalidRegister(r));
                }
                self.push(self.registers[r as usize])?;
            }
            Op::StoreReg(r) => {
                if (r as usize) >= NUM_REGISTERS {
                    return Err(VmError::InvalidRegister(r));
                }
                let v = self.pop()?;
                self.registers[r as usize] = v;
            }

            // -- Locals --
            Op::LoadLocal(offset) => {
                let base = self.current_base();
                let idx = base + offset;
                if idx >= self.stack.len() {
                    return Err(VmError::InvalidLocal(offset));
                }
                let v = self.stack[idx];
                self.push(v)?;
            }
            Op::StoreLocal(offset) => {
                let v = self.pop()?;
                let base = self.current_base();
                let idx = base + offset;
                if idx >= self.stack.len() {
                    return Err(VmError::InvalidLocal(offset));
                }
                self.stack[idx] = v;
            }

            // -- Calls --
            Op::Call(addr, n_args) => {
                if self.call_stack.len() >= MAX_CALL_DEPTH {
                    return Err(VmError::CallStackOverflow);
                }
                if addr > self.program.len() {
                    return Err(VmError::InvalidJump(addr));
                }
                let base = self.stack.len().saturating_sub(n_args);
                self.call_stack.push(CallFrame {
                    return_addr: self.pc,
                    base_ptr: base,
                    _n_locals: n_args,
                });
                self.pc = addr;
            }
            Op::Ret => {
                let frame = self.call_stack.pop().ok_or(VmError::CallStackUnderflow)?;
                let ret_val = self.pop()?;
                // Pop locals / arguments.
                self.stack.truncate(frame.base_ptr);
                self.push(ret_val)?;
                self.pc = frame.return_addr;
            }

            // -- Heap --
            Op::HeapAlloc => {
                let size = self.pop()?;
                if size <= 0 {
                    return Err(VmError::InvalidHeapSize);
                }
                let addr = self.heap.alloc(size as usize)?;
                self.push(addr as i64)?;
            }
            Op::HeapFree => {
                let addr = self.pop()?;
                self.heap.free(addr as usize)?;
            }
            Op::HeapLoad => {
                let addr = self.pop()?;
                let val = self.heap.load(addr as usize)?;
                self.push(val)?;
            }
            Op::HeapStore => {
                let val = self.pop()?;
                let addr = self.pop()?;
                self.heap.store(addr as usize, val)?;
            }
            Op::HeapLoadOffset => {
                let offset = self.pop()?;
                let addr = self.pop()?;
                let effective = (addr as usize) + (offset as usize);
                let val = self.heap.load(effective)?;
                self.push(val)?;
            }
            Op::HeapStoreOffset => {
                let val = self.pop()?;
                let offset = self.pop()?;
                let addr = self.pop()?;
                let effective = (addr as usize) + (offset as usize);
                self.heap.store(effective, val)?;
            }

            // -- Misc --
            Op::Nop => {}
            Op::Halt => {
                self.halted = true;
            }
            Op::DebugPrint => {
                let v = *self.stack.last().ok_or(VmError::StackUnderflow)?;
                self.debug_output.push(v);
            }
            Op::Inc => {
                let v = self.pop()?;
                self.push(v.wrapping_add(1))?;
            }
            Op::Dec => {
                let v = self.pop()?;
                self.push(v.wrapping_sub(1))?;
            }
        }
        Ok(())
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper
    // -----------------------------------------------------------------------
    fn run(program: Vec<Op>) -> Vm {
        let mut vm = Vm::new(program);
        vm.run().unwrap();
        vm
    }

    fn run_err(program: Vec<Op>) -> VmError {
        let mut vm = Vm::new(program);
        vm.run().unwrap_err()
    }

    // -----------------------------------------------------------------------
    // Stack operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_push() {
        let vm = run(vec![Op::Push(42), Op::Halt]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_push_negative() {
        let vm = run(vec![Op::Push(-100), Op::Halt]);
        assert_eq!(vm.top(), Some(-100));
    }

    #[test]
    fn test_pop() {
        let vm = run(vec![Op::Push(1), Op::Push(2), Op::Pop, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_pop_underflow() {
        let err = run_err(vec![Op::Pop]);
        assert_eq!(err, VmError::StackUnderflow);
    }

    #[test]
    fn test_dup() {
        let vm = run(vec![Op::Push(7), Op::Dup, Op::Halt]);
        assert_eq!(vm.stack(), &[7, 7]);
    }

    #[test]
    fn test_dup_empty() {
        let err = run_err(vec![Op::Dup]);
        assert_eq!(err, VmError::StackUnderflow);
    }

    #[test]
    fn test_swap() {
        let vm = run(vec![Op::Push(1), Op::Push(2), Op::Swap, Op::Halt]);
        assert_eq!(vm.stack(), &[2, 1]);
    }

    #[test]
    fn test_swap_underflow() {
        let err = run_err(vec![Op::Push(1), Op::Swap]);
        assert_eq!(err, VmError::StackUnderflow);
    }

    #[test]
    fn test_over() {
        let vm = run(vec![Op::Push(10), Op::Push(20), Op::Over(1), Op::Halt]);
        assert_eq!(vm.stack(), &[10, 20, 10]);
    }

    #[test]
    fn test_over_zero() {
        let vm = run(vec![Op::Push(5), Op::Over(0), Op::Halt]);
        assert_eq!(vm.stack(), &[5, 5]);
    }

    #[test]
    fn test_over_underflow() {
        let err = run_err(vec![Op::Push(1), Op::Over(5)]);
        assert_eq!(err, VmError::StackUnderflow);
    }

    // -----------------------------------------------------------------------
    // Arithmetic
    // -----------------------------------------------------------------------

    #[test]
    fn test_add() {
        let vm = run(vec![Op::Push(3), Op::Push(4), Op::Add, Op::Halt]);
        assert_eq!(vm.top(), Some(7));
    }

    #[test]
    fn test_add_negative() {
        let vm = run(vec![Op::Push(-3), Op::Push(-4), Op::Add, Op::Halt]);
        assert_eq!(vm.top(), Some(-7));
    }

    #[test]
    fn test_sub() {
        let vm = run(vec![Op::Push(10), Op::Push(3), Op::Sub, Op::Halt]);
        assert_eq!(vm.top(), Some(7));
    }

    #[test]
    fn test_mul() {
        let vm = run(vec![Op::Push(6), Op::Push(7), Op::Mul, Op::Halt]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_div() {
        let vm = run(vec![Op::Push(20), Op::Push(4), Op::Div, Op::Halt]);
        assert_eq!(vm.top(), Some(5));
    }

    #[test]
    fn test_div_by_zero() {
        let err = run_err(vec![Op::Push(1), Op::Push(0), Op::Div]);
        assert_eq!(err, VmError::DivisionByZero);
    }

    #[test]
    fn test_rem() {
        let vm = run(vec![Op::Push(10), Op::Push(3), Op::Rem, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_rem_by_zero() {
        let err = run_err(vec![Op::Push(1), Op::Push(0), Op::Rem]);
        assert_eq!(err, VmError::DivisionByZero);
    }

    #[test]
    fn test_neg() {
        let vm = run(vec![Op::Push(5), Op::Neg, Op::Halt]);
        assert_eq!(vm.top(), Some(-5));
    }

    #[test]
    fn test_neg_negative() {
        let vm = run(vec![Op::Push(-5), Op::Neg, Op::Halt]);
        assert_eq!(vm.top(), Some(5));
    }

    #[test]
    fn test_abs_positive() {
        let vm = run(vec![Op::Push(5), Op::Abs, Op::Halt]);
        assert_eq!(vm.top(), Some(5));
    }

    #[test]
    fn test_abs_negative() {
        let vm = run(vec![Op::Push(-5), Op::Abs, Op::Halt]);
        assert_eq!(vm.top(), Some(5));
    }

    #[test]
    fn test_inc() {
        let vm = run(vec![Op::Push(9), Op::Inc, Op::Halt]);
        assert_eq!(vm.top(), Some(10));
    }

    #[test]
    fn test_dec() {
        let vm = run(vec![Op::Push(10), Op::Dec, Op::Halt]);
        assert_eq!(vm.top(), Some(9));
    }

    // -----------------------------------------------------------------------
    // Bitwise
    // -----------------------------------------------------------------------

    #[test]
    fn test_bit_and() {
        let vm = run(vec![
            Op::Push(0b1100),
            Op::Push(0b1010),
            Op::BitAnd,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(0b1000));
    }

    #[test]
    fn test_bit_or() {
        let vm = run(vec![
            Op::Push(0b1100),
            Op::Push(0b1010),
            Op::BitOr,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(0b1110));
    }

    #[test]
    fn test_bit_xor() {
        let vm = run(vec![
            Op::Push(0b1100),
            Op::Push(0b1010),
            Op::BitXor,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(0b0110));
    }

    #[test]
    fn test_bit_not() {
        let vm = run(vec![Op::Push(0), Op::BitNot, Op::Halt]);
        assert_eq!(vm.top(), Some(-1));
    }

    #[test]
    fn test_shl() {
        let vm = run(vec![Op::Push(1), Op::Push(4), Op::Shl, Op::Halt]);
        assert_eq!(vm.top(), Some(16));
    }

    #[test]
    fn test_shr() {
        let vm = run(vec![Op::Push(16), Op::Push(2), Op::Shr, Op::Halt]);
        assert_eq!(vm.top(), Some(4));
    }

    #[test]
    fn test_shr_arithmetic() {
        let vm = run(vec![Op::Push(-8), Op::Push(1), Op::Shr, Op::Halt]);
        assert_eq!(vm.top(), Some(-4));
    }

    // -----------------------------------------------------------------------
    // Comparison
    // -----------------------------------------------------------------------

    #[test]
    fn test_eq_true() {
        let vm = run(vec![Op::Push(5), Op::Push(5), Op::Eq, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_eq_false() {
        let vm = run(vec![Op::Push(5), Op::Push(6), Op::Eq, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_ne_true() {
        let vm = run(vec![Op::Push(5), Op::Push(6), Op::Ne, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_ne_false() {
        let vm = run(vec![Op::Push(5), Op::Push(5), Op::Ne, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_lt_true() {
        let vm = run(vec![Op::Push(3), Op::Push(5), Op::Lt, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_lt_false() {
        let vm = run(vec![Op::Push(5), Op::Push(3), Op::Lt, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_le_equal() {
        let vm = run(vec![Op::Push(5), Op::Push(5), Op::Le, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_le_less() {
        let vm = run(vec![Op::Push(3), Op::Push(5), Op::Le, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_gt_true() {
        let vm = run(vec![Op::Push(5), Op::Push(3), Op::Gt, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_ge_equal() {
        let vm = run(vec![Op::Push(5), Op::Push(5), Op::Ge, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    // -----------------------------------------------------------------------
    // Control flow
    // -----------------------------------------------------------------------

    #[test]
    fn test_jump() {
        // Push(1), Jump(3), Push(999), Push(2), Halt
        // Should skip Push(999).
        let vm = run(vec![
            Op::Push(1),
            Op::Jump(3),
            Op::Push(999),
            Op::Push(2),
            Op::Halt,
        ]);
        assert_eq!(vm.stack(), &[1, 2]);
    }

    #[test]
    fn test_jump_if_true() {
        let vm = run(vec![
            Op::Push(1),
            Op::JumpIf(3),
            Op::Push(999),
            Op::Push(42),
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_jump_if_false() {
        let vm = run(vec![Op::Push(0), Op::JumpIf(3), Op::Push(999), Op::Halt]);
        assert_eq!(vm.top(), Some(999));
    }

    #[test]
    fn test_jump_if_zero_true() {
        let vm = run(vec![
            Op::Push(0),
            Op::JumpIfZero(3),
            Op::Push(999),
            Op::Push(42),
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_jump_if_zero_false() {
        let vm = run(vec![
            Op::Push(1),
            Op::JumpIfZero(3),
            Op::Push(999),
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(999));
    }

    #[test]
    fn test_invalid_jump() {
        let err = run_err(vec![Op::Jump(9999)]);
        assert_eq!(err, VmError::InvalidJump(9999));
    }

    // -----------------------------------------------------------------------
    // Registers
    // -----------------------------------------------------------------------

    #[test]
    fn test_store_load_reg() {
        let vm = run(vec![
            Op::Push(42),
            Op::StoreReg(0),
            Op::LoadReg(0),
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_multiple_registers() {
        let vm = run(vec![
            Op::Push(10),
            Op::StoreReg(0),
            Op::Push(20),
            Op::StoreReg(1),
            Op::LoadReg(0),
            Op::LoadReg(1),
            Op::Add,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(30));
    }

    #[test]
    fn test_invalid_register_load() {
        let err = run_err(vec![Op::LoadReg(99)]);
        assert_eq!(err, VmError::InvalidRegister(99));
    }

    #[test]
    fn test_invalid_register_store() {
        let err = run_err(vec![Op::Push(1), Op::StoreReg(99)]);
        assert_eq!(err, VmError::InvalidRegister(99));
    }

    #[test]
    fn test_get_register() {
        let vm = run(vec![Op::Push(77), Op::StoreReg(5), Op::Halt]);
        assert_eq!(vm.get_register(5).unwrap(), 77);
        assert!(vm.get_register(99).is_err());
    }

    // -----------------------------------------------------------------------
    // Locals
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_store_local() {
        // Simulate a function call with 2 args.
        // Stack: [arg0=10, arg1=20]
        // Call function at instruction 3 with 2 args.
        let vm = run(vec![
            Op::Push(10),   // 0: arg0
            Op::Push(20),   // 1: arg1
            Op::Call(3, 2), // 2: call func(2 args) at instr 3
            Op::Halt,       // 3 (return lands here, but we also jump here)
                            // Actually, func body starts at 3:
                            // But we need to reorganize. Let's adjust:
        ]);
        // The above ends at Halt directly. Let's do a proper version:
        assert_eq!(vm.top(), Some(20)); // Ret not called, so Call jumps to 3 = Halt
    }

    #[test]
    fn test_locals_in_call() {
        // func at 4: loads local 0, local 1, adds, returns
        let vm = run(vec![
            Op::Push(10),     // 0
            Op::Push(20),     // 1
            Op::Call(4, 2),   // 2: call func at 4 with 2 args
            Op::Halt,         // 3: after return
            Op::LoadLocal(0), // 4: func body — push arg0 (10)
            Op::LoadLocal(1), // 5: push arg1 (20)
            Op::Add,          // 6: 10+20 = 30
            Op::Ret,          // 7: return 30
        ]);
        assert_eq!(vm.top(), Some(30));
    }

    // -----------------------------------------------------------------------
    // Call frames
    // -----------------------------------------------------------------------

    #[test]
    fn test_simple_call_ret() {
        // Main: push 5, call double(5), halt
        // double: load arg0, dup, add, ret
        let vm = run(vec![
            Op::Push(5),      // 0
            Op::Call(3, 1),   // 1: call at 3 with 1 arg
            Op::Halt,         // 2
            Op::LoadLocal(0), // 3: func — load arg0
            Op::Dup,          // 4
            Op::Add,          // 5: arg0 * 2
            Op::Ret,          // 6
        ]);
        assert_eq!(vm.top(), Some(10));
    }

    #[test]
    fn test_nested_calls() {
        // Main: push 3, call f(3), halt
        // f(x): push x, push 2, call g(x,2), ret
        // g(a,b): load a, load b, mul, ret
        let vm = run(vec![
            Op::Push(3),    // 0: push arg
            Op::Call(3, 1), // 1: call f at 3
            Op::Halt,       // 2
            // f: at 3
            Op::LoadLocal(0), // 3: push x
            Op::Push(2),      // 4: push 2
            Op::Call(7, 2),   // 5: call g at 7 with 2 args
            Op::Ret,          // 6: return g's result
            // g: at 7
            Op::LoadLocal(0), // 7: load a
            Op::LoadLocal(1), // 8: load b
            Op::Mul,          // 9: a * b
            Op::Ret,          // 10
        ]);
        assert_eq!(vm.top(), Some(6));
    }

    #[test]
    fn test_ret_without_call() {
        let err = run_err(vec![Op::Push(1), Op::Ret]);
        assert_eq!(err, VmError::CallStackUnderflow);
    }

    // -----------------------------------------------------------------------
    // Heap
    // -----------------------------------------------------------------------

    #[test]
    fn test_heap_alloc_store_load() {
        let vm = run(vec![
            Op::Push(1),   // alloc 1 word
            Op::HeapAlloc, // addr on stack
            Op::Dup,       // keep addr for load
            Op::Push(999), // value
            Op::HeapStore, // store 999 at addr (pops value then addr)
            // But wait — HeapStore pops value then addr. So stack is [addr].
            // Actually after Dup: [addr, addr], Push(999): [addr, addr, 999]
            // HeapStore pops 999 (value), then pops addr. Stack: [addr].
            // Now just HeapLoad.
            Op::HeapLoad,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(999));
    }

    #[test]
    fn test_heap_free() {
        let vm = run(vec![Op::Push(1), Op::HeapAlloc, Op::HeapFree, Op::Halt]);
        assert_eq!(vm.top(), None);
    }

    #[test]
    fn test_heap_double_free() {
        let err = run_err(vec![
            Op::Push(1),
            Op::HeapAlloc,
            Op::Dup,
            Op::HeapFree,
            Op::HeapFree,
        ]);
        assert_eq!(err, VmError::HeapDoubleFree(0));
    }

    #[test]
    fn test_heap_alloc_zero() {
        let err = run_err(vec![Op::Push(0), Op::HeapAlloc]);
        assert_eq!(err, VmError::InvalidHeapSize);
    }

    #[test]
    fn test_heap_alloc_negative() {
        let err = run_err(vec![Op::Push(-1), Op::HeapAlloc]);
        assert_eq!(err, VmError::InvalidHeapSize);
    }

    #[test]
    fn test_heap_invalid_address() {
        let err = run_err(vec![Op::Push(99999), Op::HeapLoad]);
        assert_eq!(err, VmError::HeapInvalidAddress(99999));
    }

    #[test]
    fn test_heap_load_offset() {
        let vm = run(vec![
            Op::Push(4),     // alloc 4 words
            Op::HeapAlloc,   // addr=0
            Op::StoreReg(0), // save addr to R0
            // Store 42 at offset 2
            Op::LoadReg(0), // push addr
            Op::Push(2),    // push offset
            Op::Push(42),   // push value
            Op::HeapStoreOffset,
            // Load from offset 2
            Op::LoadReg(0), // push addr
            Op::Push(2),    // push offset
            Op::HeapLoadOffset,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_heap_multiple_allocs() {
        let vm = run(vec![
            Op::Push(2),
            Op::HeapAlloc, // addr1 = 0
            Op::StoreReg(0),
            Op::Push(3),
            Op::HeapAlloc, // addr2 = 2
            Op::StoreReg(1),
            // Store values
            Op::LoadReg(0),
            Op::Push(100),
            Op::HeapStore,
            Op::LoadReg(1),
            Op::Push(200),
            Op::HeapStore,
            // Load them back
            Op::LoadReg(0),
            Op::HeapLoad,
            Op::LoadReg(1),
            Op::HeapLoad,
            Op::Add,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(300));
    }

    // -----------------------------------------------------------------------
    // Complex programs
    // -----------------------------------------------------------------------

    #[test]
    fn test_factorial_iterative() {
        // Compute 5! = 120 using a loop.
        // R0 = n (counter), R1 = accumulator
        let vm = run(vec![
            Op::Push(5),     // 0
            Op::StoreReg(0), // 1: R0 = 5
            Op::Push(1),     // 2
            Op::StoreReg(1), // 3: R1 = 1
            // Loop start (4):
            Op::LoadReg(0),  // 4: push R0
            Op::Push(0),     // 5
            Op::Le,          // 6: R0 <= 0 ?
            Op::JumpIf(16),  // 7: if true, jump to end (index 16)
            Op::LoadReg(1),  // 8
            Op::LoadReg(0),  // 9
            Op::Mul,         // 10: R1 * R0
            Op::StoreReg(1), // 11: R1 = R1 * R0
            Op::LoadReg(0),  // 12
            Op::Dec,         // 13: R0 - 1
            Op::StoreReg(0), // 14: R0 = R0 - 1
            Op::Jump(4),     // 15: back to loop
            Op::LoadReg(1),  // 16: push result
            Op::Halt,        // 17
        ]);
        assert_eq!(vm.top(), Some(120));
    }

    #[test]
    fn test_factorial_5() {
        // Compute 5! = 120.
        // R0 = counter, R1 = accumulator.
        let vm = run(vec![
            Op::Push(5),     // 0
            Op::StoreReg(0), // 1: R0=5
            Op::Push(1),     // 2
            Op::StoreReg(1), // 3: R1=1
            // Loop (index 4):
            Op::LoadReg(0),  // 4: push R0
            Op::Push(1),     // 5
            Op::Lt,          // 6: R0 < 1?
            Op::JumpIf(16),  // 7: if yes, exit loop -> go to 16
            Op::LoadReg(1),  // 8
            Op::LoadReg(0),  // 9
            Op::Mul,         // 10: R1 * R0
            Op::StoreReg(1), // 11: R1 = R1*R0
            Op::LoadReg(0),  // 12
            Op::Dec,         // 13
            Op::StoreReg(0), // 14: R0--
            Op::Jump(4),     // 15: back to loop
            Op::LoadReg(1),  // 16: push result
            Op::Halt,        // 17
        ]);
        assert_eq!(vm.top(), Some(120));
    }

    #[test]
    fn test_fibonacci_10() {
        // Compute fib(10) = 55 iteratively.
        // R0 = n (counter), R1 = a, R2 = b
        let vm = run(vec![
            Op::Push(10),    // 0
            Op::StoreReg(0), // 1: R0=10
            Op::Push(0),     // 2
            Op::StoreReg(1), // 3: R1=0 (a)
            Op::Push(1),     // 4
            Op::StoreReg(2), // 5: R2=1 (b)
            // Loop (index 6):
            Op::LoadReg(0), // 6: push R0
            Op::Push(0),    // 7
            Op::Le,         // 8: R0 <= 0?
            Op::JumpIf(22), // 9: exit loop
            // temp = a + b
            Op::LoadReg(1),  // 10: push a
            Op::LoadReg(2),  // 11: push b
            Op::Add,         // 12: a+b
            Op::StoreReg(3), // 13: R3 = a+b (temp)
            // a = b
            Op::LoadReg(2),  // 14
            Op::StoreReg(1), // 15: R1 = R2
            // b = temp
            Op::LoadReg(3),  // 16
            Op::StoreReg(2), // 17: R2 = R3
            // n--
            Op::LoadReg(0),  // 18
            Op::Dec,         // 19
            Op::StoreReg(0), // 20: R0--
            Op::Jump(6),     // 21: back to loop
            Op::LoadReg(1),  // 22: push result (a)
            Op::Halt,        // 23
        ]);
        assert_eq!(vm.top(), Some(55));
    }

    #[test]
    fn test_sum_1_to_100() {
        // Sum 1..=100 = 5050.
        let vm = run(vec![
            Op::Push(100),   // 0: n
            Op::StoreReg(0), // 1
            Op::Push(0),     // 2: sum
            Op::StoreReg(1), // 3
            // Loop (4):
            Op::LoadReg(0),  // 4
            Op::Push(0),     // 5
            Op::Le,          // 6: n <= 0?
            Op::JumpIf(16),  // 7
            Op::LoadReg(1),  // 8
            Op::LoadReg(0),  // 9
            Op::Add,         // 10
            Op::StoreReg(1), // 11: sum += n
            Op::LoadReg(0),  // 12
            Op::Dec,         // 13
            Op::StoreReg(0), // 14: n--
            Op::Jump(4),     // 15
            Op::LoadReg(1),  // 16
            Op::Halt,        // 17
        ]);
        assert_eq!(vm.top(), Some(5050));
    }

    #[test]
    fn test_recursive_call() {
        // Recursive sum: sum(n) = n + sum(n-1), sum(0) = 0
        // Main: push 5, call sum, halt
        // sum (at 3): if arg0==0 ret 0, else arg0 + call sum(arg0-1)
        let vm = run(vec![
            Op::Push(5),    // 0
            Op::Call(3, 1), // 1
            Op::Halt,       // 2
            // sum(n) at 3:
            Op::LoadLocal(0), // 3: push n
            Op::Push(0),      // 4
            Op::Eq,           // 5: n == 0?
            Op::JumpIf(14),   // 6: if yes, return 0
            // n + sum(n-1)
            Op::LoadLocal(0), // 7: push n (for the add later)
            Op::LoadLocal(0), // 8: push n
            Op::Dec,          // 9: n-1
            Op::Call(3, 1),   // 10: sum(n-1)
            // After return, stack has: [n, sum(n-1)]
            Op::Add, // 11: n + sum(n-1)
            Op::Ret, // 12
            // Dead code (jump target needs something)
            Op::Push(0), // 13: padding
            // Base case (14):
            Op::Push(0), // 14: push 0
            Op::Ret,     // 15
        ]);
        assert_eq!(vm.top(), Some(15)); // 5+4+3+2+1+0 = 15
    }

    // -----------------------------------------------------------------------
    // Misc
    // -----------------------------------------------------------------------

    #[test]
    fn test_nop() {
        let vm = run(vec![Op::Push(1), Op::Nop, Op::Nop, Op::Halt]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_halt() {
        let vm = run(vec![Op::Halt, Op::Push(999)]);
        assert!(vm.is_halted());
        assert_eq!(vm.top(), None);
    }

    #[test]
    fn test_debug_print() {
        let vm = run(vec![Op::Push(42), Op::DebugPrint, Op::Halt]);
        assert_eq!(vm.debug_output, vec![42]);
    }

    #[test]
    fn test_debug_print_multiple() {
        let vm = run(vec![
            Op::Push(1),
            Op::DebugPrint,
            Op::Push(2),
            Op::DebugPrint,
            Op::Halt,
        ]);
        assert_eq!(vm.debug_output, vec![1, 2]);
    }

    #[test]
    fn test_debug_print_empty_stack() {
        let err = run_err(vec![Op::DebugPrint]);
        assert_eq!(err, VmError::StackUnderflow);
    }

    #[test]
    fn test_empty_program() {
        let vm = run(vec![]);
        assert_eq!(vm.top(), None);
        assert!(!vm.is_halted());
    }

    #[test]
    fn test_pc_out_of_bounds_step() {
        let mut vm = Vm::new(vec![Op::Halt]);
        vm.run().unwrap();
        assert!(vm.step().is_err());
    }

    #[test]
    fn test_is_halted_false() {
        let vm = run(vec![Op::Push(1)]);
        assert!(!vm.is_halted());
    }

    #[test]
    fn test_is_halted_true() {
        let vm = run(vec![Op::Halt]);
        assert!(vm.is_halted());
    }

    // -----------------------------------------------------------------------
    // Error display
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_display_stack_overflow() {
        assert_eq!(format!("{}", VmError::StackOverflow), "stack overflow");
    }

    #[test]
    fn test_error_display_stack_underflow() {
        assert_eq!(format!("{}", VmError::StackUnderflow), "stack underflow");
    }

    #[test]
    fn test_error_display_div_zero() {
        assert_eq!(format!("{}", VmError::DivisionByZero), "division by zero");
    }

    #[test]
    fn test_error_display_invalid_register() {
        assert_eq!(
            format!("{}", VmError::InvalidRegister(5)),
            "invalid register: 5"
        );
    }

    #[test]
    fn test_error_display_invalid_jump() {
        assert_eq!(
            format!("{}", VmError::InvalidJump(99)),
            "invalid jump target: 99"
        );
    }

    #[test]
    fn test_error_display_call_overflow() {
        assert_eq!(
            format!("{}", VmError::CallStackOverflow),
            "call stack overflow"
        );
    }

    #[test]
    fn test_error_display_call_underflow() {
        assert_eq!(
            format!("{}", VmError::CallStackUnderflow),
            "call stack underflow"
        );
    }

    #[test]
    fn test_error_display_invalid_local() {
        assert_eq!(
            format!("{}", VmError::InvalidLocal(3)),
            "invalid local offset: 3"
        );
    }

    #[test]
    fn test_error_display_heap_oom() {
        assert_eq!(
            format!("{}", VmError::HeapOutOfMemory),
            "heap out of memory"
        );
    }

    #[test]
    fn test_error_display_heap_invalid() {
        assert_eq!(
            format!("{}", VmError::HeapInvalidAddress(10)),
            "invalid heap address: 10"
        );
    }

    #[test]
    fn test_error_display_double_free() {
        assert_eq!(
            format!("{}", VmError::HeapDoubleFree(0)),
            "double free at address: 0"
        );
    }

    #[test]
    fn test_error_display_pc_oob() {
        assert_eq!(
            format!("{}", VmError::ProgramCounterOutOfBounds),
            "program counter out of bounds"
        );
    }

    #[test]
    fn test_error_display_invalid_heap_size() {
        assert_eq!(
            format!("{}", VmError::InvalidHeapSize),
            "invalid heap allocation size"
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases and additional coverage
    // -----------------------------------------------------------------------

    #[test]
    fn test_chain_arithmetic() {
        // (2 + 3) * (4 - 1) = 5 * 3 = 15
        let vm = run(vec![
            Op::Push(2),
            Op::Push(3),
            Op::Add,
            Op::Push(4),
            Op::Push(1),
            Op::Sub,
            Op::Mul,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(15));
    }

    #[test]
    fn test_wrapping_add_overflow() {
        let vm = run(vec![Op::Push(i64::MAX), Op::Push(1), Op::Add, Op::Halt]);
        assert_eq!(vm.top(), Some(i64::MIN));
    }

    #[test]
    fn test_wrapping_mul_overflow() {
        let vm = run(vec![Op::Push(i64::MAX), Op::Push(2), Op::Mul, Op::Halt]);
        assert_eq!(vm.top(), Some(-2));
    }

    #[test]
    fn test_push_zero() {
        let vm = run(vec![Op::Push(0), Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_push_max() {
        let vm = run(vec![Op::Push(i64::MAX), Op::Halt]);
        assert_eq!(vm.top(), Some(i64::MAX));
    }

    #[test]
    fn test_push_min() {
        let vm = run(vec![Op::Push(i64::MIN), Op::Halt]);
        assert_eq!(vm.top(), Some(i64::MIN));
    }

    #[test]
    fn test_register_all_16() {
        let mut program = Vec::new();
        for i in 0..16u8 {
            program.push(Op::Push(i64::from(i) * 10));
            program.push(Op::StoreReg(i));
        }
        for i in 0..16u8 {
            program.push(Op::LoadReg(i));
        }
        program.push(Op::Halt);
        let vm = run(program);
        let expected: Vec<i64> = (0..16).map(|i| i * 10).collect();
        assert_eq!(vm.stack(), expected.as_slice());
    }

    #[test]
    fn test_conditional_branch_pattern() {
        // if x > 10 then push 1 else push 0
        // x = 15
        let vm = run(vec![
            Op::Push(15),  // 0: x
            Op::Push(10),  // 1: threshold
            Op::Gt,        // 2: x > 10?
            Op::JumpIf(6), // 3: if true, go to 6
            Op::Push(0),   // 4: false branch
            Op::Jump(7),   // 5: skip true branch
            Op::Push(1),   // 6: true branch
            Op::Halt,      // 7
        ]);
        assert_eq!(vm.top(), Some(1));
    }

    #[test]
    fn test_conditional_branch_false() {
        // x = 5, threshold = 10
        let vm = run(vec![
            Op::Push(5),
            Op::Push(10),
            Op::Gt,
            Op::JumpIf(6),
            Op::Push(0),
            Op::Jump(7),
            Op::Push(1),
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_heap_alloc_free_realloc() {
        // Alloc, free, alloc again — should reuse same address.
        let vm = run(vec![
            Op::Push(4),
            Op::HeapAlloc, // addr1
            Op::StoreReg(0),
            Op::LoadReg(0),
            Op::HeapFree,
            Op::Push(4),
            Op::HeapAlloc, // addr2 (should be same as addr1)
            Op::StoreReg(1),
            Op::LoadReg(0),
            Op::LoadReg(1),
            Op::Eq,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(1)); // addresses are equal
    }

    #[test]
    fn test_heap_store_load_array() {
        // Store array [10, 20, 30] on heap and read back.
        let vm = run(vec![
            Op::Push(3),
            Op::HeapAlloc, // base addr
            Op::StoreReg(0),
            // store [0] = 10
            Op::LoadReg(0),
            Op::Push(0),
            Op::Push(10),
            Op::HeapStoreOffset,
            // store [1] = 20
            Op::LoadReg(0),
            Op::Push(1),
            Op::Push(20),
            Op::HeapStoreOffset,
            // store [2] = 30
            Op::LoadReg(0),
            Op::Push(2),
            Op::Push(30),
            Op::HeapStoreOffset,
            // load [0] + [1] + [2]
            Op::LoadReg(0),
            Op::Push(0),
            Op::HeapLoadOffset,
            Op::LoadReg(0),
            Op::Push(1),
            Op::HeapLoadOffset,
            Op::Add,
            Op::LoadReg(0),
            Op::Push(2),
            Op::HeapLoadOffset,
            Op::Add,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(60));
    }

    #[test]
    fn test_many_nops() {
        let mut program = vec![Op::Push(42)];
        for _ in 0..100 {
            program.push(Op::Nop);
        }
        program.push(Op::Halt);
        let vm = run(program);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_step_by_step() {
        let mut vm = Vm::new(vec![Op::Push(1), Op::Push(2), Op::Add, Op::Halt]);
        vm.step().unwrap(); // Push(1)
        assert_eq!(vm.stack(), &[1]);
        vm.step().unwrap(); // Push(2)
        assert_eq!(vm.stack(), &[1, 2]);
        vm.step().unwrap(); // Add
        assert_eq!(vm.stack(), &[3]);
        vm.step().unwrap(); // Halt
        assert!(vm.is_halted());
    }

    #[test]
    fn test_op_debug_format() {
        let op = Op::Push(42);
        assert_eq!(format!("{op:?}"), "Push(42)");
    }

    #[test]
    fn test_op_clone() {
        let op = Op::Add;
        let op2 = op;
        assert_eq!(op, op2);
    }

    #[test]
    fn test_vm_error_is_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(VmError::StackOverflow);
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_gt_false() {
        let vm = run(vec![Op::Push(3), Op::Push(5), Op::Gt, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_ge_less() {
        let vm = run(vec![Op::Push(3), Op::Push(5), Op::Ge, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_le_greater() {
        let vm = run(vec![Op::Push(5), Op::Push(3), Op::Le, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_complex_expression() {
        // ((10 + 20) * 3 - 5) / 2 = (30*3-5)/2 = 85/2 = 42
        let vm = run(vec![
            Op::Push(10),
            Op::Push(20),
            Op::Add,
            Op::Push(3),
            Op::Mul,
            Op::Push(5),
            Op::Sub,
            Op::Push(2),
            Op::Div,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_store_local_and_reload() {
        // Call function, modify local, reload it
        let vm = run(vec![
            Op::Push(10),   // 0: arg
            Op::Call(3, 1), // 1
            Op::Halt,       // 2
            // func at 3:
            Op::Push(99),      // 3: new value
            Op::StoreLocal(0), // 4: overwrite arg with 99
            Op::LoadLocal(0),  // 5: load it back
            Op::Ret,           // 6
        ]);
        assert_eq!(vm.top(), Some(99));
    }

    #[test]
    fn test_invalid_local() {
        let err = run_err(vec![
            Op::Push(1),
            Op::Call(3, 1),
            Op::Halt,
            Op::LoadLocal(99),
        ]);
        assert_eq!(err, VmError::InvalidLocal(99));
    }

    #[test]
    fn test_multiple_return_values_via_heap() {
        // Function returns one value but stores extra on heap.
        let vm = run(vec![
            Op::Push(2),
            Op::HeapAlloc,   // 1: alloc 2 words
            Op::StoreReg(0), // 2: save heap addr
            // Store pair (10, 20)
            Op::LoadReg(0),
            Op::Push(0),
            Op::Push(10),
            Op::HeapStoreOffset, // heap[0] = 10
            Op::LoadReg(0),
            Op::Push(1),
            Op::Push(20),
            Op::HeapStoreOffset, // heap[1] = 20
            // Read them back and add
            Op::LoadReg(0),
            Op::Push(0),
            Op::HeapLoadOffset,
            Op::LoadReg(0),
            Op::Push(1),
            Op::HeapLoadOffset,
            Op::Add,
            Op::Halt,
        ]);
        assert_eq!(vm.top(), Some(30));
    }

    #[test]
    fn test_countdown_with_debug() {
        // Count from 3 to 1, printing each.
        let vm = run(vec![
            Op::Push(3),     // 0
            Op::StoreReg(0), // 1
            // Loop (2):
            Op::LoadReg(0),  // 2
            Op::Push(0),     // 3
            Op::Le,          // 4
            Op::JumpIf(13),  // 5
            Op::LoadReg(0),  // 6
            Op::DebugPrint,  // 7
            Op::Pop,         // 8
            Op::LoadReg(0),  // 9
            Op::Dec,         // 10
            Op::StoreReg(0), // 11
            Op::Jump(2),     // 12
            Op::Halt,        // 13
        ]);
        assert_eq!(vm.debug_output, vec![3, 2, 1]);
    }

    #[test]
    fn test_bitwise_identity() {
        // a ^ a = 0 for any a
        let vm = run(vec![Op::Push(12345), Op::Dup, Op::BitXor, Op::Halt]);
        assert_eq!(vm.top(), Some(0));
    }

    #[test]
    fn test_shl_zero() {
        let vm = run(vec![Op::Push(42), Op::Push(0), Op::Shl, Op::Halt]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_shr_zero() {
        let vm = run(vec![Op::Push(42), Op::Push(0), Op::Shr, Op::Halt]);
        assert_eq!(vm.top(), Some(42));
    }

    #[test]
    fn test_jump_to_end() {
        // Jump to exactly program.len() — should stop cleanly.
        let vm = run(vec![Op::Jump(1)]);
        assert_eq!(vm.top(), None);
    }

    #[test]
    fn test_sub_negative_result() {
        let vm = run(vec![Op::Push(3), Op::Push(10), Op::Sub, Op::Halt]);
        assert_eq!(vm.top(), Some(-7));
    }

    #[test]
    fn test_div_truncates() {
        let vm = run(vec![Op::Push(7), Op::Push(2), Op::Div, Op::Halt]);
        assert_eq!(vm.top(), Some(3));
    }

    #[test]
    fn test_rem_negative() {
        let vm = run(vec![Op::Push(-7), Op::Push(3), Op::Rem, Op::Halt]);
        assert_eq!(vm.top(), Some(-1));
    }

    #[test]
    fn test_swap_three_element_stack() {
        let vm = run(vec![
            Op::Push(1),
            Op::Push(2),
            Op::Push(3),
            Op::Swap,
            Op::Halt,
        ]);
        assert_eq!(vm.stack(), &[1, 3, 2]);
    }
}
