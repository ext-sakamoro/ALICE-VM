[English](README.md) | **日本語**

# ALICE-VM

A.L.I.C.E. エコシステム向けバイトコード仮想マシン。スタック/レジスタ混合マシンとメモリ管理を純Rustで実装。

## 機能

- **スタックマシン** — Push/Pop、Dup、Swap、Over操作、1024段スタック
- **レジスタマシン** — 16本の汎用レジスタによる高速ローカル計算
- **命令セット** — 算術、ビット演算、比較、制御フロー、関数呼び出し
- **メモリ管理** — ヒープの確保・解放、インデックスアクセス
- **コールフレーム** — ローカル変数付き関数呼び出し、256段コールスタック
- **制御フロー** — 条件付き/無条件ジャンプ、call/returnセマンティクス

## アーキテクチャ

```
バイトコードプログラム [Op]
  │
  ├── Stack (1024 スロット)   — オペランドスタック
  ├── Registers (16 x i64)    — 汎用レジスタ
  ├── Call Stack (256 段)     — フレームベースポインタ
  ├── Heap (4096 ワード)      — 動的メモリ
  └── IP                       — 命令ポインタ
```

## 命令セット一覧

| カテゴリ    | 命令                                      |
|-------------|-------------------------------------------|
| スタック    | Push, Pop, Dup, Swap, Over                |
| 算術        | Add, Sub, Mul, Div, Rem, Neg, Abs         |
| ビット演算  | And, Or, Xor, Not, Shl, Shr              |
| 比較        | Eq, Ne, Lt, Le, Gt, Ge                   |
| 制御        | Jump, JumpIf, JumpIfZero, Call, Ret       |
| レジスタ    | LoadReg, StoreReg                         |
| ローカル    | LoadLocal, StoreLocal                     |
| ヒープ      | HeapAlloc, HeapFree, HeapLoad, HeapStore  |

## 使用例

```rust
use alice_vm::Op;

let program = vec![
    Op::Push(10),
    Op::Push(20),
    Op::Add,
];
```

## ライセンス

MIT OR Apache-2.0
