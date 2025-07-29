# arena_rs

`arena_rs` is a simple and safe arena-based memory allocator written in Rust.

This project demonstrates a basic and simple memory management technique where a fixed-size buffer is allocated up front, and objects are linearly placed in it. All allocated objects are cleared at once via a `reset()` call, which is highly efficient when you want fast allocation without tracking or freeing each object individually.

I started this project, after I finished reading the amazing book [`C++ Memory Management`](https://www.amazon.com/Memory-Management-leaner-memory-management-techniques/dp/1805129805) by Patrice Roy. Inspired by the ideas in that book, I decided to practice implementing similar memory management techniques in Rust. 😊

## Features

- ✅ Safe API built on top of `unsafe` internals.
- ✅ Generic: supports allocating different types.
- ✅ Fast: allocation is bump-pointer-based, and deallocation is a no-op.
- ✅ Ideal for batch-style memory usage patterns.

## Current Behavior

The arena does **not** call destructors (`Drop`) of allocated objects on `reset()`. This is intentional for performance and simplicity — suitable for plain data or types that don’t manage external resources.

## Usage Example

```rust
use arena_rs::Arena;

let mut arena = Arena::new(1024).unwrap(); // 1 KB buffer

let number = arena.alloc::<i32>().unwrap();
unsafe {
    std::ptr::write(number, 42);
}

println!("Allocated number: {}", unsafe { *number });

arena.reset(); // Fast deallocation (no destructors run)
```


## Planned Improvements
- ⬜ Add benchmarks comparing arena allocation against regular memory allocation methods
- ⬜ Support for type-specialized arenas with proper `Drop` handling  
- ⬜ Thread-safe/concurrent arena allocator