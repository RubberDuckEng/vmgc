use std::alloc::{alloc, dealloc, Layout};
use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::marker::PhantomData;
// use std::ptr::NonNull;
use std::sync::Arc;

use crate::object::*;
use crate::tagged_ptr::TaggedPtr;
use crate::types::*;

#[derive(Debug)]
pub struct Space {
    layout: Layout,
    base: *mut u8,
    size: usize,
    next: *mut u8,
}

impl Space {
    fn new(size: usize) -> Result<Space, GCError> {
        // TODO: Should we allocte on a 4k boundary? Might have implications
        // for returning memory to the system.
        let layout = Layout::from_size_align(size, 0x1000).map_err(|_| GCError::NoSpace)?;
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err(GCError::OSOutOfMemory);
        }
        Ok(Space {
            layout,
            base: ptr,
            size,
            next: ptr,
        })
    }

    // TODO: The client should be able to specify the alignment.
    pub fn alloc(&mut self, size: usize) -> Result<*mut u8, GCError> {
        let allocated = self.used();
        if allocated.checked_add(size).ok_or(GCError::NoSpace)? > self.size {
            return Err(GCError::NoSpace);
        }
        let result = self.next;
        unsafe {
            self.next = result.add(size);
            result.write_bytes(0, size);
        }
        Ok(result)
    }

    fn used(&self) -> usize {
        unsafe { self.next.offset_from(self.base) as usize }
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        unsafe {
            self.base.write_bytes(0, self.used());
            dealloc(self.base, self.layout);
        }
    }
}

#[derive(Debug)]
struct HeapCell {
    ptr: TaggedPtr,
}

impl HeapCell {
    fn header(&self) -> Option<&mut ObjectHeader> {
        self.ptr.header()
    }
}

struct WeakCell {
    #[allow(dead_code)]
    value: Box<dyn Traceable>,
    ptr: TaggedPtr,
}

#[derive(Default)]
struct HeapInner {
    globals: Vec<Option<HeapCell>>,
    scopes: Vec<Vec<HeapCell>>,
    object_cells: Vec<Option<WeakCell>>,
    // TODO: Add Vec of weak pointers.
}

impl std::fmt::Debug for HeapInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeapInner").finish()
    }
}

pub struct ObjectVisitor {
    space: Space,
    queue: VecDeque<ObjectPtr>,
}

impl ObjectVisitor {
    fn new(space: Space) -> ObjectVisitor {
        ObjectVisitor {
            space,
            queue: VecDeque::default(),
        }
    }

    fn visit_header(&mut self, header: &mut ObjectHeader) -> ObjectPtr {
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

    pub fn visit(&mut self, handle: &mut HeapHandle) {
        if let Some(header) = handle.ptr.header() {
            handle.ptr = self.visit_header(header).into();
        }
    }
}

#[derive(Debug)]
pub struct Heap {
    // TODO: Add more generations.
    space: Space,
    inner: Arc<RefCell<HeapInner>>,
}

impl Heap {
    pub fn new(size: usize) -> Result<Heap, GCError> {
        let half_size = size / 2;
        Ok(Heap {
            space: Space::new(half_size)?,
            inner: Arc::new(RefCell::new(HeapInner::default())),
        })
    }

    pub fn used(&self) -> usize {
        self.space.used()
    }

