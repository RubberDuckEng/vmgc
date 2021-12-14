use std::cell::RefCell;
use std::convert::TryInto;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::object::*;
use crate::pointer::*;
use crate::space::*;
use crate::types::*;

struct HeapInner {
    // TODO: Add more generations.
    space: Space,
    scopes: Vec<Vec<HeapHandle<()>>>,
    globals: Vec<Option<HeapHandle<()>>>,
    weaks: Vec<HeapHandle<()>>,
}

impl HeapInner {
    fn new(space: Space) -> HeapInner {
        HeapInner {
            space,
            globals: vec![],
            scopes: vec![],
            weaks: vec![],
        }
    }

    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        visitor.trace_maybe_handles(&mut self.globals);
        for scope in self.scopes.iter_mut() {
            // FIXME:  Scope should be an object, not a vec here.
            visitor.trace_handles(scope);
        }

        while let Some(object_ptr) = visitor.queue.pop_front() {
            let object = TraceableObject::load(object_ptr);
            let traceable = object.as_traceable();
            traceable.trace(visitor);
        }
    }

    fn update_weak(&mut self) -> Vec<Box<dyn Traceable>> {
        let mut doomed = vec![];
        let mut survivors = vec![];
        for handle in self.weaks.iter() {
            let maybe_object_ptr: Option<ObjectPtr> = handle.ptr().try_into().ok();
            if let Some(object_ptr) = maybe_object_ptr {
                let old_header = object_ptr.header();
                if let Some(new_header_ptr) = old_header.new_header_ptr {
                    survivors.push(HeapHandle::new(new_header_ptr.to_object_ptr().into()));
                } else {
                    let object = TraceableObject::load(object_ptr);
                    doomed.push(object.into_box());
                }
            }
        }
        std::mem::swap(&mut self.weaks, &mut survivors);
        doomed
    }
}

impl std::fmt::Debug for HeapInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeapInner").finish()
    }
}

#[derive(Debug)]
pub struct Heap {
    inner: Arc<RefCell<HeapInner>>,
}

impl Heap {
    pub fn new(size_in_bytes: usize) -> Result<Heap, GCError> {
        let half_size = size_in_bytes / 2;
        Ok(Heap {
            inner: Arc::new(RefCell::new(HeapInner::new(Space::new(half_size)?))),
        })
    }

    pub fn used(&self) -> usize {
        self.inner.borrow().space.used()
    }

    pub fn collect(&self) -> Result<(), GCError> {
        let doomed = {
            let mut visitor = ObjectVisitor::new(Space::new(self.inner.borrow().space.size)?);
            let mut inner = self.inner.borrow_mut();
            inner.trace(&mut visitor);
            let doomed = inner.update_weak();
            std::mem::swap(&mut inner.space, &mut visitor.space);
            doomed
        };
        std::mem::drop(doomed);
        Ok(())
    }

    fn emplace<T: HostObject>(&self, object: Box<T>) -> Result<ObjectPtr, GCError> {
        let object_size = std::mem::size_of::<TraceableObject>();
        let mut inner = self.inner.borrow_mut();
        let header = ObjectHeader::new(&mut inner.space, object_size, T::TYPE_ID)?;
        let object_ptr = header.as_ptr().to_object_ptr();
        TraceableObject::from_box(object).store(object_ptr);
        inner.weaks.push(HeapHandle::new(object_ptr.into()));
        Ok(object_ptr)
    }
}

#[derive(Debug)]
struct Root {
    inner: Arc<RefCell<HeapInner>>,
    index: usize,
}

#[derive(Debug)]
pub struct GlobalHandle<T> {
    root: Root,
    _phantom: PhantomData<T>,
}

impl<T> GlobalHandle<T> {
    fn ptr(&self) -> TaggedPtr {
        let inner = self.root.inner.borrow();
        let cell = inner.globals[self.root.index].as_ref().unwrap();
        cell.ptr()
    }

