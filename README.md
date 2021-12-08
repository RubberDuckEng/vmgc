# vmgc
 A GC for a VM in Rust

Hopefully eventually for github.com/rubberduckeng/safe_wren

Inspired in part by https://rust-hosted-langs.github.io/book/introduction.html

# TODO
* Add tests for various allocation failures.
* Alignment for allocations
* Shrink object header
* Generational collection
* Thread safety
* Smarter size specification for Heap size (max size?)
* Provide allocator for Heap?
* Some sort of typed Handle?
* Consider making a HandleScope like AutoReleasePool?
* Consider having a NonNullHandle type?
* Collect on allocation
* Give examples/docs to make clear which Handle types are nullable vs. not.

# Blocking for wren integration
* Typesafe List and Map classes
* Starting List from a passed in vec?  Or filling from nulls?
* Example of free-standing Null?  (Passing around scope to make null seems silly.)
* Example of matching on Handle type
* Example of try_into from LocalHandle<()> to LocalHandle<T>
    if let Value::Fn(fn_obj) = const_value {
        fn_objs.push(fn_obj.clone());
    }
* Explore if FooHandle<Option<T>> could be null or T or if we need to use Option<FooHandle<T>>.
* Plan for try_into_num or try_into_foo, how to add such outside of this crate.  try_downcast<f64> dooesn't seem possible to define with try_downcast<T: HostObject> also defined.
* Plan for unified type for matching against LocalHandle types.
* Guidance on the prefered order of LocalHandle vs &LocalHandle vs &HeapHandle as passing types.
*     pub fn push<S>(&mut self, handle: HeapHandle<S>) , should be &HeapHandle right?
* How do we allocate a List and then set it on a rust struct?  Is that even safe? e.g.
struct VM {
    stack: List<()>,
}
fn foo() -> VM {
    let list = scope.create<List>().unwrap();
    list.add_stuff();
    VM {
        stack: list,?
    }
}