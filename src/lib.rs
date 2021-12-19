mod heap;
mod object;
mod pointer;
mod space;
mod types;

pub use heap::{DowncastTo, GlobalHandle, HandleScope, Heap, LocalHandle};
pub use object::{HeapHandle, HostObject, List, Map, ObjectVisitor, Traceable};
pub use pointer::ObjectType;
pub use types::GCError;
