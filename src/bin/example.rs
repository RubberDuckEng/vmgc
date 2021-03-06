use std::convert::TryInto;
use vmgc::*;

// Holds the heap and the stack.
struct VM {
    heap: Heap,
    stack: GlobalHandle<Stack>,
}

#[derive(Default)]
struct Stack {
    pending_result: HeapHandle<()>,
    values: List<()>,
}

// type DynamicHeapHandle = HeapHandle<()>;

// struct TypedHandle<HandleType, ValueType> {
//     handle: HandleType,
//     _phantom: PhantomData<ValueType>,
// }

impl HostObject for Stack {
    const TYPE_ID: ObjectType = ObjectType::Host;
}

impl Traceable for Stack {
    fn trace(&mut self, visitor: &mut ObjectVisitor) {
        self.values.trace(visitor);
        self.pending_result.trace(visitor);
    }
}

fn init() -> VM {
    let heap = Heap::new(1000).unwrap();
    let stack = {
        let scope = HandleScope::new(&heap);
        GlobalHandle::from(scope.create::<Stack>().unwrap())
    };
    VM { stack, heap }
}

fn num_add(_vm: &VM, args: &[HeapHandle<()>], out: &mut HeapHandle<()>) -> Result<(), GCError> {
    let lhs: f64 = args[0].ptr().try_into()?;
    let rhs: f64 = args[1].ptr().try_into()?;
    *out = HeapHandle::new((lhs + rhs).into());
    Ok(())
}

fn num_is_nan(_vm: &VM, args: &[HeapHandle<()>], out: &mut HeapHandle<()>) -> Result<(), GCError> {
    let num: f64 = args[0].ptr().try_into()?;
    *out = HeapHandle::new(num.is_nan().into());
    Ok(())
}

fn main() {
    let vm = init();

    // push two numbers on the stack
    {
        let scope = HandleScope::new(&vm.heap);
        let stack = scope.as_mut(&vm.stack);
        stack.values.push(scope.create_num(1.0).into());
        stack.values.push(scope.create_num(2.0).into());
    }
    vm.heap.collect().ok();

    // call the add function
    {
        let scope = HandleScope::new(&vm.heap);
        let stack = scope.as_mut(&vm.stack);

        num_add(&vm, &stack.values[..], &mut stack.pending_result).ok();

        stack.values.truncate(0);
        stack.values.push(stack.pending_result.take());
    }

    vm.heap.collect().ok();
    // expect a single number on the stack.
    {
        let scope = HandleScope::new(&vm.heap);
        let stack = scope.as_mut(&vm.stack);

        let result: f64 = stack.values[0].ptr().try_into().unwrap();
        println!("1 + 2 = {}", result);
    }

    vm.heap.collect().ok();
    // call is_nan function
    {
        let scope = HandleScope::new(&vm.heap);
        let stack = scope.as_mut(&vm.stack);

        num_is_nan(&vm, &stack.values[..], &mut stack.pending_result).ok();

        stack.values.truncate(0);
        stack.values.push(stack.pending_result.take());
    }

    // expect a single bool (false) on the stack.
    {
        let scope = HandleScope::new(&vm.heap);
        let stack = scope.as_mut(&vm.stack);

        let result: bool = stack.values[0].ptr().try_into().unwrap();
        println!("3.is_nan = {}", result);
    }
}

// Need to explain what each use of:
// Value::String(foo)
//  -- This probably becomes Value::newString(scope, string)?
//  -- fn newString(scope, string) -> Handle?
// match on Value types
//  -- Is value just TaggedNum/TaggedPtr?
//  -- Is this a match on value.type()?
// Passing a Value into a function
// -- either &HeapHandle (ref to somewhere held tracable)
// -- or LocalHandle (temporarily tracable by its HandleScope)
// -- If not worried about perf, chose LocalHandle.
// etc.
// Maps to in wren.
// Or does Value just no longer exist and we use Handles instead?

// safe_wren Value types:
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