    pub fn collect(&mut self) -> Result<(), GCError> {
        let mut visitor = ObjectVisitor::new(Space::new(self.space.size)?);
        let mut globals = vec![];
        std::mem::swap(&mut globals, &mut self.inner.borrow_mut().globals);
        for maybe_cell in globals.iter_mut() {
            if let Some(cell) = maybe_cell {
                if let Some(header) = cell.header() {
                    cell.ptr = visitor.visit_header(header).into();
                }
            }
        }
        std::mem::swap(&mut globals, &mut self.inner.borrow_mut().globals);

        while let Some(object_ptr) = visitor.queue.pop_front() {
            match object_ptr.header().object_type {
                ObjectType::Primitive => {}
                ObjectType::Host => {
                    let value_index = unsafe { *(object_ptr.addr() as *const usize) };
                    let mut inner = self.inner.borrow_mut();
                    let cell = inner.object_cells[value_index].as_mut().unwrap();
                    cell.value.trace(&mut visitor);
                }
            }
        }

        let mut indicies_to_finalize = vec![];

        let mut inner = self.inner.borrow_mut();
        for (i, maybe_cell) in inner.object_cells.iter_mut().enumerate() {
            if let Some(cell) = maybe_cell {
                if let Some(old_header) = cell.ptr.header() {
                    if let Some(new_header_ptr) = old_header.new_header_ptr {
                        cell.ptr = new_header_ptr.to_object_ptr().into();
                    } else {
                        // Finalize later in a less vulnerable place.
                        indicies_to_finalize.push(i);
                    }
                }
            }
        }
        // FIXME: Move finalization somewhere less vulnerable to avoid dropping
        // host objects calling back into GC code while we're collecting.
        for i in indicies_to_finalize {
            inner.object_cells[i] = None;
        }

        // TODO: Scan the Vec of weak pointers to see if any are pointing into
        // the from_space. If so, call their callbacks.
        std::mem::swap(&mut self.space, &mut visitor.space);
        Ok(())
    }

    fn allocate_object<T>(&mut self, object_type: ObjectType) -> Result<ObjectPtr, GCError> {
        let object_size = std::mem::size_of::<T>();
        let header = ObjectHeader::new(&mut self.space, object_size, object_type)?;
        Ok(header.as_ptr().to_object_ptr())
    }

    // This allocates a space of size_of(T), but does not take a T, so T
    // must be a heap-only type as it will never be finalized.
    pub fn allocate_global<T>(&mut self) -> Result<GlobalHandle, GCError> {
        let object_ptr = self.allocate_object::<T>(ObjectType::Primitive)?;
        Ok(self.alloc_handle::<T>(object_ptr.into()))
    }

    pub fn allocate_integer(&mut self, value: i32) -> GlobalHandle {
        self.alloc_handle::<i32>(value.into())
    }

    pub fn allocate_heap<T>(&mut self) -> Result<HeapHandle, GCError> {
        Ok(HeapHandle::new(
            self.allocate_object::<T>(ObjectType::Primitive)?.into(),
        ))
    }

    // Wraps a given TaggedPtr in a handle.
    fn alloc_handle(&self, ptr: TaggedPtr) -> GlobalHandle {
        let index = {
            // TODO: Scan for available cells.
            let mut inner = self.inner.borrow_mut();
            let index = inner.globals.len();
            inner.globals.push(Some(HeapCell { ptr }));
            index
        };
        GlobalHandle {
            inner: Arc::clone(&self.inner),
            index,
        }
    }

    // Maybe this is "wrap_object"?  It takes ownership of a Box<T> and
    // returns a Handle to the newly allocated Object in the VM's heap.
    pub fn alloc_host_object<T: Traceable>(
        &mut self,
        value: Box<T>,
    ) -> Result<GlobalHandle, GCError> {
        let ptr = self
            .allocate_object::<HostObject<T>>(ObjectType::Host)?
            .into();
        // TODO: This work should probably be done inside allocate where we have
        // access to the ObjectPtr.
        self.inner
            .borrow_mut()
            .object_cells
            .push(Some(WeakCell { value, ptr }));
        let index = self.inner.borrow_mut().object_cells.len() - 1;
        let mut handle = self.alloc_handle::<HostObject<T>>(ptr);
        handle.get_mut().value_index = index;
        // TODO: Register weak pointer for this host object whose callback uses
        // Box::from_raw to tell Rust to take ownership of the memory again.
        Ok(handle)
    }
}

// Rename as Root
#[derive(Debug)]
pub struct GlobalHandle {
    inner: Arc<RefCell<HeapInner>>,
    index: usize,
}

// FIXME: Drop the T, GlobalHandle is always to a Value.
impl GlobalHandle {
    // // TODO: These should actually return a HeapRef<T> that prevents GC while
    // // the reference is alive.
    // pub fn get(&self) -> &T {
    //     let inner = self.inner.borrow();
    //     let cell = inner.globals[self.index].as_ref().unwrap();
    //     // If this line panics, it's because the value isn't really an object.
    //     let object_ptr: ObjectPtr = cell.ptr.try_into().unwrap();
    //     unsafe { &*(object_ptr.addr() as *const T) }
    // }

