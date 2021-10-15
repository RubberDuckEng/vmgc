#![allow(dead_code)]

use std::alloc::{alloc, Layout};
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Debug)]
enum GCError {
    OOM,
    NoSpace,
}

#[derive(Debug)]
struct Space {
    base: *mut u8,
    size: usize,
    next: *mut u8,
}

impl Space {
    fn new(size: usize) -> Result<Space, GCError> {
        unsafe {
            let layout = Layout::from_size_align_unchecked(size, 0x1000);
            let ptr = alloc(layout);
            if ptr.is_null() {
                return Err(GCError::OOM);
            }
            Ok(Space {
                base: ptr,
                size,
                next: ptr,
            })
        }
    }

    fn alloc(&mut self, size: usize) -> Result<*mut u8, GCError> {
        unsafe {
            let allocated = self.used();
            if allocated.checked_add(size).ok_or(GCError::NoSpace)? > self.size {
                return Err(GCError::NoSpace);
            }
            let result = self.next;
            self.next = result.add(size);
            Ok(result)
        }
    }

    fn used(&self) -> usize {
        unsafe { self.next.offset_from(self.base) as usize }
    }
}

#[derive(Debug)]
struct HeapCell {
    ptr: *const u8,
}

#[derive(Debug, Default)]
struct HeapInner {
    cells: Vec<HeapCell>,
}

#[derive(Debug)]
struct Heap {
    from_space: Space,
    to_space: Space,
    inner: Arc<RefCell<HeapInner>>,
}

impl Heap {
    pub fn new(size: usize) -> Result<Heap, GCError> {
        let half_size = size / 2;
        Ok(Heap {
            from_space: Space::new(half_size)?,
            to_space: Space::new(half_size)?,
            inner: Arc::new(RefCell::new(HeapInner::default())),
        })
    }

    pub fn used(&self) -> usize {
        self.from_space.used() + self.to_space.used()
    }

    pub fn collect(&self) {}

    pub fn allocate<T: Traceable>(&mut self) -> Result<GlobalHandle, GCError> {
        let size = T::size();
        // TODO: We're going to need a header.
        let ptr = self.from_space.alloc(size)?;
        T::init(ptr);
        Ok(self.alloc_handle(ptr))
    }

    fn alloc_handle(&self, ptr: *const u8) -> GlobalHandle {
        let index = {
            let mut inner = self.inner.borrow_mut();
            let index = inner.cells.len();
            inner.cells.push(HeapCell { ptr });
            index
        };
        GlobalHandle {
            inner: Arc::clone(&self.inner),
            index,
        }
    }
}

#[derive(Debug)]
struct GlobalHandle {
    inner: Arc<RefCell<HeapInner>>,
    index: usize,
}

impl Drop for GlobalHandle {
    fn drop(&mut self) {
        self.inner.borrow_mut().cells[self.index].ptr = std::ptr::null();
    }
}

trait Traceable {
    fn size() -> usize;
    fn init(ptr: *mut u8);
}

#[derive(Debug)]
struct Number {}

impl Traceable for Number {
    fn size() -> usize {
        4
    }

    fn init(ptr: *mut u8) {
        unsafe { ptr.write_bytes(0, Self::size()) }
    }
}

fn main() {
    // Allocate 2 objects
    // Hold a poitner to 1 of them.
    // Run GC see it's alive.
    // Verify one is gone.
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn smoke_test() {
        let mut heap = Heap::new(1000).unwrap();
        assert_eq!(heap.used(), 0);
        let one = heap.allocate::<Number>().unwrap();
        let two = heap.allocate::<Number>().unwrap();
        std::mem::drop(one);
        assert_eq!(heap.used(), Number::size() * 2);
        heap.collect();
        assert_eq!(heap.used(), Number::size());
        std::mem::drop(two);
    }
}
