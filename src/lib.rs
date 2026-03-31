use std::alloc::{self, Layout};
use std::ptr::NonNull;

pub struct Arena {
    memory: NonNull<u8>,
    size: usize,
    offset: usize,
}

impl Arena {
    /// Create a new arena with the specified size in bytes
    pub fn new(size: usize) -> Result<Self, ArenaError> {
        if size == 0 {
            return Err(ArenaError::InvalidSize);
        }

        let layout = Layout::from_size_align(size, 8)
            .map_err(|_| ArenaError::InvalidAlignment)?;

        let memory = unsafe {
            let ptr = alloc::alloc(layout);
            if ptr.is_null() {
                return Err(ArenaError::AllocationFailed);
            }
            NonNull::new_unchecked(ptr)
        };

        Ok(Arena {
            memory,
            size,
            offset: 0,
        })
    }

    /// Allocate space for a single object of type T, initialized with `value`.
    ///
    /// This is the safe entry point: the value is written into the arena before
    /// a reference is returned, so the returned `&mut T` always points to
    /// initialized memory.
    pub fn alloc<T>(&mut self, value: T) -> Result<&mut T, ArenaError> {
        let ptr = self.alloc_layout(Layout::new::<T>())?;

        unsafe {
            let typed_ptr = ptr.as_ptr() as *mut T;
            // Write the value, transferring ownership into the arena.
            // No destructor will be called for it when the arena resets or drops
            // (intentional — see crate docs).
            typed_ptr.write(value);
            Ok(&mut *typed_ptr)
        }
    }

    /// Allocate space for an array of `count` elements, each initialized by
    /// calling `init(index)`.
    ///
    /// Using a closure lets callers initialize elements with their index (or any
    /// other logic) without requiring `T: Clone` or `T: Default`.
    ///
    /// ```
    /// # use arena_rs::Arena;
    /// let mut arena = Arena::new(256).unwrap();
    /// let squares = arena.alloc_array(4, |i| (i * i) as u32).unwrap();
    /// assert_eq!(squares, [0, 1, 4, 9]);
    /// ```
    pub fn alloc_array<T, F>(&mut self, count: usize, mut init: F) -> Result<&mut [T], ArenaError>
    where
        F: FnMut(usize) -> T,
    {
        if count == 0 {
            return Ok(&mut []);
        }

        let layout = Layout::array::<T>(count)
            .map_err(|_| ArenaError::InvalidSize)?;
        let ptr = self.alloc_layout(layout)?;

        unsafe {
            let base = ptr.as_ptr() as *mut T;
            // Initialize each slot individually before we hand out the slice.
            for i in 0..count {
                base.add(i).write(init(i));
            }
            Ok(std::slice::from_raw_parts_mut(base, count))
        }
    }

    /// Low-level allocation based on layout.
    fn alloc_layout(&mut self, layout: Layout) -> Result<NonNull<u8>, ArenaError> {
        let size = layout.size();
        let align = layout.align();

        let aligned_offset = (self.offset + align - 1) & !(align - 1);

        if aligned_offset + size > self.size {
            return Err(ArenaError::OutOfMemory);
        }

        unsafe {
            let ptr = self.memory.as_ptr().add(aligned_offset);
            self.offset = aligned_offset + size;
            Ok(NonNull::new_unchecked(ptr))
        }
    }

    /// Reset the arena (doesn't deallocate, just resets the offset).
    ///
    /// # Safety note
    /// All references handed out by `alloc` and `alloc_array` are invalidated
    /// after this call. Rust's borrow checker cannot enforce this because the
    /// arena owns the backing memory independently of the returned references.
    /// Callers must ensure no live references remain before calling `reset`.
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Remaining space in bytes.
    pub fn remaining(&self) -> usize {
        self.size - self.offset
    }

    /// Used space in bytes.
    pub fn used(&self) -> usize {
        self.offset
    }

    /// Total capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.size
    }
}