    // // TODO: These should actually return a HeapRef<T> that prevents GC while
    // // the reference is alive.
    // pub fn get_mut(&mut self) -> &mut T {
    //     let inner = self.inner.borrow();
    //     let cell = inner.globals[self.index].as_ref().unwrap();
    //     // If this line panics, it's because the value isn't really an object.
    //     let object_ptr: ObjectPtr = cell.ptr.try_into().unwrap();
    //     unsafe { &mut *(object_ptr.addr() as *mut T) }
    // }

    pub fn ptr(&self) -> TaggedPtr {
        let inner = self.inner.borrow();
        let cell = inner.globals[self.index].as_ref().unwrap();
        cell.ptr
    }
}

impl Drop for GlobalHandle {
    fn drop(&mut self) {
        self.inner.borrow_mut().globals[self.index] = None;
    }
}

impl GlobalHandle {
    // pub fn get_object(&self) -> &T {
    //     let value_index = self.get().value_index;
    //     let inner = self.inner.borrow();
    //     let cell = inner.object_cells[value_index].as_ref().unwrap();
    //     let value = cell.value.as_ref();
    //     let ptr = value.as_any().downcast_ref().unwrap() as *const T;
    //     unsafe { &*ptr }
    // }
}

struct HandleScope {
    inner: Arc<RefCell<HeapInner>>,
    index: usize,
}

impl HandleScope {
    fn new(heap: &Heap) -> HandleScope {
        let mut inner = heap.inner.borrow_mut();
        let index = inner.scopes.len();
        inner.scopes.push(vec![]);
        HandleScope {
            inner: Arc::clone(&heap.inner),
            index,
        }
    }

    fn add(&self, ptr: TaggedPtr) -> usize {
        let mut inner = self.inner.borrow_mut();
        let cells = &mut inner.scopes[self.index];
        let index = cells.len();
        cells.push(HeapCell { ptr });
        index
    }

    fn get(&self, handle: &GlobalHandle) -> LocalHandle {
        LocalHandle::new(self, handle.ptr())
    }
}

impl Drop for HandleScope {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        inner.scopes.pop();
    }
}

// Is this really "ValueRef"?
enum Value<'a> {
    Number(i32),
    List(&'a List),
}

enum MutValue<'a> {
    Number(i32),
    List(&'a mut List),
}

struct LocalHandle<'a> {
    scope: &'a HandleScope,
    index: usize,
}

impl<'a> LocalHandle<'a> {
    fn new(scope: &'a HandleScope, ptr: TaggedPtr) -> LocalHandle<'a> {
        LocalHandle {
            scope: scope,
            index: scope.add(ptr),
        }
    }

    fn get(&self) -> Value {}

    fn get_mut(&self) -> MutValue {}
}

#[derive(Debug)]
#[repr(C)]
pub struct HostObject<T: Traceable> {
    phantom: PhantomData<T>,
    value_index: usize,
}

pub struct HeapHandle {
    ptr: TaggedPtr,
}

impl HeapHandle {
    fn new(ptr: TaggedPtr) -> HeapHandle {
        HeapHandle { ptr }
    }
}

pub trait AsAny: Any {
    fn as_any(&self) -> &dyn Any;
    fn get_type_name(&self) -> &'static str;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}
pub trait Traceable: AsAny + 'static {
    fn trace(&mut self, _visitor: &mut ObjectVisitor) {}
}
