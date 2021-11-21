use std::cell::RefCell;
use std::convert::TryInto;
use std::sync::Arc;

use crate::object::*;
use crate::pointer::*;
use crate::space::*;
use crate::types::*;

struct HeapInner {
    // TODO: Add more generations.
    space: Space,
    scopes: Vec<Vec<HeapHandle>>,
    globals: Vec<Option<HeapHandle>>,
    weaks: Vec<HeapHandle>,
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
    pub fn new(size: usize) -> Result<Heap, GCError> {
        let half_size = size / 2;
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
        TraceableObject::new(object).store(object_ptr);
        inner.weaks.push(HeapHandle::new(object_ptr.into()));
        Ok(object_ptr)
    }
}

// Rename as Root
#[derive(Debug)]
pub struct GlobalHandle {
    inner: Arc<RefCell<HeapInner>>,
    index: usize,
}

impl GlobalHandle {
    fn ptr(&self) -> TaggedPtr {
        let inner = self.inner.borrow();
        let cell = inner.globals[self.index].as_ref().unwrap();
        cell.ptr()
    }
}

impl Drop for GlobalHandle {
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

    pub fn create_num(&self, value: f64) -> LocalHandle {
        LocalHandle::new(self, value.into())
    }

    pub fn create_null(&self) -> LocalHandle {
        LocalHandle::new(self, TaggedPtr::NULL)
    }

    pub fn create<T: HostObject>(&self) -> Result<LocalHandle, GCError> {
        let object_ptr = self.heap.emplace(Box::new(T::default()))?;
        Ok(LocalHandle::new(self, object_ptr.into()))
    }

    pub fn take<T: HostObject>(&self, object: T) -> Result<LocalHandle, GCError> {
        let object_ptr = self.heap.emplace(Box::new(object))?;
        Ok(LocalHandle::new(self, object_ptr.into()))
    }

    fn add(&self, ptr: TaggedPtr) -> usize {
        let mut inner = self.heap.inner.borrow_mut();
        let cells = &mut inner.scopes[self.index];
        let index = cells.len();
        cells.push(HeapHandle::new(ptr));
        index
    }

    pub fn from_global(&self, handle: &GlobalHandle) -> LocalHandle {
        LocalHandle::new(self, handle.ptr())
    }

