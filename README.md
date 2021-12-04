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

# Blocking for wren integration
* Typesafe List and Map classes
* Starting List from a passed in vec?  Or filling from nulls?
* Example of free-standing Null?  (Passing around scope to make null seems silly.)
* Example of matching on Handle type
* Example of try_into from LocalHandle<()> to LocalHandle<T>
    if let Value::Fn(fn_obj) = const_value {
        fn_objs.push(fn_obj.clone());
    }
