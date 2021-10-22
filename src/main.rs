#![allow(dead_code)]

use std::alloc::{alloc, Layout};
use std::cell::RefCell;
use std::sync::Arc;

// Things to do
// 1. Make it possible to have different sized objects. (Object header)
// 2. Make a second object type (not number, maybe string?)
// 3. Make a host object with finalizer (start with weak pointer?)
// 4. Make it possible to trace objects.

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

const HEADER_SIZE: usize = std::mem::size_of::<ObjectHeader>();

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
                let header_ptr = unsafe { cell.ptr.sub(HEADER_SIZE) };
                let header = unsafe { &*(header_ptr as *mut ObjectHeader) };
                // TODO: Trace the object graph.
                let new_ptr = self
                    .to_space
                    .alloc(HEADER_SIZE + header.object_size)
                    .unwrap();
                unsafe {
                    std::ptr::copy_nonoverlapping(cell.ptr, new_ptr, header.object_size);
                }
                cell.ptr = unsafe { new_ptr.add(HEADER_SIZE) };
            }
        }
        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.to_space.clear();
    }

    pub fn allocate<T>(&mut self) -> Result<GlobalHandle, GCError> {
        let object_size = std::mem::size_of::<T>();
        let alloc_size = HEADER_SIZE + object_size;
        let ptr = self.from_space.alloc(alloc_size)?;
        let mut header = unsafe { &mut *(ptr as *mut ObjectHeader) };
        header.object_size = object_size;
        Ok(self.alloc_handle(unsafe { ptr.add(HEADER_SIZE) }))
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

#[derive(Debug)]
#[repr(C)]
struct ObjectHeader {
    object_size: usize,
}

#[derive(Debug)]
#[repr(C)]
struct Number {
    value: u64,
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
        assert_eq!(
            heap.used(),
            (HEADER_SIZE + std::mem::size_of::<Number>()) * 2
        );
        heap.collect();
        assert_eq!(heap.used(), HEADER_SIZE + std::mem::size_of::<Number>());
        std::mem::drop(two);
    }
}