    pub fn from_heap(&self, handle: &HeapHandle) -> LocalHandle {
        LocalHandle::new(self, handle.ptr())
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

#[derive(Copy, Clone)]
pub struct LocalHandle<'a> {
    scope: &'a HandleScope<'a>,
    index: usize,
}

impl<'a> LocalHandle<'a> {
    fn new(scope: &'a HandleScope, ptr: TaggedPtr) -> LocalHandle<'a> {
        LocalHandle {
            scope: scope,
            index: scope.add(ptr),
        }
    }

    pub fn ptr(&self) -> TaggedPtr {
        self.scope.get_ptr(self.index)
    }

    fn get_object_ptr(&self) -> Option<ObjectPtr> {
        self.ptr().try_into().ok()
    }

    pub fn as_ref<T: HostObject>(&self) -> Option<&T> {
        if let Some(object_ptr) = self.get_object_ptr() {
            // FIXME: Add ObjectPtr::is_type
            if object_ptr.header().object_type != T::TYPE_ID {
                return None;
            }
            let ptr = TraceableObject::downcast::<T>(object_ptr);
            Some(unsafe { &*ptr })
        } else {
            None
        }
    }

    pub fn as_mut<T: HostObject>(&self) -> Option<&mut T> {
        if let Some(object_ptr) = self.get_object_ptr() {
            // FIXME: Add ObjectPtr::is_type
            if object_ptr.header().object_type != T::TYPE_ID {
                return None;
            }
            let ptr = TraceableObject::downcast_mut::<T>(object_ptr);
            Some(unsafe { &mut *ptr })
        } else {
            None
        }
    }
}

impl<'a> TryInto<f64> for LocalHandle<'a> {
    type Error = GCError;
    fn try_into(self) -> Result<f64, GCError> {
        self.ptr().try_into()
    }
}

impl<'a> From<LocalHandle<'a>> for HeapHandle {
    fn from(handle: LocalHandle<'a>) -> Self {
        HeapHandle::new(handle.ptr())
    }
}

impl<'a> From<LocalHandle<'a>> for GlobalHandle {
    fn from(handle: LocalHandle<'a>) -> Self {
        let ptr = handle.ptr();
        let index = {
            // TODO: Scan for available cells.
            let mut inner = handle.scope.heap.inner.borrow_mut();
            let index = inner.globals.len();
            inner.globals.push(Some(HeapHandle::new(ptr)));
            index
        };
        GlobalHandle {
            inner: Arc::clone(&handle.scope.heap.inner),
            index,
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
        let two: GlobalHandle = {
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
        handle.as_mut::<DropObject>().unwrap().counter = Rc::clone(&counter);
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
        let handle = scope.create::<List>().unwrap();

        let list = handle.as_mut::<List>().unwrap();
        list.values
            .push(scope.create::<DropObject>().unwrap().into());
        list.values
            .push(scope.create::<DropObject>().unwrap().into());
        list.values
            .push(scope.create::<DropObject>().unwrap().into());
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
        let list = scope.create::<List>().unwrap();
        let one = scope.create_num(1.0);
        let list_value = list.as_mut::<List>().unwrap();
        list_value.values.push(one.into());
        std::mem::drop(list_value);
        heap.collect().ok();
        let list_value = list.as_mut::<List>().unwrap();
        assert_eq!(list_value.values.len(), 1);
    }

    #[test]
    fn string_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let string_handle = scope.create::<String>().unwrap();
        heap.collect().ok();
        let string_value = string_handle.as_ref::<String>().unwrap();
        assert_eq!(string_value, "");
    }

    #[test]
    fn take_string_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let string_handle = scope.take("Foo".to_string()).unwrap();
        heap.collect().ok();
        let string_value = string_handle.as_ref::<String>().unwrap();
        assert_eq!(string_value, "Foo");
    }

    #[test]
    fn list_push_string_twice_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let list = scope.create::<List>().unwrap();
        let string = scope.take("Foo".to_string()).unwrap();
        let list_value = list.as_mut::<List>().unwrap();
        list_value.values.push(string.into());
        list_value.values.push(string.into());
        std::mem::drop(list_value);
        heap.collect().ok();
        let list_value = list.as_mut::<List>().unwrap();
        assert_eq!(list_value.values.len(), 2);
        assert_eq!(
            scope
                .from_heap(&list_value.values[0])
                .as_ref::<String>()
                .unwrap(),
            "Foo"
        );
        assert_eq!(
            scope
                .from_heap(&list_value.values[1])
                .as_ref::<String>()
                .unwrap(),
            "Foo"
        );
        string.as_mut::<String>().unwrap().push_str("Bar");
        assert_eq!(
            scope
                .from_heap(&list_value.values[0])
                .as_ref::<String>()
                .unwrap(),
            "FooBar"
        );
    }

    #[test]
    fn map_insert_test() {
        let heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let map = scope.create::<Map>().unwrap();
        let foo = scope.take("Foo".to_string()).unwrap();
        let bar = scope.take("Bar".to_string()).unwrap();
        let map_value = map.as_mut::<Map>().unwrap();
        map_value.insert(foo.into(), bar.into());
        std::mem::drop(map_value);
        std::mem::drop(foo);
        std::mem::drop(bar);

        // Check if lookup works before collect.
        {
            let map_value = map.as_mut::<Map>().unwrap();
            let foo = scope.take("Foo".to_string()).unwrap();
            let bar = scope.from_heap(map_value.get(&foo.into()).unwrap());
            assert_eq!(bar.as_ref::<String>().unwrap(), "Bar");
        }

        heap.collect().ok();

        let map_value = map.as_mut::<Map>().unwrap();
        let foo = scope.take("Foo".to_string()).unwrap();
        let bar = scope.from_heap(map_value.get(&foo.into()).unwrap());
        assert_eq!(bar.as_ref::<String>().unwrap(), "Bar");
    }
}
