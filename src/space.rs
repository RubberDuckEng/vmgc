use std::alloc::{alloc, dealloc, Layout};

use crate::types::*;

#[derive(Debug)]
pub struct Space {
    layout: Layout,
    base: *mut u8,
    pub size_in_bytes: usize,
    next: *mut u8,
}

impl Space {
    // FIXME: Returning GCError::NoSpace likely leaves us in an unrecoverable
    // condition, consider returning something more severe?
    pub fn new(size_in_bytes: usize) -> Result<Space, GCError> {
        // TODO: Should we allocte on a 4k boundary? Might have implications
        // for returning memory to the system.
        let layout =
            Layout::from_size_align(size_in_bytes, 0x1000).map_err(|_| GCError::NoSpace)?;
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err(GCError::OSOutOfMemory);
        }
        Ok(Space {
            layout,
            base: ptr,
            size_in_bytes,
            next: ptr,
        })
    }

    // TODO: The client should be able to specify the alignment.
    pub fn alloc(&mut self, size: usize) -> Result<*mut u8, GCError> {
        let allocated = self.used_bytes();
        if allocated.checked_add(size).ok_or(GCError::NoSpace)? > self.size_in_bytes {
            return Err(GCError::NoSpace);
        }
        let result = self.next;
        unsafe {
            self.next = result.add(size);
            result.write_bytes(0, size);
        }
        Ok(result)
    }

    pub fn used_bytes(&self) -> usize {
        unsafe { self.next.offset_from(self.base) as usize }
    }

    pub fn free_bytes(&self) -> usize {
        self.size_in_bytes - self.used_bytes()
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        unsafe {
            self.base.write_bytes(0, self.used_bytes());
            dealloc(self.base, self.layout);
        }
    }
}