    pub fn erase_type(self) -> GlobalHandle<()> {
        GlobalHandle {
            root: self.root,
            _phantom: PhantomData::<()>::default(),
        }
    }
}

impl<T> From<GlobalHandle<T>> for HeapHandle<T> {
    fn from(handle: GlobalHandle<T>) -> Self {
        HeapHandle::<T>::new(handle.ptr())
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        self.inner.borrow_mut().globals[self.index] = None;
    }
}

pub struct HandleScope<'a> {
    heap: &'a Heap,
    index: usize,
}

impl<'a> HandleScope<'a> {
    pub fn new(heap: &Heap) -> HandleScope {
        let mut inner = heap.inner.borrow_mut();
        let index = inner.scopes.len();
        inner.scopes.push(vec![]);
        HandleScope { heap, index }
    }

    pub fn create_num(&self, value: f64) -> LocalHandle<f64> {
        LocalHandle::<f64>::new(self, value.into())
    }

    pub fn create_bool(&self, value: bool) -> LocalHandle<bool> {
        LocalHandle::<bool>::new(self, value.into())
    }

    // TODO: What type should null be?
    pub fn create_null(&self) -> LocalHandle<()> {
        LocalHandle::<()>::new(self, TaggedPtr::NULL)
    }

    pub fn create<T: HostObject + Default>(&self) -> Result<LocalHandle<T>, GCError> {
        let object_ptr = self.heap.emplace(Box::new(T::default()))?;
        Ok(LocalHandle::<T>::new(self, object_ptr.into()))
    }

    pub fn take<T: HostObject>(&self, object: T) -> Result<LocalHandle<T>, GCError> {
        let object_ptr = self.heap.emplace(Box::new(object))?;
        Ok(LocalHandle::<T>::new(self, object_ptr.into()))
    }

    // Should this be create_str?
    // Could also do generically for ToOwned?
    // fn from_unowned<T, S>(...) where T: ToOwned<S>, S : HostObject {...}
    pub fn str(&self, object: &str) -> Result<LocalHandle<String>, GCError> {
        self.take(object.to_string())
    }

    fn add(&self, ptr: TaggedPtr) -> usize {
        let mut inner = self.heap.inner.borrow_mut();
        let cells = &mut inner.scopes[self.index];
        let index = cells.len();
        cells.push(HeapHandle::new(ptr));
        index
    }

    pub fn from_global<T>(&self, handle: &GlobalHandle<T>) -> LocalHandle<T> {
        LocalHandle::<T>::new(self, handle.ptr())
    }

    pub fn from_heap<T>(&self, handle: &HeapHandle<T>) -> LocalHandle<T> {
        LocalHandle::<T>::new(self, handle.ptr())
    }

    pub fn from_maybe_heap<T>(
        &self,
        maybe_handle: &Option<HeapHandle<T>>,
    ) -> Option<LocalHandle<T>> {
        maybe_handle
            .clone()
            .map(|handle| LocalHandle::<T>::new(self, handle.ptr()))
    }

    pub fn as_ref<T: HostObject>(&self, handle: &GlobalHandle<T>) -> &T {
        let local = self.from_global(handle);
        local.as_ref()
    }

    pub fn as_mut<T: HostObject>(&self, handle: &GlobalHandle<T>) -> &mut T {
        let local = self.from_global(handle);
        local.as_mut()
    }

    fn get_ptr(&self, index: usize) -> TaggedPtr {
        let inner = self.heap.inner.borrow();
        inner.scopes[self.index][index].ptr()
    }
}

impl<'a> Drop for HandleScope<'a> {
    fn drop(&mut self) {
        let mut inner = self.heap.inner.borrow_mut();
        inner.scopes.pop();
    }
}

#[derive(Copy)]
pub struct LocalHandle<'a, T> {
    scope: &'a HandleScope<'a>,
    index: usize,
    phantom: PhantomData<T>,
}

