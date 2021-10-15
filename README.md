# vmgc
 A GC for a VM in Rust

Hopefully eventually for github.com/rubberduckeng/wren_rust

Inspired in part by https://rust-hosted-langs.github.io/book/introduction.html

# TODO
* Add Object Headers to objects (remove hardcoding for Number type)
* Add tests for various allocation failures.
* Interact with objects in heap (e.g. set values)
* Tagged Pointers
* Alignment for allocations
* Generational collection
* Thread safety