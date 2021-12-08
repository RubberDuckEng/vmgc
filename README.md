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
* Guidance on the prefered order of LocalHandle vs &LocalHandle vs &HeapHandle as passing types.
* Should as_ref() be renamed borrow()?  and as_mut() as borrow_mut()?
* Explore if FooHandle<Option<T>> could be null or T or if we need to use Option<FooHandle<T>>.

# Blocking for wren integration
