use std::convert::{From, TryFrom, TryInto};
use std::hash::{Hash, Hasher};

use crate::object::TraceableObject;
use crate::space::Space;
use crate::types::*;

// f64.from_bits and f64.to_bits exist, it might be
// possible to do this without unsafe and w/o a union.
#[derive(Copy, Clone)]
#[repr(C)]
pub union TaggedPtr {
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
// const SINGLETON_TAG_MASK: usize = 7;

// const TAG_NAN: usize = 0;
const TAG_NULL: usize = 1;
const TAG_FALSE: usize = 2;
const TAG_TRUE: usize = 3;
// const TAG_UNUSED: usize = 4;
// const TAG_UNUSED2: usize = 5;
// const TAG_UNUSED3: usize = 6;
// const TAG_UNUSED4: usize = 7;

impl TaggedPtr {
    pub const NULL: TaggedPtr = TaggedPtr {
        bits: QUIET_NAN_MASK | TAG_NULL,
    };
    pub const FALSE: TaggedPtr = TaggedPtr {
        bits: QUIET_NAN_MASK | TAG_FALSE,
    };
    pub const TRUE: TaggedPtr = TaggedPtr {
        bits: QUIET_NAN_MASK | TAG_TRUE,
    };

    // It's a number if it's not NaN.
    pub fn is_num(&self) -> bool {
        unsafe { (self.bits & QUIET_NAN_MASK) != QUIET_NAN_MASK }
    }

    // It's an object if object mask is set.
    fn is_ptr(&self) -> bool {
        unsafe { (self.bits & PTR_TAG_MASK) == PTR_TAG_MASK }
    }

    fn is_true_singleton(&self) -> bool {
        unsafe { self.bits == TaggedPtr::TRUE.bits }
    }

    fn is_false_singleton(&self) -> bool {
        unsafe { self.bits == TaggedPtr::FALSE.bits }
    }

    pub fn is_null(&self) -> bool {
        unsafe { self.bits == TaggedPtr::NULL.bits }
    }

    // fn singleton_tag(&self) -> usize {
    //     unsafe { self.bits & SINGLETON_TAG_MASK }
    // }

    pub fn header(&self) -> Option<&mut ObjectHeader> {
        (*self).try_into().ok().map(ObjectHeader::from_object_ptr)
    }
}

impl Default for TaggedPtr {
    fn default() -> Self {
        TaggedPtr::NULL
    }
}

impl From<f64> for TaggedPtr {
    fn from(value: f64) -> TaggedPtr {
        TaggedPtr { number: value }
    }
}

impl TryInto<f64> for TaggedPtr {
    type Error = GCError;
    fn try_into(self) -> Result<f64, GCError> {
        if self.is_num() {
            Ok(unsafe { self.number })
        } else {
            Err(GCError::TypeError)
        }
    }
}

impl From<bool> for TaggedPtr {
    fn from(value: bool) -> TaggedPtr {
        if value {
            TaggedPtr::TRUE
        } else {
            TaggedPtr::FALSE
        }
    }
}

// This is only TryFrom instead of From, because the caller needs to determine
// what is "truthy" or "falsey" this only converts to bools when was was stored
// was true or false.
impl TryFrom<TaggedPtr> for bool {
    type Error = GCError;
    fn try_from(tagged: TaggedPtr) -> Result<bool, GCError> {
        if tagged.is_true_singleton() {
            Ok(true)
        } else if tagged.is_false_singleton() {
            Ok(false)
        } else {
            Err(GCError::TypeError)
        }
    }
}

impl From<ObjectPtr> for TaggedPtr {
    fn from(ptr: ObjectPtr) -> TaggedPtr {
        TaggedPtr {
            bits: unsafe { std::mem::transmute::<ObjectPtr, usize>(ptr) | PTR_TAG_MASK },
        }
    }
}

impl TryFrom<TaggedPtr> for ObjectPtr {
    type Error = GCError;
    fn try_from(tagged: TaggedPtr) -> Result<ObjectPtr, GCError> {
        if tagged.is_ptr() {
            Ok(unsafe { std::mem::transmute::<usize, ObjectPtr>(tagged.bits & PTR_MASK) })
        } else {
            Err(GCError::TypeError)
        }
    }
}

impl std::fmt::Debug for TaggedPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaggedPtr").finish()
    }
}

impl PartialEq for TaggedPtr {
    fn eq(&self, rhs: &TaggedPtr) -> bool {
        if self.is_ptr() != rhs.is_ptr() {
            return false;
        }
        if self.is_ptr() {
            let lhs_ptr = self.clone().try_into().unwrap();
            let rhs_ptr = rhs.clone().try_into().unwrap();
            let lhs_object = TraceableObject::load(lhs_ptr);
            lhs_object.as_traceable().object_eq(lhs_ptr, rhs_ptr)
        } else {
            unsafe { self.bits == rhs.bits }
        }
    }
}

impl Eq for TaggedPtr {}

impl Hash for TaggedPtr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if self.is_ptr() {
            let ptr = self.clone().try_into().unwrap();
            let object = TraceableObject::load(ptr);
            object.as_traceable().object_hash(ptr).hash(state);
        } else {
            unsafe { self.bits.hash(state) }
        }
    }
}

// ObjectPtr could have a generation number, and thus we could know
// if we ever forgot one between generations (and thus was invalid).
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct ObjectPtr(*mut u8);

