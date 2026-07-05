//! The bytecode virtual machine.

use crate::constants::{MAX_CALL_DEPTH, MAX_STACK, NUM_REGISTERS};
use crate::error::VmError;
use crate::frame::CallFrame;
use crate::heap::Heap;
use crate::op::Op;

/// The ALICE bytecode virtual machine.
#[derive(Debug)]
pub struct Vm {
    program: Vec<Op>,
    pc: usize,
    stack: Vec<i64>,
    registers: [i64; NUM_REGISTERS],
    call_stack: Vec<CallFrame>,
    heap: Heap,
    pub debug_output: Vec<i64>,
    halted: bool,
}

impl Vm {
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

    #[must_use]
    pub fn top(&self) -> Option<i64> {
        self.stack.last().copied()
    }

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

    #[must_use]
    pub const fn is_halted(&self) -> bool {
        self.halted
    }

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
                self.push(a >> shift)?;
            }

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
                self.stack.truncate(frame.base_ptr);
                self.push(ret_val)?;
                self.pc = frame.return_addr;
            }

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
