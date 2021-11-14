use std::convert::TryInto;
use vmgc::heap::{
    GlobalHandle, HandleScope, Heap, HeapHandle, HostObject, ObjectVisitor, Traceable,
};
use vmgc::object::ObjectType;
use vmgc::types::GCError;

// Holds the heap and the stack.
struct VM {
    heap: Heap,
    stack: GlobalHandle,
}

#[derive(Default)]
struct Stack {
    pending_result: HeapHandle,
    values: Vec<HeapHandle>,
}

impl HostObject for Stack {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for Stack {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        for value in self.values.iter_mut() {
            visitor.visit(value);
        }
        visitor.visit(&mut self.pending_result);
    }
}

fn init() -> VM {
    let mut heap = Heap::new(1000).unwrap();
    let scope = HandleScope::new(&heap);
    VM {
        stack: heap.allocate::<Stack>(&scope).unwrap().to_global(),
        heap,
    }
}

fn num_add(_vm: &mut VM, args: &[HeapHandle], out: &mut HeapHandle) -> Result<(), GCError> {
    let lhs: i32 = args[0].ptr.try_into()?;
    let rhs: i32 = args[1].ptr.try_into()?;
    out.ptr = (lhs + rhs).into();
    Ok(())
}

fn main() {
    let mut vm = init();

    // push two numbers on the stack
    {
        let scope = HandleScope::new(&vm.heap);
        let stack_handle = scope.get(&vm.stack);
        let stack = stack_handle.as_mut::<Stack>().unwrap();

        stack.values.push(HeapHandle::new(1.into()));
        stack.values.push(HeapHandle::new(2.into()));
    }
    vm.heap.collect().ok();

    // call the add function
    {
        let scope = HandleScope::new(&vm.heap);
        let stack_handle = scope.get(&vm.stack);
        let stack = stack_handle.as_mut::<Stack>().unwrap();

        num_add(&mut vm, &stack.values[..2], &mut stack.pending_result).ok();

        stack.values.truncate(0);
        stack.values.push(stack.pending_result.take());
    }

    vm.heap.collect().ok();
    // expect a single number on the stack.
    {
        let scope = HandleScope::new(&vm.heap);
        let stack_handle = scope.get(&vm.stack);
        let stack = stack_handle.as_mut::<Stack>().unwrap();

        let result: i32 = stack.values[0].ptr.try_into().unwrap();
        println!("1 + 2 = {}", result);
    }
}

// Null,
// Num(f64),
// Boolean(bool),
// String(Rc<String>),
// // Split these off and replace with Object(Handle<dyn Obj>)
// Class(Handle<ObjClass>),
// Range(Handle<ObjRange>),
// Fn(Handle<ObjFn>),
// Closure(Handle<ObjClosure>),
// List(Handle<ObjList>),
// Map(Handle<ObjMap>),
// Fiber(Handle<ObjFiber>),
// Instance(Handle<ObjInstance>),
// Foreign(Handle<ObjForeign>),
