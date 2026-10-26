use crate::bytecompiler::ByteCompiler;
use boa_ast::{
    Statement,
    statement::{Labelled, LabelledItem},
};

impl ByteCompiler<'_> {
    /// Compile a [`Labelled`] `boa_ast` node
    pub(crate) fn compile_labelled(&mut self, labelled: &Labelled, use_expr: bool) {
        let labelled_loc = self.next_opcode_location();
        self.push_labelled_control_info(labelled.label(), labelled_loc, use_expr);

        match labelled.item() {
            LabelledItem::Statement(stmt) => match stmt {
                Statement::ForLoop(for_loop) => {
                    self.compile_for_loop(for_loop, Some(labelled.label()), use_expr);
                }
                Statement::ForInLoop(for_in_loop) => {
                    self.compile_for_in_loop(for_in_loop, Some(labelled.label()), use_expr);
                }
                Statement::ForOfLoop(for_of_loop) => {
                    self.compile_for_of_loop(for_of_loop, Some(labelled.label()), use_expr);
                }
                Statement::WhileLoop(while_loop) => {
                    self.compile_while_loop(while_loop, Some(labelled.label()), use_expr);
                }
                Statement::DoWhileLoop(do_while_loop) => {
                    self.compile_do_while_loop(do_while_loop, Some(labelled.label()), use_expr);
                }
                stmt => self.compile_stmt(stmt, use_expr, true),
            },
            // The function was instantiated with the declarations of the enclosing block or
            // body, so evaluating it only performs the Annex B copy of a block function.
            #[cfg(feature = "annex-b")]
            LabelledItem::FunctionDeclaration(f) => self.compile_annex_b_block_function(f),
            #[cfg(not(feature = "annex-b"))]
            LabelledItem::FunctionDeclaration(_) => {}
        }

        self.pop_labelled_control_info();
    }
}
