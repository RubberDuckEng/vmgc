use crate::heap::Space;
use crate::types::*;

// ObjectPtr could have a generation number, and thus we could know
// if we ever forgot one between generations (and thus was invalid).
#[derive(Copy, Clone, Debug)]
pub struct ObjectPtr(*mut u8);

impl ObjectPtr {
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
}

#[derive(Copy, Clone, Debug)]
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

#[derive(Debug)]
#[repr(C)]
pub enum ObjectType {
    Primitive,
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

pub const HEADER_SIZE: usize = std::mem::size_of::<ObjectHeader>();

impl ObjectHeader {
    pub fn new<'a>(
        space: &mut Space,
        object_size: usize,
        object_type: ObjectType,
    ) -> Result<&'a mut ObjectHeader, GCError> {
        let header_ptr = HeaderPtr::new(space.alloc(HEADER_SIZE + object_size)?);
        let header = ObjectHeader::from_header_ptr(header_ptr);
        header.object_size = object_size;
        header.object_type = object_type;
        Ok(header)
    }

    pub fn from_header_ptr<'a>(header_ptr: HeaderPtr) -> &'a mut ObjectHeader {
        unsafe { &mut *(header_ptr.addr() as *mut ObjectHeader) }
    }

    pub fn from_object_ptr<'a>(object_ptr: ObjectPtr) -> &'a mut ObjectHeader {
        Self::from_header_ptr(object_ptr.to_header_ptr())
    }

    pub fn alloc_size(&self) -> usize {
        HEADER_SIZE + self.object_size
    }

    pub fn as_ptr(&mut self) -> HeaderPtr {
        HeaderPtr::new(self as *mut ObjectHeader as *mut u8)
    }
}