// Derive Clone requires T to be Cloneable, which isn't required for Handles.
impl<'a, T> Clone for LocalHandle<'a, T> {
    fn clone(&self) -> Self {
        LocalHandle {
            scope: self.scope,
            index: self.index,
            phantom: PhantomData::<T>::default(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.scope = source.scope;
        self.index = source.index;
    }
}

impl<'a, T> LocalHandle<'a, T> {
    fn new(scope: &'a HandleScope, ptr: TaggedPtr) -> Self {
        Self {
            scope: scope,
            index: scope.add(ptr),
            phantom: PhantomData::<T>::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn ptr_for_test(&self) -> TaggedPtr {
        self.ptr()
    }

    fn ptr(&self) -> TaggedPtr {
        self.scope.get_ptr(self.index)
    }

    fn get_object_ptr(&self) -> Option<ObjectPtr> {
        self.ptr().try_into().ok()
    }

    pub fn erase_type(&self) -> LocalHandle<'a, ()> {
        LocalHandle {
            scope: self.scope,
            index: self.index,
            phantom: PhantomData::<()>::default(),
        }
    }
}

impl<'a> LocalHandle<'a, ()> {
    pub fn is_null(&self) -> bool {
        self.ptr().is_null()
    }

    pub fn is_bool(&self) -> bool {
        self.ptr().is_bool()
    }

    pub fn is_num(&self) -> bool {
        self.ptr().is_num()
    }

    pub fn try_as_ref<S: HostObject>(&self) -> Option<&'a S> {
        if let Some(object_ptr) = self.get_object_ptr() {
            if object_ptr.is_type(S::TYPE_ID) {
                if let Some(ptr) = TraceableObject::try_downcast::<S>(object_ptr) {
                    return Some(unsafe { &*ptr });
                }
            }
        }
        None
    }

    pub fn try_as_mut<S: HostObject>(&self) -> Option<&'a mut S> {
        if let Some(object_ptr) = self.get_object_ptr() {
            if object_ptr.is_type(S::TYPE_ID) {
                if let Some(ptr) = TraceableObject::try_downcast::<S>(object_ptr) {
                    let mut_ptr = ptr as *mut S;
                    return Some(unsafe { &mut *mut_ptr });
                }
            }
        }
        None
    }

    pub fn is_of_type<S: HostObject>(&self) -> bool {
        let maybe_ref: Option<&S> = self.try_as_ref();
        maybe_ref.is_some()
    }
}

pub trait DowncastTo<T> {
    fn try_downcast(self) -> Option<T>;
}

impl<'a, T: HostObject> DowncastTo<LocalHandle<'a, T>> for LocalHandle<'a, ()> {
    fn try_downcast(self) -> Option<LocalHandle<'a, T>> {
        if let Some(object_ptr) = self.get_object_ptr() {
            if object_ptr.is_type(T::TYPE_ID) {
                let ptr = TraceableObject::try_downcast::<T>(object_ptr);
                if ptr.is_some() {
                    return Some(LocalHandle {
                        scope: self.scope,
                        index: self.index,
                        phantom: PhantomData::<T>::default(),
                    });
                }
            }
        }
        None
    }
}

impl<'a> DowncastTo<LocalHandle<'a, f64>> for LocalHandle<'a, ()> {
    fn try_downcast(self) -> Option<LocalHandle<'a, f64>> {
        self.try_into()
            .ok()
            .map(|value| self.scope.create_num(value))
    }
}

impl<'a> DowncastTo<LocalHandle<'a, bool>> for LocalHandle<'a, ()> {
    fn try_downcast(self) -> Option<LocalHandle<'a, bool>> {
        self.try_into()
            .ok()
            .map(|value| self.scope.create_bool(value))
    }
}

