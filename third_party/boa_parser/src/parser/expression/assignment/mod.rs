//! Assignment operator parsing.
//!
//! More information:
//!  - [MDN documentation][mdn]
//!  - [ECMAScript specification][spec]
//!
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Assignment_Operators#Assignment
//! [spec]: https://tc39.es/ecma262/#sec-assignment-operators

mod arrow_function;
mod async_arrow_function;
mod conditional;
mod exponentiation;
mod r#yield;

use crate::{
    Error,
    lexer::{Error as LexError, InputElement, Token, TokenKind},
    parser::{
        AllowAwait, AllowIn, AllowYield, Cursor, OrAbrupt, ParseResult, TokenParser,
        expression::{
            FormalParameterListOrExpression,
            assignment::{
                arrow_function::{ArrowFunction, ConciseBody},
                async_arrow_function::{
                    AsyncArrowFunction, cover_call_parameters, finish_async_arrow_function,
                },
                conditional::ConditionalExpression,
                r#yield::YieldExpression,
            },
        },
        name_in_lexically_declared_names,
    },
    source::ReadChar,
};
use boa_ast::{
    Expression, Keyword, Punctuator, Span, Spanned,
    expression::operator::assign::{Assign, AssignOp, AssignTarget},
    operations::{ContainsSymbol, bound_names, contains, lexically_declared_names},
};
use boa_interner::{Interner, Sym};

pub(super) use exponentiation::ExponentiationExpression;

