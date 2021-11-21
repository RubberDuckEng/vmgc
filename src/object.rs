use std::any::Any;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

use crate::pointer::*;
use crate::space::*;

pub struct ObjectVisitor {
    pub space: Space,
    pub queue: VecDeque<ObjectPtr>,
}

impl ObjectVisitor {
    pub fn new(space: Space) -> ObjectVisitor {
        ObjectVisitor {
            space,
            queue: VecDeque::default(),
        }
    }

    fn visit(&mut self, header: &mut ObjectHeader) -> ObjectPtr {
        if let Some(new_header_ptr) = header.new_header_ptr {
            return new_header_ptr.to_object_ptr();
        }
        let alloc_size = header.alloc_size();
        let new_header_ptr = HeaderPtr::new(self.space.alloc(alloc_size).unwrap());
        unsafe {
            std::ptr::copy_nonoverlapping(
                header.as_ptr().addr(),
                new_header_ptr.addr(),
                alloc_size,
            );
        }
        header.new_header_ptr = Some(new_header_ptr);
        let object_ptr = new_header_ptr.to_object_ptr();
        self.queue.push_back(object_ptr);
        object_ptr
    }

    pub fn trace_handles(&mut self, handles: &Vec<HeapHandle>) {
        for index in 0..handles.len() {
            let handle = &handles[index];
            handle.trace(self);
        }
    }

    pub fn trace_maybe_handles(&mut self, handles: &Vec<Option<HeapHandle>>) {
        for index in 0..handles.len() {
            if let Some(handle) = &handles[index] {
                handle.trace(self);
            }
        }
    }
}
#[derive(PartialEq, Eq)]
#[repr(transparent)]
pub struct HeapHandle {
    // Held in a Cell so that visit doesn't require mut self.
    // visit() is the ONLY place where ptr should ever change.
    ptr: Cell<TaggedPtr>,
}

impl Default for HeapHandle {
    fn default() -> Self {
        HeapHandle::new(TaggedPtr::NULL)
    }
}

impl Hash for HeapHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ptr().hash(state);
    }
}

impl HeapHandle {
    pub fn new(ptr: TaggedPtr) -> HeapHandle {
        HeapHandle {
            ptr: Cell::new(ptr),
        }
    }

    pub fn ptr(&self) -> TaggedPtr {
        self.ptr.get()
    }

    pub fn trace(&self, visitor: &mut ObjectVisitor) {
        if let Some(header) = self.ptr().header() {
            self.ptr.set(visitor.visit(header).into());
        }
    }

    // This intentionally takes &mut self and has normal mutation
    // rules, only visit() should use _ptr.set().
    pub fn set_ptr(&mut self, ptr: TaggedPtr) {
        self.ptr.set(ptr);
    }

    pub fn take(&mut self) -> HeapHandle {
        let result = HeapHandle::new(self.ptr());
        self.set_ptr(TaggedPtr::default());
        result
    }
}

pub trait AsAny: Any {
    fn as_any(&self) -> &dyn Any;
    fn type_name(&self) -> &'static str;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}
pub trait Traceable: AsAny {
    fn trace(&mut self, _visitor: &mut ObjectVisitor);

    // Using Hash<T> includes a type parameter, which makes Tracable no longer
    // dyn compatible and the rust compiler barfs. :/
    // fn object_hash(&self) -> u64 {
    //     let mut hasher = DefaultHasher::new();
    //     std::ptr::hash(self as *const dyn Traceable, &mut hasher);
    //     hasher.finish()
    // }

    // fn object_eq(&self, rhs: &dyn Traceable) -> bool {
    //     std::ptr::eq(self as *const dyn Traceable, rhs as *const dyn Traceable)
    // }

    fn object_hash(&self, ptr: ObjectPtr) -> u64 {
        ptr.addr() as u64
    }

    fn object_eq(&self, lhs: ObjectPtr, rhs: ObjectPtr) -> bool {
        lhs.addr().eq(&rhs.addr())
    }
}

#[repr(C)]
pub struct TraceableObject {
    ptr: *mut dyn Traceable,
}

impl TraceableObject {
    pub fn new(traceable: Box<dyn Traceable>) -> TraceableObject {
        TraceableObject {
            ptr: Box::into_raw(traceable),
        }
    }

    pub fn into_box(self) -> Box<dyn Traceable> {
        unsafe { Box::from_raw(self.ptr) }
    }

    pub fn store(&self, object_ptr: ObjectPtr) {
        unsafe {
            *(object_ptr.addr() as *mut *mut dyn Traceable) = self.ptr;
        }
    }

    pub fn load(object_ptr: ObjectPtr) -> TraceableObject {
        let traceable_ptr = unsafe { *(object_ptr.addr() as *mut *mut dyn Traceable) };
        TraceableObject { ptr: traceable_ptr }
    }

    pub fn as_traceable(&self) -> &mut dyn Traceable {
        unsafe { &mut (*self.ptr) }
    }

    pub fn downcast<T: 'static>(object_ptr: ObjectPtr) -> *const T {
        let traceable_ptr = unsafe { *(object_ptr.addr() as *const *const dyn Traceable) };
        let traceable_ref = unsafe { &(*traceable_ptr) };
        traceable_ref.as_any().downcast_ref().unwrap() as *const T
    }

    pub fn downcast_mut<T: 'static>(object_ptr: ObjectPtr) -> *mut T {
        Self::downcast::<T>(object_ptr) as *mut T
    }
}

// We will eventually add a HeapObject as an optimization
// for things which don't hold pointers out to rust objects.
pub trait HostObject: Traceable + Default {
    const TYPE_ID: ObjectType;
}

impl HostObject for String {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for String {
    fn trace(&mut self, _visitor: &mut ObjectVisitor) {}

    fn object_hash(&self, _ptr: ObjectPtr) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    fn object_eq(&self, _lhs: ObjectPtr, rhs_object_ptr: ObjectPtr) -> bool {
        // FIXME: This depends on the caller having passed the correct ObjectPtr
        let rhs_ptr = TraceableObject::downcast::<String>(rhs_object_ptr);
        let rhs = unsafe { &*rhs_ptr };
        self.eq(rhs)
    }
}

pub type Map = HashMap<HeapHandle, HeapHandle>;

impl HostObject for Map {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for Map {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        for (key, value) in self.iter_mut() {
            key.trace(visitor);
            value.trace(visitor);
        }
    }
}

#[derive(Default, Hash)]
pub struct List {
    pub values: Vec<HeapHandle>,
}

impl HostObject for List {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for List {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        visitor.trace_handles(&self.values);
    }
}
