use std::any::Any;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::heap::{HandleScope, LocalHandle};
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

    pub fn trace_handles<T>(&mut self, handles: &Vec<HeapHandle<T>>) {
        for index in 0..handles.len() {
            let handle = &handles[index];
            handle.trace(self);
        }
    }

    pub fn trace_maybe_handles<T>(&mut self, handles: &Vec<Option<HeapHandle<T>>>) {
        for index in 0..handles.len() {
            if let Some(handle) = &handles[index] {
                handle.trace(self);
            }
        }
    }
}
#[derive(PartialEq, Eq)]
#[repr(transparent)]
pub struct HeapHandle<T> {
    // Held in a Cell so that visit doesn't require mut self.
    // visit() is the ONLY place where ptr should ever change.
    ptr: Cell<TaggedPtr>,
    _phantom: PhantomData<T>,
}

impl<T> Default for HeapHandle<T> {
    fn default() -> Self {
        HeapHandle::new(TaggedPtr::NULL)
    }
}

impl<T> Hash for HeapHandle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ptr().hash(state);
    }
}

impl<T> HeapHandle<T> {
    pub fn new(ptr: TaggedPtr) -> Self {
        Self {
            ptr: Cell::new(ptr),
            _phantom: PhantomData::<T>::default(),
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
        // FIXME: Danger, this could change the type T including to null!
        self.ptr.set(ptr);
    }

    pub fn erase_type(&self) -> HeapHandle<()> {
        HeapHandle {
            ptr: self.ptr.clone(),
            _phantom: PhantomData::<()>::default(),
        }
    }
}

impl HeapHandle<()> {
    // It's not safe to assign null to HeapHandle<T>
    pub fn take(&mut self) -> Self {
        let result = Self::new(self.ptr());
        self.set_ptr(TaggedPtr::default());
        result
    }

    pub fn is_null(&self) -> bool {
        self.ptr().is_null()
    }

    pub fn is_num(&self) -> bool {
        self.ptr().is_num()
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
    pub fn from_box(traceable: Box<dyn Traceable>) -> TraceableObject {
        TraceableObject {
            ptr: Box::into_raw(traceable),
        }
    }

    pub fn into_box(self) -> Box<dyn Traceable> {
        unsafe { Box::from_raw(self.ptr) }
    }

    pub fn store(&self, object_ptr: ObjectPtr) {
        // FIXME: Express this precondition in the type system?
        assert!(object_ptr.header().object_type == ObjectType::Host);
        unsafe {
            *(object_ptr.addr() as *mut *mut dyn Traceable) = self.ptr;
        }
    }

    pub fn load(object_ptr: ObjectPtr) -> TraceableObject {
        // FIXME: Express this precondition in the type system?
        assert!(object_ptr.header().object_type == ObjectType::Host);
        let traceable_ptr = unsafe { *(object_ptr.addr() as *mut *mut dyn Traceable) };
        TraceableObject { ptr: traceable_ptr }
    }

    pub fn as_traceable(&self) -> &mut dyn Traceable {
        unsafe { &mut (*self.ptr) }
    }

    pub fn try_downcast<T: 'static>(object_ptr: ObjectPtr) -> Option<*const T> {
        // FIXME: Express this precondition in the type system?
        assert!(object_ptr.header().object_type == ObjectType::Host);
        let traceable_ptr = unsafe { *(object_ptr.addr() as *const *const dyn Traceable) };
        let traceable_ref = unsafe { &(*traceable_ptr) };
        traceable_ref
            .as_any()
            .downcast_ref()
            .map(|t_ref| t_ref as *const T)
    }

    /// This will panic (in unwrap) if the ObjectPtr does not point to a
    /// HostObject of type T.
    pub fn downcast<T: 'static>(object_ptr: ObjectPtr) -> *const T {
        Self::try_downcast(object_ptr).unwrap()
    }

    /// This will panic (in unwrap) if the ObjectPtr does not point to a
    /// HostObject of type T.
    pub fn downcast_mut<T: 'static>(object_ptr: ObjectPtr) -> *mut T {
        Self::downcast::<T>(object_ptr) as *mut T
    }
}

// We will eventually add a HeapObject as an optimization
// for things which don't hold pointers out to rust objects.
pub trait HostObject: Traceable {
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
        // FIXME: This still assumes ObjectPtr is an object!
        let maybe_rhs_ptr = TraceableObject::try_downcast::<String>(rhs_object_ptr);
        if let Some(rhs_ptr) = maybe_rhs_ptr {
            let rhs = unsafe { &*rhs_ptr };
            return self.eq(rhs);
        }
        false
    }
}

pub type Map<K, V> = HashMap<HeapHandle<K>, HeapHandle<V>>;

impl<K: 'static, V: 'static> HostObject for Map<K, V> {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl<K: 'static, V: 'static> Traceable for Map<K, V> {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        for (key, value) in self.iter_mut() {
            key.trace(visitor);
            value.trace(visitor);
        }
    }
}

#[derive(Default, Hash)]
pub struct List<T>(Vec<HeapHandle<T>>);

impl<T: 'static> HostObject for List<T> {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl<T: 'static> Traceable for List<T> {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        visitor.trace_handles(&self.0);
    }
}

impl List<()> {
    pub fn push<S>(&mut self, handle: HeapHandle<S>) {
        self.push(handle.erase_type());
    }
}

// FIXME: Use macros.
impl List<bool> {
    pub fn push(&mut self, handle: HeapHandle<bool>) {
        self.0.push(handle);
    }
}

impl List<f64> {
    pub fn push(&mut self, handle: HeapHandle<f64>) {
        self.0.push(handle);
    }
}

impl<T: HostObject> List<T> {
    pub fn push(&mut self, handle: HeapHandle<T>) {
        self.0.push(handle);
    }
}

impl<T> List<T> {
    pub fn pop<'a>(&mut self, scope: &'a HandleScope<'a>) -> Option<LocalHandle<'a, T>> {
        self.0.pop().map(|handle| scope.from_heap(&handle))
    }

    pub fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn first(&self) -> Option<&HeapHandle<T>> {
        self.0.first()
    }

    pub fn last(&self) -> Option<&HeapHandle<T>> {
        self.0.last()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T, I: std::slice::SliceIndex<[HeapHandle<T>]>> std::ops::Index<I> for List<T> {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        std::ops::Index::index(&self.0, index)
    }
}
