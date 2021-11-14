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

impl Traceable for HeapString {}

#[derive(Default)]
pub struct List {
    values: Vec<HeapHandle>,
}

impl HostObject for List {
    const TYPE_ID: ObjectType = ObjectType::List;
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

    // use crate::object::HEADER_SIZE;

    #[derive(Default)]
    struct DropObject {
        counter: Rc<Cell<u32>>,
    }

    impl HostObject for DropObject {
        const TYPE_ID: ObjectType = ObjectType::Test;
    }

    impl Traceable for DropObject {}

    impl Drop for DropObject {
        fn drop(&mut self) {
            let counter = self.counter.get();
            self.counter.set(counter + 1);
        }
    }

    // struct HostNumber {
    //     value: u64,
    // }

    // impl Traceable for HostNumber {}

    // #[test]
    // pub fn smoke_test() {
    //     let mut heap = Heap::new(1000).unwrap();
    //     assert_eq!(heap.used(), 0);
    //     let one = heap.allocate_global::<Number>().unwrap();
    //     let two = heap.allocate_global::<Number>().unwrap();
    //     std::mem::drop(one);
    //     assert_eq!(
    //         heap.used(),
    //         (HEADER_SIZE + std::mem::size_of::<Number>()) * 2
    //     );
    //     heap.collect().ok();
    //     assert_eq!(heap.used(), HEADER_SIZE + std::mem::size_of::<Number>());
    //     std::mem::drop(two);
    // }

    // #[test]
    // fn finalizer_test() {
    //     let mut heap = Heap::new(1000).unwrap();
    //     let counter = Rc::new(Cell::new(0));
    //     let host = Box::new(DropObject {
    //         counter: Rc::clone(&counter),
    //     });

    //     let handle = heap.alloc_host_object(host);
    //     std::mem::drop(handle);
    //     assert_eq!(0u32, counter.get());
    //     heap.collect().ok();
    //     assert_eq!(1u32, counter.get());
    // }

    // #[test]
    // fn number_value_test() {
    //     let mut heap = Heap::new(1000).unwrap();
    //     let one = heap.allocate_global::<Number>().unwrap();
    //     let two = heap.allocate_global::<Number>().unwrap();
    //     one.get_mut().value = 1;
    //     two.get_mut().value = 2;
    //     assert_eq!(1, one.get().value);
    //     assert_eq!(2, two.get().value);
    //     heap.collect().ok();
    //     assert_eq!(1, one.get().value);
    //     assert_eq!(2, two.get().value);
    // }

    // #[test]
    // fn number_as_host_object_test() {
    //     let mut heap = Heap::new(1000).unwrap();

    //     let num = HostNumber { value: 1 };
    //     let number = Box::new(num);
    //     let handle = heap.alloc_host_object(number).unwrap();
    //     assert_eq!(1, handle.get_object().value);
    //     std::mem::drop(handle);
    // }

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
        let three_global = three.to_global(&heap);
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
    }
}
