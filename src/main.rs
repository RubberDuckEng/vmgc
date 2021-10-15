#![allow(dead_code)]

use std::alloc::{alloc, Layout};
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Debug)]
enum GCError {
    // The operating system did not provide use with memory.
    OSOutOfMemory,

    // There is no memory left in this space.
    NoSpace,

    // There is no space left in the heap to allocate this object, even after
    // collecting dead objects.
    HeapFull,
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
            // TODO: Should we allocte on a 4k boundary? Might have implications
            // for returning memory to the system.
            let layout = Layout::from_size_align_unchecked(size, 0x1000);
            let ptr = alloc(layout);
            if ptr.is_null() {
                return Err(GCError::OSOutOfMemory);
            }
            Ok(Space {
                base: ptr,
                size,
                next: ptr,
            })
        }
    }

    fn clear(&mut self) {
        // TODO: Return memory to the system.
        self.next = self.base;
    }

    // TODO: The client should be able to specify the alignment.
    fn alloc(&mut self, size: usize) -> Result<*mut u8, GCError> {
        unsafe {
            let allocated = self.used();
            if allocated.checked_add(size).ok_or(GCError::NoSpace)? > self.size {
                return Err(GCError::NoSpace);
            }
            let result = self.next;
            self.next = result.add(size);
            result.write_bytes(0, size);
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
    cells: Vec<Option<HeapCell>>,
}

#[derive(Debug)]
struct Heap {
    // TODO: Add more generations.
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

    pub fn collect(&mut self) {
        let mut inner = self.inner.borrow_mut();
        for maybe_cell in inner.cells.iter_mut() {
            if let Some(cell) = maybe_cell {
                // TODO: Get the size from the object header
                // TODO: Trace the object graph.
                let object_size = Number::size();
                let new_ptr = self.to_space.alloc(object_size).unwrap();
                unsafe {
                    std::ptr::copy_nonoverlapping(cell.ptr, new_ptr, object_size);
                }
                cell.ptr = new_ptr;
            }
        }
        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.to_space.clear();
    }

    pub fn allocate<T: Traceable>(&mut self) -> Result<GlobalHandle, GCError> {
        let size = T::size();
        // TODO: We're going to need a header.
        // TODO: If we're out of space, we should collect.
        let ptr = self.from_space.alloc(size)?;
        Ok(self.alloc_handle(ptr))
    }

    fn alloc_handle(&self, ptr: *const u8) -> GlobalHandle {
        let index = {
            // TODO: Scan for available cells.
            let mut inner = self.inner.borrow_mut();
            let index = inner.cells.len();
            inner.cells.push(Some(HeapCell { ptr }));
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
        self.inner.borrow_mut().cells[self.index] = None;
    }
}

// TODO: Add HandleScope and LocalHandle.

trait Traceable {
    fn size() -> usize;
}

#[derive(Debug)]
struct Number {}

impl Traceable for Number {
    fn size() -> usize {
        4
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
