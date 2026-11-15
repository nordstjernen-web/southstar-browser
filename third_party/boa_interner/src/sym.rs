use boa_gc::{Finalize, Trace, empty_trace};
use core::num::NonZeroUsize;

/// The string symbol type for Boa.
///
/// This symbol type is internally a `NonZeroUsize`, which makes it pointer-width in size and it's
/// optimized so that it can occupy 1 pointer width even in an `Option` type.
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Finalize)]
pub struct Sym {
    value: NonZeroUsize,
}

// SAFETY: `NonZeroUsize` is a constrained `usize`, and all primitive types don't need to be traced
// by the garbage collector.
unsafe impl Trace for Sym {
    empty_trace!();
}

impl Sym {
    /// Creates a new [`Sym`] from the provided `value`, or returns `None` if `index` is zero.
    pub(super) fn new(value: usize) -> Option<Self> {
        NonZeroUsize::new(value).map(|value| Self { value })
    }

    /// Creates a new [`Sym`] from the provided `value`, without checking if `value` is not zero
    ///
    /// # Safety
    ///
    /// `value` must not be zero.
    pub(super) const unsafe fn new_unchecked(value: usize) -> Self {
        Self {
            value:
            // SAFETY: The caller must ensure the invariants of the function.
            unsafe {
                NonZeroUsize::new_unchecked(value)
            },
        }
    }

    /// Checks if this symbol is one of the [reserved identifiers][spec] of the ECMAScript
    /// specification, excluding `await` and `yield`
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-ReservedWord
    #[inline]
    #[must_use]
    pub fn is_reserved_identifier(self) -> bool {
        (Self::BREAK..=Self::WITH).contains(&self)
    }

    /// Checks if this symbol is one of the [strict reserved identifiers][spec] of the ECMAScript
    /// specification.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-ReservedWord
    #[inline]
    #[must_use]
    pub fn is_strict_reserved_identifier(self) -> bool {
        (Self::IMPLEMENTS..=Self::YIELD).contains(&self)
    }

    /// Returns the internal value of the [`Sym`]
    #[inline]
    #[must_use]
    pub const fn get(self) -> usize {
        self.value.get()
    }
}

macro_rules! static_syms {
    ($(($ident:ident, $literal:literal)),* $(,)?) => {
        static_syms!(@consts 1usize; $(($ident, $literal))*);

        type Set<T> = indexmap::IndexSet<T, core::hash::BuildHasherDefault<rustc_hash::FxHasher>>;

        /// Ordered set of commonly used static `UTF-8` strings, in the order of the [`Sym`] constants.
        pub(super) static COMMON_STRINGS_UTF8: once_cell::sync::Lazy<Set<&'static str>> =
            once_cell::sync::Lazy::new(|| [$($literal),*].into_iter().collect());

        /// Ordered set of commonly used static `UTF-16` strings, in the order of the [`Sym`] constants.
        pub(super) static COMMON_STRINGS_UTF16: once_cell::sync::Lazy<Set<&'static [u16]>> =
            once_cell::sync::Lazy::new(|| [$(boa_macros::utf16!($literal)),*].into_iter().collect());
    };
    (@consts $index:expr; ($ident:ident, $literal:literal) $($rest:tt)*) => {
        impl Sym {
            #[doc = concat!("Symbol for the `", $literal, "` string.")]
            // SAFETY: indices start at 1 and only grow, so the value is never zero.
            pub const $ident: Self = unsafe { Self::new_unchecked($index) };
        }
        static_syms!(@consts $index + 1; $($rest)*);
    };
    (@consts $index:expr;) => {};
}

static_syms! {
    // Reserved identifiers
    // See: <https://tc39.es/ecma262/#prod-ReservedWord>
    // Note, they must all be together.
    (BREAK, "break"),
    (CASE, "case"),
    (CATCH, "catch"),
    (CLASS, "class"),
    (CONST, "const"),
    (CONTINUE, "continue"),
    (DEBUGGER, "debugger"),
    (DEFAULT, "default"),
    (DELETE, "delete"),
    (DO, "do"),
    (ELSE, "else"),
    (ENUM, "enum"),
    (EXPORT, "export"),
    (EXTENDS, "extends"),
    (FALSE, "false"),
    (FINALLY, "finally"),
    (FOR, "for"),
    (FUNCTION, "function"),
    (IF, "if"),
    (IMPORT, "import"),
    (IN, "in"),
    (INSTANCEOF, "instanceof"),
    (NEW, "new"),
    (NULL, "null"),
    (RETURN, "return"),
    (SUPER, "super"),
    (SWITCH, "switch"),
    (THIS, "this"),
    (THROW, "throw"),
    (TRUE, "true"),
    (TRY, "try"),
    (TYPEOF, "typeof"),
    (VAR, "var"),
    (VOID, "void"),
    (WHILE, "while"),
    (WITH, "with"),
    // End reserved identifier

    // strict reserved identifiers.
    // See: <https://tc39.es/ecma262/#prod-Identifier>
    // Note, they must all be together.
    (IMPLEMENTS, "implements"),
    (INTERFACE, "interface"),
    (LET, "let"),
    (PACKAGE, "package"),
    (PRIVATE, "private"),
    (PROTECTED, "protected"),
    (PUBLIC, "public"),
    (STATIC, "static"),
    (YIELD, "yield"),
    // End strict reserved identifiers

    (EMPTY_STRING, ""),
    (PROTOTYPE, "prototype"),
    (CONSTRUCTOR, "constructor"),
    (ARGUMENTS, "arguments"),
    (EVAL, "eval"),
    (REGEXP, "RegExp"),
    (GET, "get"),
    (SET, "set"),
    (MAIN, "<main>"),
    (RAW, "raw"),
    (ANONYMOUS, "anonymous"),
    (ASYNC, "async"),
    (OF, "of"),
    (TARGET, "target"),
    (AS, "as"),
    (FROM, "from"),
    (__PROTO__, "__proto__"),
    (NAME, "name"),
    (AWAIT, "await"),
    (DEFAULT_EXPORT, "*default*"),
    (META, "meta"),
    (DEFER, "defer"),
    (SOURCE, "source"),
    (USING, "using"),
    (DISPOSE, "dispose"),
    (ASYNCDISPOSE, "asyncDispose"),
}
