use crate::{
    bytecompiler::{
        BindingAccessOpcode, ByteCompiler, Register, ToJsString, jump_control::JumpControlInfoFlags,
    },
    vm::opcode::{BindingOpcode, IndexOperand, RegisterOperand},
};
use boa_ast::declaration::{Binding, VariableList};

impl ByteCompiler<'_> {
    /// Compiles `body` inside a scope that owns a dispose capability: the resources that the
    /// `using` and `await using` declarations of `body` add to it are disposed when the scope
    /// ends, whether normally, through `break`, `continue` or `return`, by a throw, or by a
    /// generator's `return()`.
    ///
    /// The scope is compiled like a `try` statement with a `finally` block that runs
    /// `DisposeResources`; an error thrown while disposing replaces the scope's completion.
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-disposeresources
    pub(crate) fn compile_dispose_scope<F>(&mut self, is_async: bool, use_expr: bool, body: F)
    where
        F: FnOnce(&mut Self),
    {
        let is_generator = self.is_generator();

        let capability = self.register_allocator.alloc();
        let has_error = self.register_allocator.alloc();
        let error = self.register_allocator.alloc();
        let generator_return = self.register_allocator.alloc();
        let finally_re_throw = self.register_allocator.alloc();
        let finally_jump_index = self.register_allocator.alloc();

        self.bytecode
            .emit_create_dispose_capability(capability.variable());
        self.bytecode.emit_store_false(has_error.variable());
        self.bytecode.emit_store_undefined(error.variable());
        self.bytecode.emit_store_false(generator_return.variable());
        self.bytecode.emit_store_true(finally_re_throw.variable());
        self.bytecode.emit_store_zero(finally_jump_index.variable());

        self.push_try_with_finally_control_info(&finally_re_throw, &finally_jump_index, use_expr);

        self.dispose_capabilities.push(capability.index());
        let try_handler = self.push_handler();
        body(self);
        self.dispose_capabilities.pop();

        self.bytecode.emit_store_false(finally_re_throw.variable());
        let finally = self.jump();
        self.patch_handler(try_handler);

        // A throw completion: remember the error. A generator's `return()` reaches here as an
        // empty exception, which `Exception` rethrows to the next handler.
        let return_handler = is_generator.then(|| self.push_handler());
        self.bytecode.emit_exception(error.variable());
        self.bytecode.emit_store_true(has_error.variable());
        if let Some(return_handler) = return_handler {
            let thrown = self.jump();
            self.patch_handler(return_handler);
            self.bytecode.emit_store_true(generator_return.variable());
            self.patch_jump(thrown);
        }
        self.patch_jump(finally);

        let finally_start = self.next_opcode_location();
        self.jump_info
            .last_mut()
            .expect("there should be a dispose scope")
            .flags |= JumpControlInfoFlags::IN_FINALLY;

        let accumulator = self.register_allocator.alloc();
        self.bytecode
            .emit_set_register_from_accumulator(accumulator.variable());
        if is_async {
            self.compile_async_dispose(&capability, &has_error, &error);
        } else {
            self.bytecode.emit_dispose_resources(
                capability.variable(),
                has_error.variable(),
                error.variable(),
            );
        }
        self.bytecode.emit_set_accumulator(accumulator.variable());
        self.register_allocator.dealloc(accumulator);

        // An error thrown by the scope or while disposing it.
        let no_error = self.jump_if_false(&has_error);
        self.bytecode.emit_throw(error.variable());
        self.patch_jump(no_error);

        // A generator `return()` continues once the resources are disposed.
        if is_generator {
            let not_returning = self.jump_if_false(&generator_return);
            self.bytecode.emit_re_throw();
            self.patch_jump(not_returning);
        }

        self.pop_try_with_finally_control_info(finally_start);

        self.register_allocator.dealloc(finally_jump_index);
        self.register_allocator.dealloc(finally_re_throw);
        self.register_allocator.dealloc(generator_return);
        self.register_allocator.dealloc(error);
        self.register_allocator.dealloc(has_error);
        self.register_allocator.dealloc(capability);
    }

    /// Emits the loop of an asynchronous `DisposeResources`: each step disposes resources up to
    /// the next value to await, and a rejection is folded into the completion.
    fn compile_async_dispose(
        &mut self,
        capability: &Register,
        has_error: &Register,
        error: &Register,
    ) {
        let value = self.register_allocator.alloc();
        let done = self.register_allocator.alloc();

        let loop_start = self.next_opcode_location();
        self.bytecode.emit_async_dispose_step(
            capability.variable(),
            has_error.variable(),
            error.variable(),
            value.variable(),
            done.variable(),
        );
        let exit = self.jump_if_true(&done);

        let await_handler = self.push_handler();
        self.bytecode.emit_await(value.variable());
        let resume_kind = self.register_allocator.alloc();
        self.pop_into_register(&resume_kind);
        self.pop_into_register(&value);
        self.generator_next(&value, &resume_kind);
        self.register_allocator.dealloc(resume_kind);
        self.bytecode.emit_jump(loop_start);
        self.patch_handler(await_handler);

        let rejection = self.register_allocator.alloc();
        self.bytecode.emit_exception(rejection.variable());
        self.bytecode.emit_add_dispose_error(
            has_error.variable(),
            error.variable(),
            rejection.variable(),
        );
        self.register_allocator.dealloc(rejection);
        self.bytecode.emit_jump(loop_start);

        self.patch_jump(exit);
        self.register_allocator.dealloc(done);
        self.register_allocator.dealloc(value);
    }

    /// Adds `value`, the value of a `using` (or `await using`) binding, to the dispose
    /// capability of the innermost dispose scope.
    pub(crate) fn emit_add_disposable_resource(&mut self, value: &Register, is_async: bool) {
        if let Some(&capability) = self.dispose_capabilities.last() {
            self.bytecode.emit_add_disposable_resource(
                RegisterOperand::new(capability),
                value.variable(),
                IndexOperand::from(is_async),
            );
        }
    }

    /// Compiles a `using` or `await using` declaration: each initializer is evaluated, added
    /// to the dispose capability and then bound.
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-let-const-using-and-await-using-declarations-runtime-semantics-evaluation
    pub(crate) fn compile_using_declaration(&mut self, decls: &VariableList, is_async: bool) {
        for variable in decls.as_ref() {
            let value = self.register_allocator.alloc();
            if let Some(init) = variable.init() {
                self.compile_expr(init, &value);
            } else {
                self.bytecode.emit_store_undefined(value.variable());
            }
            match variable.binding() {
                Binding::Identifier(ident) => {
                    self.emit_add_disposable_resource(&value, is_async);
                    let ident = ident.to_js_string(self.interner());
                    let binding = self.lexical_scope.get_identifier_reference(ident.clone());
                    if binding.local() {
                        let reg = self.register_allocator.alloc_persistent();
                        self.bytecode.emit_move(reg.variable(), value.variable());
                        self.local_binding_registers.insert(binding, reg.index());
                    } else {
                        let index = self.insert_binding(binding);
                        self.emit_binding_access(
                            BindingAccessOpcode::PutLexicalValue,
                            &index,
                            &value,
                        );
                    }
                }
                Binding::Pattern(pattern) => {
                    self.compile_declaration_pattern(pattern, BindingOpcode::InitLexical, &value);
                }
            }
            self.register_allocator.dealloc(value);
        }
    }
}
