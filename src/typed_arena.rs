use alloc::alloc::{self as allocator, Layout};
use core::ptr::{self, NonNull};

use crate::ArenaError;

/// A type-specialized arena allocator that properly calls destructors.
///
/// Unlike [`Arena`], which is a raw bump allocator that never runs `Drop`,
/// `TypedArena<T>` tracks every allocated `T` and calls `drop_in_place` on
/// each one when [`reset`] is called or the arena itself is dropped.
///
/// This makes it safe to allocate types that own heap resources (e.g.
/// `String`, `Vec<T>`, `Box<T>`).
///
/// # Example
/// ```
/// use arenars::TypedArena;
///
/// let mut arena = TypedArena::<String>::new(16).unwrap();
///
/// let s = arena.alloc("hello".to_string()).unwrap();
/// assert_eq!(*s, "hello");
///
/// arena.reset(); // drop_in_place called on the String — no leak
/// ```
///
/// [`Arena`]: crate::Arena
/// [`reset`]: TypedArena::reset
pub struct TypedArena<T> {
    memory: NonNull<T>,
    capacity: usize, // in number of T's, not bytes
    count: usize,    // number of live T's
}

impl<T> TypedArena<T> {
    /// Create a new `TypedArena` that can hold up to `capacity` objects of
    /// type `T`.
    pub fn new(capacity: usize) -> Result<Self, ArenaError> {
        if capacity == 0 {
            return Err(ArenaError::InvalidSize);
        }

        let layout = Layout::array::<T>(capacity)
            .map_err(|_| ArenaError::InvalidSize)?;

        let memory = unsafe {
            let ptr = allocator::alloc(layout) as *mut T;
            if ptr.is_null() {
                return Err(ArenaError::AllocationFailed);
            }
            NonNull::new_unchecked(ptr)
        };

        Ok(Self {
            memory,
            capacity,
            count: 0,
        })
    }

    /// Allocate a single `T`, initialized with `value`.
    ///
    /// Returns an `&mut T` whose lifetime is tied to `&mut self`, so the
    /// borrow checker prevents calling [`reset`] while the reference is live.
    ///
    /// [`reset`]: TypedArena::reset
    pub fn alloc(&mut self, value: T) -> Result<&mut T, ArenaError> {
        if self.count == self.capacity {
            return Err(ArenaError::OutOfMemory);
        }

        unsafe {
            let slot = self.memory.as_ptr().add(self.count);
            ptr::write(slot, value);
            self.count += 1;
            Ok(&mut *slot)
        }
    }

    /// Drop all allocated objects and reset the arena for reuse.
    ///
    /// Calls `drop_in_place` on every live `T` in allocation order, then
    /// resets the count to zero. The backing memory is retained.
    pub fn reset(&mut self) {
        unsafe {
            let base = self.memory.as_ptr();
            for i in 0..self.count {
                ptr::drop_in_place(base.add(i));
            }
        }
        self.count = 0;
    }

    /// Number of objects currently allocated.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Returns `true` if no objects are currently allocated.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Maximum number of objects this arena can hold.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Remaining slots available.
    pub fn remaining(&self) -> usize {
        self.capacity - self.count
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for TypedArena<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "TypedArena {{ len: {}, capacity: {} }}",
            self.count, self.capacity
        )
    }
}

impl<T> Drop for TypedArena<T> {
    fn drop(&mut self) {
        // Drop all live objects first, then free the backing memory.
        self.reset();

        unsafe {
            // capacity == 0 is prevented by new(), but guard anyway
            if self.capacity > 0 {
                let layout = Layout::array::<T>(self.capacity)
                    .expect("layout valid: same params used in new()");
                allocator::dealloc(self.memory.as_ptr() as *mut u8, layout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::format;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use alloc::sync::Arc;

    // Counts how many instances are currently live via a shared atomic.
    #[derive(Debug)]
    struct DropCounter {
        live: Arc<AtomicUsize>,
    }

    impl DropCounter {
        fn new(live: &Arc<AtomicUsize>) -> Self {
            live.fetch_add(1, Ordering::SeqCst);
            Self { live: Arc::clone(live) }
        }
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.live.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_alloc_and_read() {
        let mut arena = TypedArena::<String>::new(4).unwrap();
        let s = arena.alloc("hello".to_string()).unwrap();
        assert_eq!(*s, "hello");
    }

    #[test]
    fn test_reset_calls_drop() {
        let live = Arc::new(AtomicUsize::new(0));
        let mut arena = TypedArena::new(4).unwrap();

        arena.alloc(DropCounter::new(&live)).unwrap();
        arena.alloc(DropCounter::new(&live)).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 2);

        arena.reset();
        assert_eq!(live.load(Ordering::SeqCst), 0); // both dropped
    }

    #[test]
    fn test_drop_calls_reset() {
        let live = Arc::new(AtomicUsize::new(0));
        {
            let mut arena = TypedArena::new(4).unwrap();
            arena.alloc(DropCounter::new(&live)).unwrap();
            arena.alloc(DropCounter::new(&live)).unwrap();
            assert_eq!(live.load(Ordering::SeqCst), 2);
        } // arena dropped here
        assert_eq!(live.load(Ordering::SeqCst), 0); // both dropped
    }

    #[test]
    fn test_reset_reuse() {
        let mut arena = TypedArena::<String>::new(2).unwrap();

        arena.alloc("first".to_string()).unwrap();
        arena.alloc("second".to_string()).unwrap();
        assert_eq!(arena.len(), 2);

        arena.reset();
        assert_eq!(arena.len(), 0);

        // Slots reused — no OOM
        let s = arena.alloc("third".to_string()).unwrap();
        assert_eq!(*s, "third");
    }

    #[test]
    fn test_out_of_capacity() {
        let mut arena = TypedArena::<u32>::new(2).unwrap();
        arena.alloc(1).unwrap();
        arena.alloc(2).unwrap();
        assert!(matches!(arena.alloc(3), Err(ArenaError::OutOfMemory)));
    }

    #[test]
    fn test_zero_capacity_fails() {
        assert!(matches!(TypedArena::<u32>::new(0), Err(ArenaError::InvalidSize)));
    }

    #[test]
    fn test_debug_format() {
        let mut arena = TypedArena::<u32>::new(8).unwrap();
        arena.alloc(1).unwrap();
        assert_eq!(format!("{:?}", arena), "TypedArena { len: 1, capacity: 8 }");
    }
}