/// Assignment expression parsing.
///
/// This can be one of the following:
///
///  - [`ConditionalExpression`](../conditional_operator/struct.ConditionalExpression.html)
///  - `YieldExpression`
///  - [`ArrowFunction`](../../function/arrow_function/struct.ArrowFunction.html)
///  - `AsyncArrowFunction`
///  - [`LeftHandSideExpression`][lhs] `=` `AssignmentExpression`
///  - [`LeftHandSideExpression`][lhs] `AssignmentOperator` `AssignmentExpression`
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Assignment_Operators#Assignment
/// [spec]: https://tc39.es/ecma262/#prod-AssignmentExpression
/// [lhs]: ../lhs_expression/struct.LeftHandSideExpression.html
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct AssignmentExpression {
    allow_in: AllowIn,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl AssignmentExpression {
    /// Creates a new `AssignmentExpression` parser.
    pub(in crate::parser) fn new<I, Y, A>(allow_in: I, allow_yield: Y, allow_await: A) -> Self
    where
        I: Into<AllowIn>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_in: allow_in.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for AssignmentExpression
where
    R: ReadChar,
{
    type Output = Expression;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Expression> {
        cursor.set_goal(InputElement::RegExp);

        let mut async_call_head = false;
        match cursor.peek(0, interner).or_abrupt()?.kind() {
            // [+Yield]YieldExpression[?In, ?Await]
            TokenKind::Keyword((Keyword::Yield, _)) if self.allow_yield.0 => {
                return YieldExpression::new(self.allow_in, self.allow_await)
                    .parse(cursor, interner);
            }
            // ArrowFunction[?In, ?Yield, ?Await] -> ArrowParameters[?Yield, ?Await] -> BindingIdentifier[?Yield, ?Await]
            TokenKind::IdentifierName(_)
            | TokenKind::Keyword((
                Keyword::Yield | Keyword::Await | Keyword::Using | Keyword::Let,
                _,
            )) => {
                cursor.set_goal(InputElement::Div);

                // Because we already peeked the identifier token, there may be a line terminator before the identifier token.
                // In that case we have to skip an additional token on the next peek.
                let skip_n = if cursor.peek_is_line_terminator(0, interner).or_abrupt()? {
                    2
                } else {
                    1
                };
                if let Some(tok) = cursor.peek_no_skip_line_term(skip_n, interner)?
                    && tok.kind() == &TokenKind::Punctuator(Punctuator::Arrow)
                {
                    return ArrowFunction::new(self.allow_in, self.allow_yield, self.allow_await)
                        .parse(cursor, interner)
                        .map(Expression::ArrowFunction);
                }
            }
            //  AsyncArrowFunction[?In, ?Yield, ?Await]
            TokenKind::Keyword((Keyword::Async, false)) => {
                let skip_n = if cursor.peek_is_line_terminator(0, interner).or_abrupt()? {
                    2
                } else {
                    1
                };

                let peek_1 = cursor.peek(1, interner).or_abrupt()?.kind().clone();
                let line_terminator = cursor
                    .peek_is_line_terminator(skip_n, interner)
                    .or_abrupt()?;
                if line_terminator {
                    async_call_head = false;
                } else if matches!(peek_1, TokenKind::Punctuator(Punctuator::OpenParen)) {
                    async_call_head = true;
                } else if matches!(
                    peek_1,
                    TokenKind::IdentifierName(_)
                        | TokenKind::Keyword((
                            Keyword::Yield
                                | Keyword::Await
                                | Keyword::Of
                                | Keyword::Using
                                | Keyword::Let,
                            _
                        ))
                ) && matches!(
                    cursor.peek(2, interner).or_abrupt()?.kind(),
                    TokenKind::Punctuator(Punctuator::Arrow)
                ) {
                    return Ok(AsyncArrowFunction::new(self.allow_in, self.allow_yield)
                        .parse(cursor, interner)?
                        .into());
                }
            }
            _ => {}
        }

        cursor.set_goal(InputElement::Div);

        let peek_token = cursor.peek(0, interner).or_abrupt()?;
        let position = peek_token.span().start();
        let start_linear_span = peek_token.linear_span();
        let lhs = ConditionalExpression::new(self.allow_in, self.allow_yield, self.allow_await)
            .parse(cursor, interner)?;

        // If the left hand side is a parameter list, we must parse an arrow function.
        let mut lhs = match lhs {
            FormalParameterListOrExpression::FormalParameterList {
                fpl: parameters, ..
            } => {
                cursor.peek_expect_no_lineterminator(0, "arrow function", interner)?;

                cursor.expect(
                    TokenKind::Punctuator(Punctuator::Arrow),
                    "arrow function",
                    interner,
                )?;
                let arrow = cursor.arrow();
                cursor.set_arrow(true);
                let body = ConciseBody::new(self.allow_in).parse(cursor, interner)?;
                cursor.set_arrow(arrow);

                // Early Error: ArrowFormalParameters are UniqueFormalParameters.
                if parameters.has_duplicates() {
                    return Err(Error::lex(LexError::Syntax(
                        "Duplicate parameter name not allowed in this context".into(),
                        position,
                    )));
                }

                // Early Error: It is a Syntax Error if ArrowParameters Contains YieldExpression is true.
                if contains(&parameters, ContainsSymbol::YieldExpression) {
                    return Err(Error::lex(LexError::Syntax(
                        "Yield expression not allowed in this context".into(),
                        position,
                    )));
                }

                // Early Error: It is a Syntax Error if ArrowParameters Contains AwaitExpression is true.
                if contains(&parameters, ContainsSymbol::AwaitExpression) {
                    return Err(Error::lex(LexError::Syntax(
                        "Await expression not allowed in this context".into(),
                        position,
                    )));
                }

                // Early Error: It is a Syntax Error if ConciseBodyContainsUseStrict of ConciseBody is true
                // and IsSimpleParameterList of ArrowParameters is false.
                if body.strict() && !parameters.is_simple() {
                    return Err(Error::lex(LexError::Syntax(
                        "Illegal 'use strict' directive in function with non-simple parameter list"
                            .into(),
                        position,
                    )));
                }

                // The parameters are strict mode code as well, so they cannot bind "eval",
                // "arguments" or a word reserved in strict mode code.
                if body.strict() && contains(&parameters, ContainsSymbol::EvalOrArguments) {
                    return Err(Error::lex(LexError::Syntax(
                        "unexpected identifier 'eval' or 'arguments' in strict mode".into(),
                        position,
                    )));
                }

                // It is a Syntax Error if any element of the BoundNames of ArrowParameters
                // also occurs in the LexicallyDeclaredNames of ConciseBody.
                // https://tc39.es/ecma262/#sec-arrow-function-definitions-static-semantics-early-errors
                name_in_lexically_declared_names(
                    &bound_names(&parameters),
                    &lexically_declared_names(&body),
                    position,
                    interner,
                )?;

                let linear_pos_end = body.linear_pos_end();
                let linear_span = start_linear_span.union(linear_pos_end);

                let body_span_end = body.span().end();
                return Ok(boa_ast::function::ArrowFunction::new(
                    None,
                    parameters,
                    body,
                    linear_span,
                    Span::new(position, body_span_end),
                )
                .into());
            }
            FormalParameterListOrExpression::Expression(expression) => expression,
        };

        // CoverCallExpressionAndAsyncArrowHead: `async(...)` followed by `=>` is refined into an
        // AsyncArrowHead, anything else stays a call to a function named `async`.
        if async_call_head
            && let Expression::Call(call) = &lhs
            && let Expression::Identifier(callee) = call.function()
            && callee.sym() == Sym::ASYNC
            && callee.span().start() == position
            && cursor.peek(0, interner)?.map(Token::kind)
                == Some(&TokenKind::Punctuator(Punctuator::Arrow))
        {
            let trailing_comma = cursor.arguments_had_trailing_comma(call.span().end());
            let params = cover_call_parameters(call, trailing_comma, cursor.strict())?;
            return Ok(finish_async_arrow_function(
                cursor,
                interner,
                params,
                call.span().start(),
                callee.span(),
                start_linear_span,
                self.allow_in,
            )?
            .into());
        }

        // Review if we are trying to assign to an invalid left hand side expression.
        if let Some(tok) = cursor.peek(0, interner)?.cloned() {
            match tok.kind() {
                TokenKind::Punctuator(Punctuator::Assign) => {
                    cursor.advance(interner);
                    cursor.set_goal(InputElement::RegExp);

                    let lhs_name = if let Expression::Identifier(ident) = lhs {
                        Some(ident)
                    } else {
                        None
                    };

                    if let Some(target) = AssignTarget::from_expression(&lhs, cursor.strict()) {
                        let mut expr = self.parse(cursor, interner)?;
                        if let Some(ident) = lhs_name {
                            expr.set_anonymous_function_definition_name(&ident);
                        }
                        lhs = Assign::new(AssignOp::Assign, target, expr).into();
                    } else {
                        return Err(Error::lex(LexError::Syntax(
                            "Invalid left-hand side in assignment".into(),
                            tok.span().start(),
                        )));
                    }
                }
                TokenKind::Punctuator(p) if p.as_assign_op().is_some() => {
                    cursor.advance(interner);
                    let assignop = p.as_assign_op().expect("assignop disappeared");
                    let logical = matches!(
                        assignop,
                        AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce
                    );
                    if let Some(target) =
                        AssignTarget::from_expression_simple(&lhs, cursor.strict())
                            .filter(|target| !(logical && matches!(target, AssignTarget::Call(_))))
                    {
                        let mut rhs = self.parse(cursor, interner)?;
                        // NamedEvaluation applies only when the left-hand side is an
                        // `IdentifierRef`, so not to a parenthesized one: `(a) ??= function() {}`.
                        if logical && let Expression::Identifier(ident) = lhs {
                            rhs.set_anonymous_function_definition_name(&ident);
                        }
                        lhs = Assign::new(assignop, target, rhs).into();
                    } else {
                        return Err(Error::lex(LexError::Syntax(
                            "Invalid left-hand side in assignment".into(),
                            tok.span().start(),
                        )));
                    }
                }
                _ => {}
            }
        }

        Ok(lhs)
    }
}
