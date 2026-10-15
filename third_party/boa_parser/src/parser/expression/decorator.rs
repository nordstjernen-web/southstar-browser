//! Decorator parsing.
//!
//! More information:
//!  - [Decorators proposal][spec]
//!
//! [spec]: https://arai-a.github.io/ecma262-compare/?pr=2417&id=sec-decorators

use crate::{
    Error,
    lexer::TokenKind,
    parser::{
        AllowAwait, AllowYield, Cursor, OrAbrupt, ParseResult, TokenParser,
        expression::{
            Expression, identifiers::IdentifierReference, left_hand_side::arguments::Arguments,
        },
    },
    source::ReadChar,
};
use boa_ast::{
    self as ast, Punctuator, Span, Spanned,
    expression::{
        Call, Identifier, Parenthesized,
        access::{PrivatePropertyAccess, SimplePropertyAccess},
    },
    function::PrivateName,
};
use boa_interner::{Interner, Sym};

/// Parses a `DecoratorList`: one or more decorators, returned in source order.
///
/// ```text
/// Decorator[Yield, Await] :
///   @ DecoratorMemberExpression[?Yield, ?Await]
///   @ DecoratorParenthesizedExpression[?Yield, ?Await]
///   @ DecoratorCallExpression[?Yield, ?Await]
///
/// DecoratorMemberExpression[Yield, Await] :
///   IdentifierReference[?Yield, ?Await]
///   DecoratorMemberExpression[?Yield, ?Await] . IdentifierName
///   DecoratorMemberExpression[?Yield, ?Await] . PrivateIdentifier
///
/// DecoratorParenthesizedExpression[Yield, Await] :
///   ( Expression[+In, ?Yield, ?Await] )
///
/// DecoratorCallExpression[Yield, Await] :
///   DecoratorMemberExpression[?Yield, ?Await] Arguments[?Yield, ?Await]
/// ```
///
/// More information:
///  - [Decorators proposal][spec]
///
/// [spec]: https://arai-a.github.io/ecma262-compare/?pr=2417&id=sec-decorators
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct DecoratorList {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl DecoratorList {
    /// Creates a new `DecoratorList` parser.
    pub(in crate::parser) fn new<Y, A>(allow_yield: Y, allow_await: A) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for DecoratorList
where
    R: ReadChar,
{
    type Output = Box<[ast::Expression]>;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let mut decorators = Vec::new();
        while cursor
            .next_if(TokenKind::Punctuator(Punctuator::At), interner)?
            .is_some()
        {
            let token = cursor.peek(0, interner).or_abrupt()?;
            if token.kind() == &TokenKind::Punctuator(Punctuator::OpenParen) {
                let start = token.span().start();
                cursor.advance(interner);
                let expression = Expression::new(true, self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?;
                let end = cursor
                    .expect(Punctuator::CloseParen, "decorator", interner)?
                    .span()
                    .end();
                decorators.push(Parenthesized::new(expression, Span::new(start, end)).into());
                continue;
            }

            let mut decorator: ast::Expression =
                IdentifierReference::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?
                    .into();
            while cursor
                .next_if(TokenKind::Punctuator(Punctuator::Dot), interner)?
                .is_some()
            {
                let token = cursor.next(interner).or_abrupt()?;
                let name = match token.kind() {
                    TokenKind::IdentifierName((name, _)) => *name,
                    TokenKind::Keyword((keyword, _)) => keyword.to_sym(),
                    TokenKind::BooleanLiteral((true, _)) => Sym::TRUE,
                    TokenKind::BooleanLiteral((false, _)) => Sym::FALSE,
                    TokenKind::NullLiteral(_) => Sym::NULL,
                    TokenKind::PrivateIdentifier(name) => {
                        let span = Span::new(decorator.span().start(), token.span().end());
                        decorator = ast::Expression::PropertyAccess(
                            PrivatePropertyAccess::new(
                                decorator,
                                PrivateName::new(*name, token.span()),
                                span,
                            )
                            .into(),
                        );
                        continue;
                    }
                    _ => {
                        return Err(Error::expected(
                            ["identifier".to_owned()],
                            token.to_string(interner),
                            token.span(),
                            "decorator",
                        ));
                    }
                };
                decorator = ast::Expression::PropertyAccess(
                    SimplePropertyAccess::new(decorator, Identifier::new(name, token.span()))
                        .into(),
                );
            }

            if cursor.peek(0, interner).or_abrupt()?.kind()
                == &TokenKind::Punctuator(Punctuator::OpenParen)
            {
                let (args, args_span) =
                    Arguments::new(self.allow_yield, self.allow_await).parse(cursor, interner)?;
                decorator = Call::new(decorator, args, args_span).into();
            }
            decorators.push(decorator);
        }
        Ok(decorators.into_boxed_slice())
    }
}
