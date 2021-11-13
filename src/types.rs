#[derive(Debug)]
pub enum GCError {
    // The operating system did not provide use with memory.
    OSOutOfMemory,

    // There is no memory left in this space.
    NoSpace,
    // There is no space left in the heap to allocate this object, even after
    // collecting dead objects.
    // HeapFull,
    TypeError,
}
