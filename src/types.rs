use std::error::Error;
use std::fmt;

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

impl fmt::Display for GCError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl Error for GCError {
    fn description(&self) -> &str {
        match self {
            GCError::OSOutOfMemory => "OS failed to provide memory",
            GCError::NoSpace => "No memory left in space",
            GCError::TypeError => "Type coercion failed",
        }
    }
}
