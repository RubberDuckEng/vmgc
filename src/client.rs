use crate::heap::*;
use crate::object::ObjectType;

// 1. Create some sort of "Value" type?
// 2. Create tagged pointer
// Union
// Type_id
//

// #[repr(u16)]
// #[derive(Debug, Copy, Clone, PartialEq)]
// pub enum TypeId {
//     Num,
//     String,
//     List,
// }

// Write primitive functions
// add numbers -> immediate value
// add to a list -> host object with references (traced)
// add strings -> leaf node host object (no tracing)

// fn num_add(heap: &mut Heap, a: LocalHandle<'_>, b: LocalHandle<'_>) -> Result<LocalHandle, VMError> {
//     let result = a.as_num()? + b.as_num()?;
//     heap.allocate_local::<Number>(result)
// }

#[derive(Debug)]
pub struct HeapString {
    value: String,
}

impl Traceable for HeapString {
    fn trace(&mut self, _visitor: &mut ObjectVisitor) {}
}

#[derive(Default)]
pub struct List {
    values: Vec<HeapHandle>,
}

impl HostObject for List {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for List {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        for handle in self.values.iter_mut() {
            visitor.visit(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::Cell;
    use std::convert::TryInto;
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

    #[test]
    pub fn smoke_test() {
        let mut heap = Heap::new(1000).unwrap();
        assert_eq!(heap.used(), 0);
        let two = {
            let scope = HandleScope::new(&heap);
            let one = heap.allocate::<DropObject>(&scope).unwrap();
            let two = heap.allocate::<DropObject>(&scope).unwrap();
            std::mem::drop(one);
            two.to_global()
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
        let mut heap = Heap::new(1000).unwrap();
        let counter = Rc::new(Cell::new(0));
        let scope = HandleScope::new(&heap);

        let handle = heap.allocate::<DropObject>(&scope).unwrap();
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
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let handle = heap.allocate::<List>(&scope).unwrap();

        let list = handle.as_mut::<List>().unwrap();
        list.values
            .push(heap.allocate::<DropObject>(&scope).unwrap().into());
        list.values
            .push(heap.allocate::<DropObject>(&scope).unwrap().into());
        list.values
            .push(heap.allocate::<DropObject>(&scope).unwrap().into());
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
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);

        let a = heap.allocate_integer(&scope, 1);
        let b = heap.allocate_integer(&scope, 2);
        assert_eq!(0, heap.used());
        let a_value: i32 = a.ptr().try_into().unwrap();
        assert_eq!(1, a_value);
        let b_value: i32 = b.ptr().try_into().unwrap();
        assert_eq!(2, b_value);
    }

    #[test]
    fn add_i32_test() {
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let one = heap.allocate_integer(&scope, 1);
        let two = heap.allocate_integer(&scope, 2);
        let one_value: i32 = one.try_into().unwrap();
        assert_eq!(1, one_value);
        let two_value: i32 = two.try_into().unwrap();
        assert_eq!(2, two_value);
        let three_value = one_value + two_value;
        let three = heap.allocate_integer(&scope, three_value);
        let three_global = three.to_global();
        std::mem::drop(scope);

        let scope = HandleScope::new(&heap);
        let three = scope.get(&three_global);
        let three_value: i32 = three.try_into().unwrap();
        assert_eq!(3, three_value);
    }

    #[test]
    fn list_push_test() {
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let list = heap.allocate::<List>(&scope).unwrap();
        let one = heap.allocate_integer(&scope, 1);
        let list_value = list.as_mut::<List>().unwrap();
        list_value.values.push(one.into());
        std::mem::drop(list_value);
        heap.collect().ok();
        let list_value = list.as_mut::<List>().unwrap();
        assert_eq!(list_value.values.len(), 1);
    }
}
