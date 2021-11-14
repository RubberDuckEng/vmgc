use std::convert::{From, TryInto};

use crate::object::{ObjectHeader, ObjectPtr};
use crate::types::*;

#[derive(Copy, Clone)]
#[repr(C)]
pub union TaggedNum {
    number: f64,
    bits: usize,
}

const SIGN_MASK: usize = 1 << 63;
const QUIET_NAN_MASK: usize = 0x7ffc000000000000;
// If sign and quiet nan are set, this is a pointer.
const PTR_TAG_MASK: usize = SIGN_MASK | QUIET_NAN_MASK;
// The rest of the bits are the poitner.
const PTR_MASK: usize = !PTR_TAG_MASK;

// Used for identifying singletons.  All singletons have quiet nan bits set.
const SINGLETON_TAG_MASK: usize = 7;

const TAG_NAN: usize = 0;
const TAG_NULL: usize = 1;
const TAG_FALSE: usize = 2;
const TAG_TRUE: usize = 3;
// const TAG_UNUSED: usize = 4;
// const TAG_UNUSED2: usize = 5;
// const TAG_UNUSED3: usize = 6;
// const TAG_UNUSED4: usize = 7;

impl TaggedNum {
    const NULL: TaggedNum = TaggedNum {
        bits: QUIET_NAN_MASK | TAG_NULL,
    };
    const FALSE: TaggedNum = TaggedNum {
        bits: QUIET_NAN_MASK | TAG_FALSE,
    };
    const TRUE: TaggedNum = TaggedNum {
        bits: QUIET_NAN_MASK | TAG_TRUE,
    };

    // It's a number if it's not NaN.
    fn is_num(&self) -> bool {
        unsafe { (self.bits & QUIET_NAN_MASK) != QUIET_NAN_MASK }
    }

    // It's an object if object mask is set.
    fn is_ptr(&self) -> bool {
        unsafe { (self.bits & PTR_TAG_MASK) == PTR_TAG_MASK }
    }

    fn is_false(&self) -> bool {
        unsafe { self.bits == TaggedNum::FALSE.bits }
    }
    fn is_null(&self) -> bool {
        unsafe { self.bits == TaggedNum::NULL.bits }
    }

    // Should this be From/Into?
    fn as_bool(&self) -> bool {
        unsafe { self.bits == TaggedNum::TRUE.bits }
    }

    fn singleton_tag(&self) -> usize {
        unsafe { self.bits & SINGLETON_TAG_MASK }
    }

    pub fn header(&self) -> Option<&mut ObjectHeader> {
        (*self).try_into().ok().map(ObjectHeader::from_object_ptr)
    }
}

impl Default for TaggedNum {
    fn default() -> Self {
        TaggedNum::NULL
    }
}

impl From<f64> for TaggedNum {
    fn from(value: f64) -> TaggedNum {
        TaggedNum { number: value }
    }
}

impl TryInto<f64> for TaggedNum {
    type Error = GCError;
    fn try_into(self) -> Result<f64, GCError> {
        unsafe {
            if self.is_num() {
                Ok(self.number)
            } else {
                Err(GCError::TypeError)
            }
        }
    }
}

// impl From<bool> for TaggedNum {
//     fn from(value: bool) -> TaggedNum {
//         TaggedNum { number: value }
//     }
// }

// impl From<TaggedNum> for bool {
//     fn from(item: TaggedNum) -> Self {
//         item.as_bool()
//     }
// }

impl From<ObjectPtr> for TaggedNum {
    fn from(ptr: ObjectPtr) -> TaggedNum {
        unsafe {
            TaggedNum {
                bits: std::mem::transmute::<ObjectPtr, usize>(ptr) | PTR_TAG_MASK,
            }
        }
    }
}

impl TryInto<ObjectPtr> for TaggedNum {
    type Error = GCError;
    fn try_into(self) -> Result<ObjectPtr, GCError> {
        unsafe {
            if self.is_ptr() {
                Ok(std::mem::transmute::<usize, ObjectPtr>(
                    self.bits & PTR_MASK,
                ))
            } else {
                Err(GCError::TypeError)
            }
        }
    }
}

impl std::fmt::Debug for TaggedNum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaggedNum").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn size_test() {
        // This should be compile time instead:
        // https://github.com/rust-lang/rfcs/issues/2790
        assert_eq!(std::mem::size_of::<TaggedNum>(), 8);
    }

    #[test]
    pub fn null_test() {
        assert!(TaggedNum::default().is_null());
        let zero: TaggedNum = 0.0.into();
        assert!(!zero.is_null());
    }
}
