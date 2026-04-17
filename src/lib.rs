#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::alloc::{self as allocator, Layout};
use core::ptr::NonNull;

pub mod typed_arena;
pub use typed_arena::TypedArena;

pub struct Arena {
    memory: NonNull<u8>,
    size: usize,
    offset: usize,
}

/// A reference to a value allocated in an [`Arena`].
///
/// The lifetime `'arena` is tied to the arena that owns the backing memory,
/// so the borrow checker statically prevents:
/// - using the reference after the arena is dropped
/// - calling [`Arena::reset`] while any `ArenaRef` is still live
pub struct ArenaRef<'arena, T> {
    inner: &'arena mut T,
}

impl<'arena, T> core::ops::Deref for ArenaRef<'arena, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner
    }
}

impl<'arena, T> core::ops::DerefMut for ArenaRef<'arena, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner
    }
}

impl<'arena, T: core::fmt::Debug> core::fmt::Debug for ArenaRef<'arena, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
    }
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
            let ptr = allocator::alloc(layout);
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
    /// Returns an [`ArenaRef`] whose lifetime is bound to the arena, so the
    /// borrow checker prevents both use-after-drop and calling [`reset`] while
    /// the reference is live.
    pub fn alloc<T>(&mut self, value: T) -> Result<ArenaRef<'_, T>, ArenaError> {
        let ptr = self.alloc_layout(Layout::new::<T>())?;

        unsafe {
            let typed_ptr = ptr.as_ptr() as *mut T;
            typed_ptr.write(value);
            Ok(ArenaRef { inner: &mut *typed_ptr })
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
            Ok(core::slice::from_raw_parts_mut(base, count))
        }
    }

    /// Allocate space for a single object of type T without initializing it.
    ///
    /// Returns an [`ArenaRef`] wrapping `MaybeUninit<T>`. The caller **must**
    /// initialize the value before calling `.assume_init_mut()` or reading
    /// through the reference.
    ///
    /// Prefer [`alloc`] unless you have a specific performance reason to skip
    /// initialization.
    pub fn alloc_uninit<T>(&mut self) -> Result<ArenaRef<'_, core::mem::MaybeUninit<T>>, ArenaError> {
        let ptr = self.alloc_layout(Layout::new::<T>())?;

        unsafe {
            let typed_ptr = ptr.as_ptr() as *mut core::mem::MaybeUninit<T>;
            Ok(ArenaRef { inner: &mut *typed_ptr })
        }
    }

    /// Allocate space for an array of `count` elements without initializing them.
    ///
    /// Returns `&mut [MaybeUninit<T>]`. The caller **must** initialize every
    /// element before reading from the slice.
    ///
    /// Prefer [`alloc_array`] unless you have a specific performance reason to
    /// skip initialization.
    pub fn alloc_array_uninit<T>(
        &mut self,
        count: usize,
    ) -> Result<&mut [core::mem::MaybeUninit<T>], ArenaError> {
        if count == 0 {
            return Ok(&mut []);
        }

        let layout = Layout::array::<T>(count)
            .map_err(|_| ArenaError::InvalidSize)?;
        let ptr = self.alloc_layout(layout)?;

        unsafe {
            let base = ptr.as_ptr() as *mut core::mem::MaybeUninit<T>;
            Ok(core::slice::from_raw_parts_mut(base, count))
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

impl core::fmt::Debug for Arena {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Arena {{ used: {}, capacity: {} }}", self.used(), self.capacity())
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.size, 8);
            allocator::dealloc(self.memory.as_ptr(), layout);
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

impl core::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArenaError::InvalidSize => write!(f, "Invalid size specified"),
            ArenaError::InvalidAlignment => write!(f, "Invalid alignment"),
            ArenaError::AllocationFailed => write!(f, "Failed to allocate memory"),
            ArenaError::OutOfMemory => write!(f, "Arena out of memory"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ArenaError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

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
        assert!(matches!(arena.alloc(0u64), Err(ArenaError::OutOfMemory)));
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
    fn test_arena_ref_deref() {
        let mut arena = Arena::new(1024).unwrap();
        let mut r = arena.alloc(10u32).unwrap();
        assert_eq!(*r, 10);
        *r = 20;
        assert_eq!(*r, 20);
    }

    #[test]
    fn test_arena_ref_debug() {
        let mut arena = Arena::new(1024).unwrap();
        let r = arena.alloc(42u32).unwrap();
        assert_eq!(format!("{:?}", r), "42");
    }

    #[test]
    fn test_reset_allowed_after_ref_dropped() {
        let mut arena = Arena::new(1024).unwrap();
        {
            let _r = arena.alloc(1u32).unwrap();
            // cannot call arena.reset() here — borrow checker prevents it
        } // _r dropped here, borrow released
        arena.reset(); // fine now
        assert_eq!(arena.used(), 0);
    }

    #[test]
    fn test_alloc_uninit_single() {
        let mut arena = Arena::new(1024).unwrap();

        let mut slot = arena.alloc_uninit::<u64>().unwrap();
        slot.write(77);
        let val = unsafe { slot.assume_init_mut() };
        assert_eq!(*val, 77);
    }

    #[test]
    fn test_alloc_array_uninit() {
        let mut arena = Arena::new(1024).unwrap();

        let slots = arena.alloc_array_uninit::<u32>(4).unwrap();
        for (i, slot) in slots.iter_mut().enumerate() {
            slot.write(i as u32 * 10);
        }
        let vals: &[u32] = unsafe { core::mem::transmute(&*slots) };
        assert_eq!(vals, [0, 10, 20, 30]);
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