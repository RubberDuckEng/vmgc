use crate::heap::*;
use crate::object::ObjectType;

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

#[derive(Default, Hash)]
pub struct List {
    values: Vec<HeapHandle>,
}

impl HostObject for List {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for List {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        visitor.trace_handles(&self.values);
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
        let three_global = three.to_global();
        std::mem::drop(scope);

        let scope = HandleScope::new(&heap);
        let three = scope.from_global(&three_global);
        let three_value: f64 = three.try_into().unwrap();
        assert_eq!(3.0, three_value);
    }

    #[test]
    fn list_push_test() {
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let list = heap.allocate::<List>(&scope).unwrap();
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
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let string_handle = heap.allocate::<String>(&scope).unwrap();
        heap.collect().ok();
        let string_value = string_handle.as_ref::<String>().unwrap();
        assert_eq!(string_value, "");
    }

    #[test]
    fn take_string_test() {
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let string_handle = heap.take(&scope, "Foo".to_string()).unwrap();
        heap.collect().ok();
        let string_value = string_handle.as_ref::<String>().unwrap();
        assert_eq!(string_value, "Foo");
    }

    #[test]
    fn list_push_string_twice_test() {
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let list = heap.allocate::<List>(&scope).unwrap();
        let string = heap.take(&scope, "Foo".to_string()).unwrap();
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
        let mut heap = Heap::new(1000).unwrap();
        let scope = HandleScope::new(&heap);
        let map = heap.allocate::<Map>(&scope).unwrap();
        let foo = heap.take(&scope, "Foo".to_string()).unwrap();
        let bar = heap.take(&scope, "Bar".to_string()).unwrap();
        let map_value = map.as_mut::<Map>().unwrap();
        map_value.insert(foo.into(), bar.into());
        std::mem::drop(map_value);
        std::mem::drop(foo);
        std::mem::drop(bar);

        // Check if lookup works before collect.
        {
            let map_value = map.as_mut::<Map>().unwrap();
            let foo = heap.take(&scope, "Foo".to_string()).unwrap();
            let bar = scope.from_heap(map_value.get(&foo.into()).unwrap());
            assert_eq!(bar.as_ref::<String>().unwrap(), "Bar");
        }

        heap.collect().ok();

        let map_value = map.as_mut::<Map>().unwrap();
        let foo = heap.take(&scope, "Foo".to_string()).unwrap();
        let bar = scope.from_heap(map_value.get(&foo.into()).unwrap());
        assert_eq!(bar.as_ref::<String>().unwrap(), "Bar");
    }
}
