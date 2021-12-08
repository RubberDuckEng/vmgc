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
* Should as_ref() be renamed borrow()?  and as_mut() as borrow_mut()?
* Starting List from a passed in vec?  Or filling from nulls?
* Example of free-standing Null?  (Passing around scope to make null seems silly.)
* Example of matching on Handle type
* Explore if FooHandle<Option<T>> could be null or T or if we need to use Option<FooHandle<T>>.
* Plan for unified type for matching against LocalHandle types.
* Guidance on the prefered order of LocalHandle vs &LocalHandle vs &HeapHandle as passing types.
*     pub fn push<S>(&mut self, handle: HeapHandle<S>) , should be &HeapHandle right?
* How do we allocate a List and then set it on a rust struct?  Is that even safe? e.g.
struct ObjFn {
    fields: List<()>,
}
impl ObjFn {
    fn new(scope: &'a HandleScope<'_>) -> LocalHandle<'a, ObjFn> {
        let fields = scope.create<List>().unwrap();
        fields.add_stuff();
        scope.take(ObjFn {
            fields: scope.give(list),
        })
    }
}