impl std::fmt::Debug for Arena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Arena {{ used: {}, capacity: {} }}", self.used(), self.capacity())
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.size, 8);
            alloc::dealloc(self.memory.as_ptr(), layout);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArenaError {
    InvalidSize,
    InvalidAlignment,
    AllocationFailed,
    OutOfMemory,
}

impl std::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArenaError::InvalidSize => write!(f, "Invalid size specified"),
            ArenaError::InvalidAlignment => write!(f, "Invalid alignment"),
            ArenaError::AllocationFailed => write!(f, "Failed to allocate memory"),
            ArenaError::OutOfMemory => write!(f, "Arena out of memory"),
        }
    }
}

impl std::error::Error for ArenaError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[test]
    fn test_alloc_single() {
        let mut arena = Arena::new(1024).unwrap();

        let n = arena.alloc(42u32).unwrap();
        assert_eq!(*n, 42);

        let p = arena.alloc(Point { x: 1.0, y: 2.0 }).unwrap();
        assert_eq!(*p, Point { x: 1.0, y: 2.0 });
    }

    #[test]
    fn test_alloc_array_with_init() {
        let mut arena = Arena::new(1024).unwrap();

        // Initialize each element using its index
        let squares = arena.alloc_array(5, |i| (i * i) as u32).unwrap();
        assert_eq!(squares, [0, 1, 4, 9, 16]);
    }

    #[test]
    fn test_alloc_array_uniform_value() {
        let mut arena = Arena::new(1024).unwrap();

        // Uniform init: ignore the index, always return the same value
        let zeros = arena.alloc_array(4, |_| 0u64).unwrap();
        assert_eq!(zeros, [0, 0, 0, 0]);
    }

    #[test]
    fn test_million_objects() {
        const COUNT: usize = 1_000_000;
        let mut arena = Arena::new(COUNT * size_of::<u64>()).unwrap();

        let values = arena.alloc_array(COUNT, |i| i as u64).unwrap();

        for (i, &v) in values.iter().enumerate() {
            assert_eq!(v, i as u64);
        }

        println!("{:?}", arena);
    }

    #[test]
    fn test_mixed_allocations() {
        let mut arena = Arena::new(1024).unwrap();

        let a = arena.alloc(10u64).unwrap();
        assert_eq!(*a, 10);

        let b = arena.alloc_array(3, |i| i as u32).unwrap();
        assert_eq!(b, [0, 1, 2]);

        let c = arena.alloc(Point { x: 3.0, y: 4.0 }).unwrap();
        assert_eq!(c.x, 3.0);
    }

    #[test]
    fn test_out_of_memory() {
        let mut arena = Arena::new(8).unwrap();
        arena.alloc(0u64).unwrap();
        assert_eq!(arena.alloc(0u64), Err(ArenaError::OutOfMemory));
    }

    #[test]
    fn test_reset_reuses_memory() {
        let mut arena = Arena::new(64).unwrap();

        {
            let _x = arena.alloc(1u32).unwrap();
            assert!(arena.used() > 0);
        }

        arena.reset();
        assert_eq!(arena.used(), 0);

        // Can allocate again after reset
        let y = arena.alloc(99u32).unwrap();
        assert_eq!(*y, 99);
    }

    #[test]
    fn test_debug_format() {
        let arena = Arena::new(512).unwrap();
        let s = format!("{:?}", arena);
        assert_eq!(s, "Arena { used: 0, capacity: 512 }");
    }

    #[test]
    fn test_zero_size_arena_fails() {
        assert!(matches!(Arena::new(0), Err(ArenaError::InvalidSize)));
    }

    #[test]
    fn test_empty_array_alloc() {
        let mut arena = Arena::new(64).unwrap();
        let empty = arena.alloc_array::<u32, _>(0, |_| 0).unwrap();
        assert!(empty.is_empty());
        assert_eq!(arena.used(), 0); // nothing consumed
    }
}