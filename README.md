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
* Example of passing LocalHandle
    fn emit_constant(ctx: &mut ParseContext, value: LocalHandle) -> Result<(), WrenError> {
        let index = ensure_constant(ctx, value)?;
        emit(ctx, Ops::Constant(index));
        Ok(())
    }
    fn store_this(&self, frame: &CallFrame, value: LocalHandle) {
        self.store_local(frame, 0, value)
    }
    pub(crate) fn new(
        vm: &VM,
        scope: &'a HandleScope,
        closure: LocalHandle<'_, ObjClosure>,
        run_source: FiberRunSource,
    ) -> LocalHandle<'a, ObjFiber> {
    }
* Example of saving a handle passed into you, or copying and returning a new handle.
* Example of returning some type of handle?
    pub(crate) fn variable_by_name(&self, scope: &'a HandleScope, name: &str) -> LocalHandle<'a> {
        self.lookup_symbol(name)
            .map(|index| self.variables[index as usize].clone())
    }
    fn as_try_return_value(&self, scope: &'a HandleScope) -> LocalHandle<'a> {
        match self {
            VMError::Error(string) => scope.from_str(string),
            VMError::FiberAbort(value) => scope.from_heap(value),
        }
    }
