use std::convert::{From, TryInto};

use crate::object::{ObjectHeader, ObjectPtr};
use crate::types::*;

#[derive(Copy, Clone)]
#[repr(C)]
pub union TaggedPtr {
    tag: usize,
    number: isize,
    object: usize, // FIXME: Should be NonNull<T>
}

impl TaggedPtr {
    pub fn header(&self) -> Option<&mut ObjectHeader> {
        (*self).try_into().ok().map(ObjectHeader::from_object_ptr)
    }
}

impl Default for TaggedPtr {
    fn default() -> Self {
        TaggedPtr { tag: 0 }
    }
}

const TAG_MASK: usize = 0x3;
const TAG_NUMBER: usize = 0x0;
const TAG_OBJECT: usize = 0x1;
const PTR_MASK: usize = !0x3;

impl From<i32> for TaggedPtr {
    fn from(value: i32) -> TaggedPtr {
        TaggedPtr {
            number: (value as isize) << 2,
        }
    }
}

impl TryInto<i32> for TaggedPtr {
    type Error = GCError;
    fn try_into(self) -> Result<i32, GCError> {
        unsafe {
            match self.tag & TAG_MASK {
                TAG_NUMBER => Ok((self.number >> 2) as i32),
                _ => Err(GCError::TypeError),
            }
        }
    }
}

impl From<ObjectPtr> for TaggedPtr {
    fn from(ptr: ObjectPtr) -> TaggedPtr {
        unsafe {
            TaggedPtr {
                object: std::mem::transmute::<ObjectPtr, usize>(ptr) | TAG_OBJECT,
            }
        }
    }
}

impl TryInto<ObjectPtr> for TaggedPtr {
    type Error = GCError;
    fn try_into(self) -> Result<ObjectPtr, GCError> {
        unsafe {
            match self.tag & TAG_MASK {
                TAG_OBJECT => Ok(std::mem::transmute::<usize, ObjectPtr>(
                    self.object & PTR_MASK,
                )),
                _ => Err(GCError::TypeError),
            }
        }
    }
}

impl std::fmt::Debug for TaggedPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaggedPtr").finish()
    }
}