impl<'a, T: HostObject> LocalHandle<'a, T> {
    pub fn borrow(&self) -> &'a T {
        let object_ptr = self.get_object_ptr().unwrap();
        let ptr = TraceableObject::downcast::<T>(object_ptr);
        unsafe { &*ptr }
    }

    pub fn borrow_mut(&self) -> &'a mut T {
        let object_ptr = self.get_object_ptr().unwrap();
        let ptr = TraceableObject::downcast_mut::<T>(object_ptr);
        unsafe { &mut *ptr }
    }

    // Old names:
    pub fn as_ref(&self) -> &'a T {
        self.borrow()
    }

    pub fn as_mut(&self) -> &'a mut T {
        self.borrow_mut()
    }
}

impl<'a> TryInto<f64> for LocalHandle<'a, ()> {
    type Error = GCError;
    fn try_into(self) -> Result<f64, GCError> {
        self.ptr().try_into()
    }
}

impl<'a> Into<f64> for LocalHandle<'a, f64> {
    fn into(self) -> f64 {
        self.ptr().try_into().unwrap()
    }
}

impl<'a> TryInto<bool> for LocalHandle<'a, ()> {
    type Error = GCError;
    fn try_into(self) -> Result<bool, GCError> {
        self.ptr().try_into()
    }
}

impl<'a> Into<bool> for LocalHandle<'a, bool> {
    fn into(self) -> bool {
        self.ptr().try_into().unwrap()
    }
}

impl<'a, T> From<LocalHandle<'a, T>> for HeapHandle<T> {
    fn from(handle: LocalHandle<'a, T>) -> Self {
        HeapHandle::<T>::new(handle.ptr())
    }
}

impl<'a, T> From<LocalHandle<'a, T>> for GlobalHandle<T> {
    fn from(handle: LocalHandle<'a, T>) -> Self {
        let ptr = handle.ptr();
        let index = {
            // TODO: Scan for available cells.
            let mut inner = handle.scope.heap.inner.borrow_mut();
            let index = inner.globals.len();
            inner.globals.push(Some(HeapHandle::<()>::new(ptr)));
            index
        };
        GlobalHandle {
            root: Root {
                inner: Arc::clone(&handle.scope.heap.inner),
                index,
            },
            _phantom: PhantomData::<T>::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::Cell;
    use std::convert::TryInto;
    use std::hash::{Hash, Hasher};
    use std::rc::Rc;

    #[derive(Default)]
    struct DropObject {
        counter: Rc<Cell<u32>>,
    }

    impl HostObject for DropObject {
        const TYPE_ID: ObjectType = ObjectType::Host;
    }

    impl Traceable for DropObject {
        fn trace(&mut self, _visitor: &mut ObjectVisitor) {}
    }

    impl Drop for DropObject {
        fn drop(&mut self) {
            let counter = self.counter.get();
            self.counter.set(counter + 1);
        }
    }

    impl Hash for DropObject {
        fn hash<H: Hasher>(&self, state: &mut H) {
            (self as *const DropObject as usize).hash(state);
        }
    }

    #[test]
    pub fn smoke_test() {
        let heap = Heap::new(1000).unwrap();
        assert_eq!(heap.used(), 0);
        let two: GlobalHandle<DropObject> = {
            let scope = HandleScope::new(&heap);
            let one = scope.create::<DropObject>().unwrap();
            let two = scope.create::<DropObject>().unwrap();
            std::mem::drop(one);
            two.into()
        };
        let used_before_collection = heap.used();
        heap.collect().unwrap();
        let used_after_collection = heap.used();
        assert!(0 < used_after_collection);
        assert!(used_before_collection > used_after_collection);
        std::mem::drop(two);
        heap.collect().unwrap();
        assert_eq!(0, heap.used());
    }

    #[test]
    fn finalizer_test() {
        let heap = Heap::new(1000).unwrap();
        let counter = Rc::new(Cell::new(0));
        let scope = HandleScope::new(&heap);

        let handle = scope.create::<DropObject>().unwrap();
        handle.as_mut().counter = Rc::clone(&counter);
        std::mem::drop(handle);
        assert_eq!(0u32, counter.get());
        std::mem::drop(scope);
        assert_eq!(0u32, counter.get());
        heap.collect().ok();
        assert_eq!(1u32, counter.get());
    }

    #[test]
    fn tracing_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let handle = scope.create::<List<DropObject>>().unwrap();

        let list = handle.as_mut();
        list.push(scope.create::<DropObject>().unwrap().into());
        list.push(scope.create::<DropObject>().unwrap().into());
        list.push(scope.create::<DropObject>().unwrap().into());
        std::mem::drop(list);

        let used = heap.used();
        heap.collect().ok();
        assert_eq!(used, heap.used());
        std::mem::drop(handle);
        heap.collect().ok();
        assert_eq!(used, heap.used());
        std::mem::drop(scope);
        heap.collect().ok();
        assert_eq!(0, heap.used());
    }

    #[test]
    fn tagged_num_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);

