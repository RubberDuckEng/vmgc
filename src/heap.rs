// #![allow(dead_code)]

use std::alloc::{alloc, dealloc, Layout};
use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::{From, TryInto};
use std::marker::PhantomData;
// use std::ptr::NonNull;
use std::sync::Arc;

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

#[derive(Debug)]
struct Space {
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
    fn alloc(&mut self, size: usize) -> Result<*mut u8, GCError> {
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

#[derive(Copy, Clone)]
pub union TaggedPtr {
    tag: usize,
    number: isize,
    object: usize, // FIXME: Should be NonNull<T>
}

impl TaggedPtr {
    fn header(&self) -> Option<&mut ObjectHeader> {
        (*self).try_into().ok().map(ObjectHeader::from_object_ptr)
    }
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
    type Error = GCError;
    fn try_into(self) -> Result<i32, GCError> {
        unsafe {
            match self.tag & TAG_MASK {
                TAG_NUMBER => Ok((self.number >> 2) as i32),
                _ => Err(GCError::TypeError),
            }
        }
    }
}

impl From<ObjectPtr> for TaggedPtr {
    fn from(ptr: ObjectPtr) -> TaggedPtr {
        unsafe {
            TaggedPtr {
                object: std::mem::transmute::<ObjectPtr, usize>(ptr) | TAG_OBJECT,
            }
        }
    }
}

impl TryInto<ObjectPtr> for TaggedPtr {
    type Error = GCError;
    fn try_into(self) -> Result<ObjectPtr, GCError> {
        unsafe {
            match self.tag & TAG_MASK {
                TAG_OBJECT => Ok(std::mem::transmute::<usize, ObjectPtr>(
                    self.object & PTR_MASK,
                )),
                _ => Err(GCError::TypeError),
            }
        }
    }
}

impl std::fmt::Debug for TaggedPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaggedPtr").finish()
    }
}

// ObjectPtr could have a generation number, and thus we could know
// if we ever forgot one between generations (and thus was invalid).
#[derive(Copy, Clone, Debug)]
struct ObjectPtr(*mut u8);

impl ObjectPtr {
    fn new(addr: *mut u8) -> ObjectPtr {
        ObjectPtr(addr)
    }

    fn addr(&self) -> *mut u8 {
        self.0
    }

    fn to_header_ptr(&self) -> HeaderPtr {
        HeaderPtr::new(unsafe { self.addr().sub(HEADER_SIZE) })
    }

    fn header(&self) -> &mut ObjectHeader {
        ObjectHeader::from_object_ptr(*self)
    }
}

#[derive(Copy, Clone, Debug)]
struct HeaderPtr(*mut u8);

impl HeaderPtr {
    fn new(addr: *mut u8) -> HeaderPtr {
        HeaderPtr(addr)
    }

    fn addr(&self) -> *mut u8 {
        self.0
    }

    fn to_object_ptr(&self) -> ObjectPtr {
        ObjectPtr::new(unsafe { self.addr().add(HEADER_SIZE) })
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
    pub fn allocate_global<T>(&mut self) -> Result<GlobalHandle<T>, GCError> {
        let object_ptr = self.allocate_object::<T>(ObjectType::Primitive)?;
        Ok(self.alloc_handle::<T>(object_ptr.into()))
    }

    pub fn allocate_integer(&mut self, value: i32) -> GlobalHandle<i32> {
        self.alloc_handle::<i32>(value.into())
    }

    pub fn allocate_heap<T>(&mut self) -> Result<HeapHandle, GCError> {
        Ok(HeapHandle::new(
            self.allocate_object::<T>(ObjectType::Primitive)?.into(),
        ))
    }

    // Wraps a given TaggedPtr in a handle.
    fn alloc_handle<T>(&self, ptr: TaggedPtr) -> GlobalHandle<T> {
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
            phantom: PhantomData::<T>::default(),
        }
    }

