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

    /// Allocate space for a single object of type T
    pub fn alloc<T>(&mut self) -> Result<&mut T, ArenaError> {
        let layout = Layout::new::<T>();
        let ptr = self.alloc_layout(layout)?;

        unsafe {
            let typed_ptr = ptr.as_ptr() as *mut T;
            Ok(&mut *typed_ptr)
        }
    }

    /// Allocate space for an array of objects of type T
    pub fn alloc_array<T>(&mut self, count: usize) -> Result<&mut [T], ArenaError> {
        if count == 0 {
            return Ok(&mut []);
        }

        let layout = Layout::array::<T>(count)
            .map_err(|_| ArenaError::InvalidSize)?;
        let ptr = self.alloc_layout(layout)?;

        unsafe {
            let typed_ptr = ptr.as_ptr() as *mut T;
            Ok(std::slice::from_raw_parts_mut(typed_ptr, count))
        }
    }

    /// Low-level allocation based on layout
    fn alloc_layout(&mut self, layout: Layout) -> Result<NonNull<u8>, ArenaError> {
        let size = layout.size();
        let align = layout.align();

        // Align the current offset to the required alignment
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

    /// Reset the arena (doesn't deallocate, just resets the offset)
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Get remaining space in bytes
    pub fn remaining(&self) -> usize {
        self.size - self.offset
    }

    /// Get used space in bytes
    pub fn used(&self) -> usize {
        self.offset
    }

    /// Get total capacity in bytes
    pub fn capacity(&self) -> usize {
        self.size
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

// Example usage and test
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct TestObject {
        id: u64,
        value: u64,
    }

    #[test]
    fn test_arena_basic() {
        let mut arena = Arena::new(1024).unwrap();

        // Allocate a single object
        let obj = arena.alloc::<TestObject>().unwrap();
        obj.id = 42;
        obj.value = 100;

        assert_eq!(obj.id, 42);
        assert_eq!(obj.value, 100);
    }

    #[test]
    fn test_arena_million_objects() {
        const COUNT: usize = 1_000_000;
        const OBJECT_SIZE: usize = 16; // TestObject is 16 bytes
        const TOTAL_SIZE: usize = COUNT * OBJECT_SIZE;

        let mut arena = Arena::new(TOTAL_SIZE).unwrap();

        // Allocate array of 1 million objects
        let objects = arena.alloc_array::<TestObject>(COUNT).unwrap();

        // Initialize the objects
        for (i, obj) in objects.iter_mut().enumerate() {
            obj.id = i as u64;
            obj.value = (i * 2) as u64;
        }

        // Verify the objects
        for (i, obj) in objects.iter().enumerate() {
            assert_eq!(obj.id, i as u64);
            assert_eq!(obj.value, (i * 2) as u64);
        }

        println!("Successfully allocated and initialized {} objects", COUNT);
        println!("Used {} bytes of {} bytes", arena.used(), arena.capacity());
    }

    #[test]
    fn test_arena_mixed_allocations() {
        let mut arena = Arena::new(1024).unwrap();

        // Mix different types of allocations
        let single = arena.alloc::<u64>().unwrap();
        *single = 42;
        assert_eq!(*single, 42);

        let array = arena.alloc_array::<u32>(10).unwrap();
        for (i, val) in array.iter_mut().enumerate() {
            *val = i as u32;
        }
        assert_eq!(array[5], 5);

        let another = arena.alloc::<TestObject>().unwrap();
        another.id = 999;
        another.value = 888;
        
        assert_eq!(another.id, 999);
    }
}

