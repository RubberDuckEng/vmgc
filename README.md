# vmgc
 A GC for a VM in Rust

Hopefully eventually for github.com/rubberduckeng/safe_wren

Inspired in part by https://rust-hosted-langs.github.io/book/introduction.html

# TODO
* Add tests for various allocation failures.
* Alignment for allocations
* Generational collection
* Thread safety
* Smarter size specification for Heap size (max size?)
* Provide allocator for Heap?
* Some sort of typed Handle?
* Consider making a HandleScope like AutoReleasePool?

# Blocking for wren integration
* A map type which takes handles.
* Examples of allocating every type including Null, Num, String, List, Map
* Example of passing LocalHandle
* Example of returning some type of handle?

