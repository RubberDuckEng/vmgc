extern crate vmgc;

use vmgc::heap::*;

fn main() {
    let mut heap = Heap::new(1000).unwrap();
    assert_eq!(heap.used(), 0);
    let one = heap.allocate::<Number>().unwrap();
    assert_eq!(0, one.get().value);
    let two = heap.allocate::<Number>().unwrap();
    std::mem::drop(one);
    heap.collect();
    std::mem::drop(two);
}
