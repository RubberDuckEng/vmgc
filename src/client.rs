// 1. Create some sort of "Value" type?
// 2. Create tagged pointer
// Union
// Type_id
//

#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TypeId {
    Num,
    String,
    List,
}

#[derive(Copy, Clone)]
pub union TaggedPtr {
    tag: usize,
    number: isize,
    object: NonNull<()>,
}

const TAG_MASK: usize = 0x3;
pub const TAG_NUMBER: usize = 0x0;
pub const TAG_OBJECT: usize = 0x1;
const PTR_MASK: usize = !0x3;

impl From<i32> for TaggedPtr {
    fn from(value: i32) -> TaggedPtr {
        TaggedPtr {
            number: (value as isize) << 2,
        }
    }
}

impl TryInto<i32> for TaggedPtr {
    type Error = VMError;
    fn try_into(ptr: &TaggedPtr) -> Result<i32, Self::Error> {
        match ptr.tag & TAG_MASK {
            TAG_NUMBER => Ok(ptr.number >> 2 as i32),
            _ => Err(VMError::TypeError),
        }
    }
}

// Write primitive functions
// add numbers -> immediate value
// add to a list -> host object with references (traced)
// add strings -> leaf node host object (no tracing)

// fn num_add(heap: &mut Heap, a: LocalHandle, b: LocalHandle) -> Result<LocalHandle, VMError> {
//     let result = a.as_num()? + b.as_num()?;
//     heap.allocate_local::<Number>(result)
// }
