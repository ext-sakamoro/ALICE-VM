**English** | [日本語](README_JP.md)

# ALICE-VM

Bytecode virtual machine for the A.L.I.C.E. ecosystem. Implements a hybrid stack/register machine with memory management in pure Rust.

## Features

- **Stack Machine** — Push/pop, dup, swap, over operations with 1024-depth stack
- **Register Machine** — 16 general-purpose registers for fast local computation
- **Instruction Set** — Arithmetic, bitwise, comparison, control flow, function calls
- **Memory Management** — Heap allocation/deallocation with indexed access
- **Call Frames** — Function calls with local variables and 256-depth call stack
- **Control Flow** — Conditional/unconditional jumps, call/return semantics

## Architecture

```
Bytecode Program [Op]
  │
  ├── Stack (1024 slots)     — Operand stack
  ├── Registers (16 x i64)   — General-purpose registers
  ├── Call Stack (256 depth)  — Frame base pointers
  ├── Heap (4096 words)       — Dynamic memory
  └── IP                      — Instruction pointer
```

## Instruction Set Overview

| Category    | Instructions                              |
|-------------|-------------------------------------------|
| Stack       | Push, Pop, Dup, Swap, Over                |
| Arithmetic  | Add, Sub, Mul, Div, Rem, Neg, Abs         |
| Bitwise     | And, Or, Xor, Not, Shl, Shr              |
| Comparison  | Eq, Ne, Lt, Le, Gt, Ge                   |
| Control     | Jump, JumpIf, JumpIfZero, Call, Ret       |
| Register    | LoadReg, StoreReg                         |
| Local       | LoadLocal, StoreLocal                     |
| Heap        | HeapAlloc, HeapFree, HeapLoad, HeapStore  |

## Usage

```rust
use alice_vm::Op;

let program = vec![
    Op::Push(10),
    Op::Push(20),
    Op::Add,
];
```

## License

MIT OR Apache-2.0