        let a = scope.create_num(1.0);
        let b = scope.create_num(2.0);
        assert_eq!(0, heap.used());
        let a_value: f64 = a.ptr().try_into().unwrap();
        assert_eq!(1.0, a_value);
        let b_value: f64 = b.ptr().try_into().unwrap();
        assert_eq!(2.0, b_value);
    }

    #[test]
    fn add_f64_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let one = scope.create_num(1.0);
        let two = scope.create_num(2.0);
        let one_value: f64 = one.try_into().unwrap();
        assert_eq!(1.0, one_value);
        let two_value: f64 = two.try_into().unwrap();
        assert_eq!(2.0, two_value);
        let three_value = one_value + two_value;
        let three = scope.create_num(three_value);
        let three_global = GlobalHandle::from(three);
        std::mem::drop(scope);

        let scope = HandleScope::new(&heap);
        let three = scope.from_global(&three_global);
        let three_value: f64 = three.try_into().unwrap();
        assert_eq!(3.0, three_value);
    }

    #[test]
    fn list_push_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let list = scope.create::<List<f64>>().unwrap();
        let one = scope.create_num(1.0);
        let list_value = list.as_mut();
        list_value.push(one.into());
        std::mem::drop(list_value);
        heap.collect().ok();
        let list_value = list.as_ref();
        assert_eq!(list_value.len(), 1);
    }

    #[test]
    fn string_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let string_handle = scope.create::<String>().unwrap();
        heap.collect().ok();
        let string_value = string_handle.as_ref();
        assert_eq!(string_value, "");
    }

    #[test]
    fn take_string_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let string_handle = scope.take("Foo".to_string()).unwrap();
        heap.collect().ok();
        let string_value = string_handle.as_ref();
        assert_eq!(string_value, "Foo");
    }

    #[test]
    fn list_push_string_twice_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let list = scope.create::<List<String>>().unwrap();
        let string = scope.str("Foo").unwrap();
        let list_value = list.as_mut();
        list_value.push(string.clone().into());
        list_value.push(string.clone().into());
        std::mem::drop(list_value);
        heap.collect().ok();
        let list_value = list.as_mut();
        assert_eq!(list_value.len(), 2);
        assert_eq!(list_value[0].as_ref(), "Foo");
        assert_eq!(list_value[1].as_ref(), "Foo");
        string.as_mut().push_str("Bar");
        assert_eq!(list_value[0].as_ref(), "FooBar");
    }

    #[test]
    fn map_insert_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let map = scope.create::<Map<String, String>>().unwrap();
        let foo = scope.str("Foo").unwrap();
        let bar = scope.str("Bar").unwrap();
        let map_value = map.as_mut();
        map_value.insert(foo.clone().into(), bar.clone().into());
        std::mem::drop(map_value);
        std::mem::drop(foo);
        std::mem::drop(bar);

        // Check if lookup works before collect.
        {
            let map_value = map.as_mut();
            let foo = scope.str("Foo").unwrap();
            let bar = scope.from_heap(map_value.get(&foo.into()).unwrap());
            assert_eq!(bar.as_ref(), "Bar");
        }

        heap.collect().ok();

        let map_value = map.as_ref();
        let foo = scope.str("Foo").unwrap();
        let bar = map_value.get(&foo.into()).unwrap();
        assert_eq!(bar.as_ref(), "Bar");
    }

    #[test]
    fn typed_handle_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);

        // Bools
        let boolean: LocalHandle<bool> = scope.create_bool(true);
        let out: bool = boolean.into();
        assert_eq!(out, true);
        // bool.as_ref() can't work.
        // bool.as_mut() similarly so.

        // Nums
        let num: LocalHandle<f64> = scope.create_num(1.0);
        let out: f64 = num.try_into().unwrap();
        assert_eq!(out, 1.0);
        // num.as_ref() should be possible.
        // num.as_mut() might be possible?

        // Null
        let null: LocalHandle<()> = scope.create_null();
        assert_eq!(null.is_null(), true);

        // HostObjects (e.g. String)
        let string: LocalHandle<String> = scope.str("Foo").unwrap();
        assert_eq!(string.as_ref(), "Foo");

        // Untyped handles
        let untyped = num.erase_type();
        let out: f64 = untyped.try_into().unwrap();
        assert_eq!(out, 1.0);

        // create a String
        // try to store it in the wrong type'd handle
        // see it panic.

        // Things to test:
        // Combinations of types (null, f64 (valid and NaN), HostObject, bool)
        // - Getting refs to all types
        // - Geting (and changing?) a mut-ref to num, bool, null types?
        // - value cast to the wrong type
        // - handle cast to the wrong type
        // - Using try_downcast and getting back None with the wrong type.
    }

    #[test]
    fn downcast_to_typed_handle_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);

        // Bools
        let untyped: LocalHandle<()> = scope.create_bool(true).erase_type();
        let maybe_string: Option<LocalHandle<String>> = untyped.try_downcast();
        let maybe_bool: Option<LocalHandle<bool>> = untyped.try_downcast();
        let maybe_f64: Option<LocalHandle<f64>> = untyped.try_downcast();
        assert!(maybe_string.is_none());
        assert!(maybe_bool.is_some());
        assert!(maybe_f64.is_none());

        // Nums
        let untyped: LocalHandle<()> = scope.create_num(1.0).erase_type();
        let maybe_string: Option<LocalHandle<String>> = untyped.try_downcast();
        let maybe_bool: Option<LocalHandle<bool>> = untyped.try_downcast();
        let maybe_f64: Option<LocalHandle<f64>> = untyped.try_downcast();
        assert!(maybe_string.is_none());
        assert!(maybe_bool.is_none());
        assert!(maybe_f64.is_some());

        // Null
        let untyped: LocalHandle<()> = scope.create_null();
        let maybe_string: Option<LocalHandle<String>> = untyped.try_downcast();
        let maybe_bool: Option<LocalHandle<bool>> = untyped.try_downcast();
        let maybe_f64: Option<LocalHandle<f64>> = untyped.try_downcast();
        assert!(maybe_string.is_none());
        assert!(maybe_bool.is_none());
        assert!(maybe_f64.is_none());

        // HostObjects (e.g. String)
        let untyped: LocalHandle<()> = scope.str("Foo").unwrap().erase_type();
        let maybe_string: Option<LocalHandle<String>> = untyped.try_downcast();
        let maybe_bool: Option<LocalHandle<bool>> = untyped.try_downcast();
        let maybe_f64: Option<LocalHandle<f64>> = untyped.try_downcast();
        assert!(maybe_string.is_some());
        assert!(maybe_bool.is_none());
        assert!(maybe_f64.is_none());
    }
}