    // Maybe this is "wrap_object"?  It takes ownership of a Box<T> and
    // returns a Handle to the newly allocated Object in the VM's heap.
    pub fn alloc_host_object<T: Traceable>(
        &mut self,
        value: Box<T>,
    ) -> Result<GlobalHandle<HostObject<T>>, GCError> {
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

#[derive(Debug)]
pub struct GlobalHandle<T> {
    inner: Arc<RefCell<HeapInner>>,
    index: usize,
    phantom: PhantomData<T>,
}

// FIXME: Drop the T, GlobalHandle is always to a Value.
impl<T> GlobalHandle<T> {
    // TODO: These should actually return a HeapRef<T> that prevents GC while
    // the reference is alive.
    pub fn get(&self) -> &T {
        let inner = self.inner.borrow();
        let cell = inner.globals[self.index].as_ref().unwrap();
        // If this line panics, it's because the value isn't really an object.
        let object_ptr: ObjectPtr = cell.ptr.try_into().unwrap();
        unsafe { &*(object_ptr.addr() as *const T) }
    }

    // TODO: These should actually return a HeapRef<T> that prevents GC while
    // the reference is alive.
    pub fn get_mut(&mut self) -> &mut T {
        let inner = self.inner.borrow();
        let cell = inner.globals[self.index].as_ref().unwrap();
        // If this line panics, it's because the value isn't really an object.
        let object_ptr: ObjectPtr = cell.ptr.try_into().unwrap();
        unsafe { &mut *(object_ptr.addr() as *mut T) }
    }

    // TODO: Remove once we have a Value enum.
    #[cfg(test)]
    fn get_tagged_ptr(&self) -> TaggedPtr {
        let inner = self.inner.borrow();
        let cell = inner.globals[self.index].as_ref().unwrap();
        cell.ptr
    }
}

impl<T> Drop for GlobalHandle<T> {
    fn drop(&mut self) {
        self.inner.borrow_mut().globals[self.index] = None;
    }
}

impl<T: Traceable> GlobalHandle<HostObject<T>> {
    pub fn get_object(&self) -> &T {
        let value_index = self.get().value_index;
        let inner = self.inner.borrow();
        let cell = inner.object_cells[value_index].as_ref().unwrap();
        let value = cell.value.as_ref();
        let ptr = value.as_any().downcast_ref().unwrap() as *const T;
        unsafe { &*ptr }
    }
}

// TODO: Add HandleScope and LocalHandle.

#[derive(Debug)]
#[repr(C)]
enum ObjectType {
    Primitive,
    Host,
}

#[derive(Debug)]
#[repr(C)]
struct ObjectHeader {
    object_size: usize,
    object_type: ObjectType,

    // When we move the object to the new space, we'll record in this field
    // where we moved it to.
    new_header_ptr: Option<HeaderPtr>,
}

pub const HEADER_SIZE: usize = std::mem::size_of::<ObjectHeader>();

impl ObjectHeader {
    fn new<'a>(
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

    fn from_header_ptr<'a>(header_ptr: HeaderPtr) -> &'a mut ObjectHeader {
        unsafe { &mut *(header_ptr.addr() as *mut ObjectHeader) }
    }

    fn from_object_ptr<'a>(object_ptr: ObjectPtr) -> &'a mut ObjectHeader {
        Self::from_header_ptr(object_ptr.to_header_ptr())
    }

    fn alloc_size(&self) -> usize {
        HEADER_SIZE + self.object_size
    }

    fn as_ptr(&mut self) -> HeaderPtr {
        HeaderPtr::new(self as *mut ObjectHeader as *mut u8)
    }
}

// FIXME: Number does not belong heap.rs.
#[derive(Debug)]
#[repr(C)]
pub struct Number {
    pub value: u64,
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

#[derive(Default)]
pub struct NumberList {
    values: Vec<HeapHandle>,
}

impl Traceable for NumberList {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        for handle in self.values.iter_mut() {
            visitor.visit(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::heap::*;

    struct DropObject {
        counter: Rc<Cell<u32>>,
    }

    impl Traceable for DropObject {}

    impl Drop for DropObject {
        fn drop(&mut self) {
            let counter = self.counter.get();
            self.counter.set(counter + 1);
        }
    }

    struct HostNumber {
        value: u64,
    }

    impl Traceable for HostNumber {}

    #[test]
    pub fn smoke_test() {
        let mut heap = Heap::new(1000).unwrap();
        assert_eq!(heap.used(), 0);
        let one = heap.allocate_global::<Number>().unwrap();
        let two = heap.allocate_global::<Number>().unwrap();
        std::mem::drop(one);
        assert_eq!(
            heap.used(),
            (HEADER_SIZE + std::mem::size_of::<Number>()) * 2
        );
        heap.collect().ok();
        assert_eq!(heap.used(), HEADER_SIZE + std::mem::size_of::<Number>());
        std::mem::drop(two);
    }

    #[test]
    fn finalizer_test() {
        let mut heap = Heap::new(1000).unwrap();
        let counter = Rc::new(Cell::new(0));
        let host = Box::new(DropObject {
            counter: Rc::clone(&counter),
        });

        let handle = heap.alloc_host_object(host);
        std::mem::drop(handle);
        assert_eq!(0u32, counter.get());
        heap.collect().ok();
        assert_eq!(1u32, counter.get());
    }

    #[test]
    fn number_value_test() {
        let mut heap = Heap::new(1000).unwrap();
        let mut one = heap.allocate_global::<Number>().unwrap();
        let mut two = heap.allocate_global::<Number>().unwrap();
        one.get_mut().value = 1;
        two.get_mut().value = 2;
        assert_eq!(1, one.get().value);
        assert_eq!(2, two.get().value);
        heap.collect().ok();
        assert_eq!(1, one.get().value);
        assert_eq!(2, two.get().value);
    }

    #[test]
    fn number_as_host_object_test() {
        let mut heap = Heap::new(1000).unwrap();

        let num = HostNumber { value: 1 };
        let number = Box::new(num);
        let handle = heap.alloc_host_object(number).unwrap();
        assert_eq!(1, handle.get_object().value);
        std::mem::drop(handle);
    }

    #[test]
    fn tracing_test() {
        let mut heap = Heap::new(1000).unwrap();
        let mut list = Box::new(NumberList::default());
        list.values.push(heap.allocate_heap::<Number>().unwrap());
        list.values.push(heap.allocate_heap::<Number>().unwrap());
        list.values.push(heap.allocate_heap::<Number>().unwrap());
        let handle = heap.alloc_host_object(list).unwrap();
        let used = heap.used();
        heap.collect().unwrap();
        assert_eq!(used, heap.used());
        std::mem::drop(handle);
        assert_eq!(used, heap.used());
        heap.collect().unwrap();
        assert_eq!(0, heap.used());
    }

    #[test]
    fn tagged_num_test() {
        let mut heap = Heap::new(1000).unwrap();
        let a = heap.allocate_integer(1);
        let b = heap.allocate_integer(2);
        assert_eq!(0, heap.used());
        let a_value: i32 = a.get_tagged_ptr().try_into().unwrap();
        assert_eq!(1, a_value);
        let b_value: i32 = b.get_tagged_ptr().try_into().unwrap();
        assert_eq!(2, b_value);
    }
}