impl ObjectPtr {
    // Note: addr is assumed to point to a TraceableObject.
    fn new(addr: *mut u8) -> ObjectPtr {
        ObjectPtr(addr)
    }

    pub fn addr(&self) -> *mut u8 {
        self.0
    }

    fn to_header_ptr(&self) -> HeaderPtr {
        HeaderPtr::new(unsafe { self.addr().sub(HEADER_SIZE) })
    }

    pub fn header(&self) -> &mut ObjectHeader {
        ObjectHeader::from_object_ptr(*self)
    }

    pub fn is_type(&self, expected: ObjectType) -> bool {
        self.header().object_type == expected
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct HeaderPtr(*mut u8);

impl HeaderPtr {
    pub fn new(addr: *mut u8) -> HeaderPtr {
        HeaderPtr(addr)
    }

    pub fn addr(&self) -> *mut u8 {
        self.0
    }

    pub fn to_object_ptr(&self) -> ObjectPtr {
        ObjectPtr::new(unsafe { self.addr().add(HEADER_SIZE) })
    }
}

#[derive(Debug, PartialEq)]
#[repr(u16)]
pub enum ObjectType {
    Host,
}

#[derive(Debug)]
#[repr(C)]
pub struct ObjectHeader {
    object_size: usize,
    pub object_type: ObjectType,

    // When we move the object to the new space, we'll record in this field
    // where we moved it to.
    pub new_header_ptr: Option<HeaderPtr>,
}

const HEADER_SIZE: usize = std::mem::size_of::<ObjectHeader>();

impl ObjectHeader {
    pub fn new<'a>(
        space: &mut Space,
        object_size: usize,
        object_type: ObjectType,
    ) -> Result<&'a mut ObjectHeader, GCError> {
        let header_ptr = HeaderPtr::new(space.alloc(HEADER_SIZE + object_size)?);
        let header = ObjectHeader::from_ptr(header_ptr);
        header.object_size = object_size;
        header.object_type = object_type;
        Ok(header)
    }

    fn from_ptr<'a>(header_ptr: HeaderPtr) -> &'a mut ObjectHeader {
        unsafe { &mut *(header_ptr.addr() as *mut ObjectHeader) }
    }

    fn from_object_ptr<'a>(object_ptr: ObjectPtr) -> &'a mut ObjectHeader {
        Self::from_ptr(object_ptr.to_header_ptr())
    }

    pub fn alloc_size(&self) -> usize {
        HEADER_SIZE + self.object_size
    }

    pub fn as_ptr(&mut self) -> HeaderPtr {
        HeaderPtr::new(self as *mut ObjectHeader as *mut u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn size_test() {
        // This should be compile time instead:
        // https://github.com/rust-lang/rfcs/issues/2790
        assert_eq!(std::mem::size_of::<TaggedPtr>(), 8);
    }

    #[test]
    pub fn null_test() {
        assert!(TaggedPtr::default().is_null());
        let zero: TaggedPtr = 0.0.into();
        assert!(!zero.is_null());
    }

    #[test]
    pub fn truthiness_test() {
        // This layer intentionally only gives an answer for True and False
        // and leaves what else is "truthy" or "falsey" to the caller.
        assert_eq!(bool::try_from(TaggedPtr::FALSE).unwrap(), false);
        assert_eq!(bool::try_from(TaggedPtr::TRUE).unwrap(), true);
        assert_eq!(bool::try_from(TaggedPtr::NULL).ok(), None);

        // Try round-tripping a pointer as well.
        let boxed = Box::new(1);
        // This is technically unsafe use of ObjectPtr::new() as
        // ObjectPtr::eq assumes any ptr() is a TraceableObject.
        let ptr = ObjectPtr::new(Box::into_raw(boxed));
        let tagged = TaggedPtr::from(ptr);
        assert_eq!(bool::try_from(tagged).ok(), None);
        let ptr: ObjectPtr = tagged.try_into().unwrap();
        let boxed = unsafe { Box::from_raw(ptr.addr()) };
        assert_eq!(*boxed, 1);
    }

    #[test]
    pub fn eq_test() {
        assert_eq!(TaggedPtr::TRUE, TaggedPtr::TRUE);
        assert_ne!(TaggedPtr::TRUE, TaggedPtr::FALSE);
        assert_ne!(TaggedPtr::NULL, TaggedPtr::FALSE);
        let zero: TaggedPtr = 0.0.into();
        assert_ne!(TaggedPtr::NULL, zero);
        assert_ne!(TaggedPtr::FALSE, zero);

        // FIXME: This crashes?  Maybe T needs to be Tracable?
        // If so, what part of the type-system is failing here?
        // fn tagged_from_object<T>(value: T) -> TaggedPtr {
        //     let boxed = Box::new(value);
        //     let raw_ptr = Box::into_raw(boxed) as *mut u8;
        //     TaggedPtr::from(ObjectPtr::new(raw_ptr))
        // }

        // let one = tagged_from_object(1);
        // let also_one = tagged_from_object(1);
        // let two = tagged_from_object(2);
        // let object_true = tagged_from_object(true);
        // assert_eq!(one, one);
        // assert_eq!(one, also_one);
        // assert_ne!(one, two);
        // assert_ne!(one, object_true);
        // assert_ne!(TaggedPtr::TRUE, object_true);
    }
}
