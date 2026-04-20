# arena_rs

`arena_rs` is a simple and safe arena-based memory allocator written in Rust.

A fixed-size buffer is allocated up front, and objects are placed into it linearly via a bump pointer. All allocated objects are cleared at once via a `reset()` call — highly efficient for batch-style workloads where you don't need to free objects individually.

Inspired by the book [`C++ Memory Management`](https://www.amazon.com/Memory-Management-leaner-memory-management-techniques/dp/1805129805) by Patrice Roy.

## Features

- ✅ `no_std` compatible (requires `alloc`)
- ✅ Safe API built on top of `unsafe` internals
- ✅ Generic: supports allocating any type
- ✅ Fast: bump-pointer allocation, reset is O(1)
- ✅ `ArenaRef<T>`: lifetime-tied references that prevent use-after-reset at compile time
- ✅ `TypedArena<T>`: type-specialized arena with proper `Drop` support
- ✅ Benchmarks via Criterion

## Crate Features

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | ✅ yes  | Implements `std::error::Error` for `ArenaError`. Disable for `no_std` environments. |

## When to use which arena

| | `Arena` | `TypedArena<T>` |
|---|---|---|
| Multiple types | ✅ | ❌ single type only |
| Calls `Drop` on reset | ❌ | ✅ |
| Overhead | minimal | tracks object count |
| Good for | plain data, mixed types | `String`, `Vec`, any resource-owning type |

## Usage

### `Arena` — fast bump allocator

```rust
use arena_rs::Arena;

let mut arena = Arena::new(1024).unwrap(); // 1 KB buffer

// Single object — initialized before the reference is returned
let number = arena.alloc(42i32).unwrap();
assert_eq!(*number, 42);

// Array — each element initialized via closure
let squares = arena.alloc_array(4, |i| (i * i) as u32).unwrap();
assert_eq!(squares, [0, 1, 4, 9]);

arena.reset(); // O(1) — no destructors run
```

### `TypedArena<T>` — arena with `Drop` support

```rust
use arena_rs::TypedArena;

let mut arena = TypedArena::<String>::new(16).unwrap();

let s = arena.alloc("hello".to_string()).unwrap();
assert_eq!(*s, "hello");

arena.reset(); // drop_in_place called on every live String — no leaks
```

### Heterogeneous types via enum

`TypedArena` allocates a single type, but you can use an enum to store multiple
types in one arena — each variant's `Drop` is called correctly on reset:

```rust
use arena_rs::TypedArena;

enum GameObject {
    Player(String),
    Enemy { hp: u32 },
}

let mut arena = TypedArena::<GameObject>::new(100).unwrap();
arena.alloc(GameObject::Player("hero".to_string())).unwrap();
arena.alloc(GameObject::Enemy { hp: 50 }).unwrap();

arena.reset(); // drops all variants correctly
```

### Uninitialized allocation (advanced)

```rust
use arena_rs::Arena;

let mut arena = Arena::new(64).unwrap();

let mut slot = arena.alloc_uninit::<u64>().unwrap();
slot.write(99);
let val = unsafe { slot.assume_init_mut() };
assert_eq!(*val, 99);
```

## Compile-time reset safety

`alloc` returns `ArenaRef<'_, T>`, whose lifetime is tied to the arena.
Calling `reset()` while a reference is live is a **compile error**:

```rust
let mut arena = Arena::new(64).unwrap();
let r = arena.alloc(1u32).unwrap();
arena.reset(); // error: cannot borrow `arena` as mutable
               //        because it is also borrowed as immutable
drop(r);
arena.reset(); // fine
```

## Running benchmarks

```bash
cargo bench
```

HTML reports are written to `target/criterion/report/index.html`.

## Planned improvements

- ⬜ Thread-safe/concurrent arena allocator
- ⬜ Crate-level `#![deny(unsafe_op_in_unsafe_fn)]` audit