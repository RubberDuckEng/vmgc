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
* Provide allocator for Heap?
* Consider making a HandleScope like AutoReleasePool?
* Give examples/docs to make clear which Handle types are nullable vs. not.
* Guidance on the prefered order of LocalHandle vs &LocalHandle vs &HeapHandle as passing types.
* How can we share more code (e.g. is_of_type, try_downcast, etc.) between LocalHandle and HeapHandle?
* Some sort of Guard and Temporary which refers back to a Guard.  Could just assert if you try to allocate?
* Map and List are footguns (they can't be easily extended, and you don't want to is_type_of<Map> or List depending on your setup) and should be separated out into some sort of easy example.