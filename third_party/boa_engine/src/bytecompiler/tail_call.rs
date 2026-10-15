use crate::bytecompiler::{ByteCompiler, Label};
use boa_ast::{
    Expression,
    expression::operator::binary::{BinaryOp, LogicalOp},
};

impl ByteCompiler<'_> {
    /// Returns `true` if a `return` statement compiled at this point is in tail position, so
    /// that a call it returns may replace the current frame.
    ///
    /// Tail calls are only defined in strict mode code, and never in generators or async
    /// functions. A `return` inside a `try` block, inside a `catch` block followed by a
    /// `finally` block, inside a `for-in`/`for-of` loop or inside a dispose scope is not in
    /// tail position either, since something still has to run once the call returns.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-isintailposition
    pub(crate) fn return_is_in_tail_position(&self) -> bool {
        self.strict()
            && !self.is_async()
            && !self.is_generator()
            && self
                .handlers
                .iter()
                .all(|handler| handler.end != Self::DUMMY_ADDRESS)
            && self.jump_info.iter().all(|info| {
                !info.iterator_loop() && (info.in_finally() || !info.is_try_with_finally_block())
            })
    }

    /// Compiles the expression of a `return` statement in tail position to the stack, so that
    /// the calls in its tail positions become tail calls.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-static-semantics-hascallintailposition
    pub(crate) fn compile_tail_expr_to_stack(&mut self, expr: &Expression) {
        match expr {
            Expression::Call(_) | Expression::TaggedTemplate(_) => {
                self.tail_call = true;
                self.compile_expr_to_stack(expr);
                self.tail_call = false;
            }
            Expression::Parenthesized(parenthesized) => {
                self.compile_tail_expr_to_stack(parenthesized.expression());
            }
            Expression::Conditional(conditional) => {
                let condition = self.register_allocator.alloc();
                self.compile_expr(conditional.condition(), &condition);
                let if_false = self.jump_if_false(&condition);
                self.register_allocator.dealloc(condition);
                self.compile_tail_expr_to_stack(conditional.if_true());
                let exit = self.jump();
                self.patch_jump(if_false);
                self.compile_tail_expr_to_stack(conditional.if_false());
                self.patch_jump(exit);
            }
            Expression::Binary(binary) => match binary.op() {
                BinaryOp::Comma => {
                    self.compile_expr_for_side_effects(binary.lhs());
                    self.compile_tail_expr_to_stack(binary.rhs());
                }
                BinaryOp::Logical(op) => {
                    let lhs = self.register_allocator.alloc();
                    self.compile_expr(binary.lhs(), &lhs);
                    let short_circuit = self.next_opcode_location();
                    match op {
                        LogicalOp::And => self
                            .bytecode
                            .emit_logical_and(Self::DUMMY_ADDRESS, lhs.variable()),
                        LogicalOp::Or => self
                            .bytecode
                            .emit_logical_or(Self::DUMMY_ADDRESS, lhs.variable()),
                        LogicalOp::Coalesce => self
                            .bytecode
                            .emit_coalesce(Self::DUMMY_ADDRESS, lhs.variable()),
                    }
                    self.compile_tail_expr_to_stack(binary.rhs());
                    let exit = self.jump();
                    self.patch_jump(Label {
                        index: short_circuit,
                    });
                    self.push_from_register(&lhs);
                    self.register_allocator.dealloc(lhs);
                    self.patch_jump(exit);
                }
                _ => self.compile_expr_to_stack(expr),
            },
            _ => self.compile_expr_to_stack(expr),
        }
    }
}
