//! Boa's implementation of ECMAScript's global `RegExp` object.
//!
//! The `RegExp` object is used for matching text with a pattern.
//!
//! More information:
//!  - [ECMAScript reference][spec]
//!  - [MDN documentation][mdn]
//!
//! [spec]: https://tc39.es/ecma262/#sec-regexp-constructor
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp

use crate::{
    Context, JsArgs, JsData, JsExpect, JsResult, JsString,
    builtins::{BuiltInObject, array::Array, string},
    context::intrinsics::{Intrinsics, StandardConstructor, StandardConstructors},
    error::JsNativeError,
    js_string,
    object::{
        CONSTRUCTOR, IndexedProperties, JsObject, internal_methods::get_prototype_from_constructor,
    },
    property::Attribute,
    realm::Realm,
    string::{CodePoint, CommonJsStringBuilder, JsStrVariant, StaticJsStrings},
    symbol::JsSymbol,
    value::JsValue,
};
use boa_gc::{Finalize, Trace};
use boa_macros::{js_str, utf16};
use boa_parser::lexer::regex::RegExpFlags;
use regress::{Flags, Range, Regex};
use std::rc::Rc;
use std::str::FromStr;
use thin_vec::ThinVec;

use super::{BuiltInBuilder, BuiltInConstructor, IntrinsicObject};

mod regexp_string_iterator;
pub(crate) use regexp_string_iterator::RegExpStringIterator;
#[cfg(test)]
mod tests;

/// The internal representation of a `RegExp` object.
#[derive(Debug, Clone, Trace, Finalize, JsData)]
// Safety: `RegExp` does not contain any objects which needs to be traced, so this is safe.
#[boa_gc(unsafe_empty_trace)]
pub struct RegExp {
    /// Regex matcher, shared by the copies `RegExpBuiltinExec` takes of this record.
    matcher: Rc<Regex>,
    flags: RegExpFlags,
    original_source: JsString,
    original_flags: JsString,
    /// The id of `[[Realm]]`, the realm the object was allocated in.
    realm_id: u64,
    /// `[[LegacyFeaturesEnabled]]`: false for objects of `RegExp` subclasses.
    legacy_features: bool,
}

/// The matchers compiled for recent pattern and flag pairs, so a regular expression literal
/// evaluated in a loop, or the copy `split` and `replaceAll` construct, reuses its matcher.
#[derive(Debug, Default)]
pub(crate) struct MatcherCache {
    matchers: std::collections::HashMap<(JsString, u8), Rc<Regex>>,
}

impl MatcherCache {
    const CAPACITY: usize = 512;

    fn get(&self, pattern: &JsString, flags: RegExpFlags) -> Option<Rc<Regex>> {
        self.matchers.get(&(pattern.clone(), flags.bits())).cloned()
    }

    fn insert(&mut self, pattern: JsString, flags: RegExpFlags, matcher: Rc<Regex>) {
        if self.matchers.len() >= Self::CAPACITY {
            self.matchers.clear();
        }
        self.matchers.insert((pattern, flags.bits()), matcher);
    }
}

/// The values of the legacy static properties of a realm's `%RegExp%` (`[[RegExpInput]]`,
/// `[[RegExpLastMatch]]`, `[[RegExpLastParen]]`, `[[RegExpLeftContext]]`,
/// `[[RegExpRightContext]]` and `[[RegExpParen1]]` to `[[RegExpParen9]]`).
///
/// More information:
///  - [Proposal reference][spec]
///
/// [spec]: https://github.com/tc39/proposal-regexp-legacy-features
#[cfg(feature = "annex-b")]
#[derive(Debug, Clone, Trace, Finalize)]
pub(crate) struct LegacyRegExpStatics {
    input: JsString,
    /// The string the last match ran on; the other values are slices of it, made when read.
    matched: JsString,
    #[unsafe_ignore_trace]
    start: usize,
    #[unsafe_ignore_trace]
    end: usize,
    #[unsafe_ignore_trace]
    captures: Vec<Option<Range>>,
}

#[cfg(feature = "annex-b")]
impl Default for LegacyRegExpStatics {
    fn default() -> Self {
        Self {
            input: js_string!(),
            matched: js_string!(),
            start: 0,
            end: 0,
            captures: Vec::new(),
        }
    }
}

#[cfg(feature = "annex-b")]
impl LegacyRegExpStatics {
    fn capture(&self, capture: Option<&Option<Range>>) -> JsString {
        capture
            .cloned()
            .flatten()
            .map_or_else(|| js_string!(), |range| self.matched.get_expect(range))
    }

    fn value(&self, slot: LegacySlot) -> JsString {
        match slot {
            LegacySlot::Input => self.input.clone(),
            LegacySlot::LastMatch => self.matched.get_expect(self.start..self.end),
            LegacySlot::LastParen => self.capture(self.captures.last()),
            LegacySlot::LeftContext => self.matched.get_expect(..self.start),
            LegacySlot::RightContext => self.matched.get_expect(self.end..),
            LegacySlot::Paren(index) => self.capture(self.captures.get(index)),
        }
    }
}

/// One of the internal slots of [`LegacyRegExpStatics`].
#[cfg(feature = "annex-b")]
#[derive(Debug, Clone, Copy)]
enum LegacySlot {
    Input,
    LastMatch,
    LastParen,
    LeftContext,
    RightContext,
    Paren(usize),
}

#[cfg(feature = "annex-b")]
macro_rules! legacy_getters {
    ($($name:ident => $slot:expr),* $(,)?) => {
        impl RegExp {
            $(
                fn $name(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                    Self::get_legacy_static(this, $slot, context)
                }
            )*
        }
    };
}

#[cfg(feature = "annex-b")]
legacy_getters! {
    get_legacy_input => LegacySlot::Input,
    get_legacy_last_match => LegacySlot::LastMatch,
    get_legacy_last_paren => LegacySlot::LastParen,
    get_legacy_left_context => LegacySlot::LeftContext,
    get_legacy_right_context => LegacySlot::RightContext,
    get_legacy_paren1 => LegacySlot::Paren(0),
    get_legacy_paren2 => LegacySlot::Paren(1),
    get_legacy_paren3 => LegacySlot::Paren(2),
    get_legacy_paren4 => LegacySlot::Paren(3),
    get_legacy_paren5 => LegacySlot::Paren(4),
    get_legacy_paren6 => LegacySlot::Paren(5),
    get_legacy_paren7 => LegacySlot::Paren(6),
    get_legacy_paren8 => LegacySlot::Paren(7),
    get_legacy_paren9 => LegacySlot::Paren(8),
}

impl RegExp {
    /// Returns the original source string of the regex (e.g. `"regex-test"`).
    pub(crate) fn original_source(&self) -> &JsString {
        &self.original_source
    }

    /// Returns the original flags string of the regex (e.g. `"gi"`).
    pub(crate) fn original_flags(&self) -> &JsString {
        &self.original_flags
    }
}

impl IntrinsicObject for RegExp {
    fn init(realm: &Realm) {
        let get_species = BuiltInBuilder::callable(realm, Self::get_species)
            .name(js_string!("get [Symbol.species]"))
            .build();

        let flag_attributes = Attribute::CONFIGURABLE | Attribute::NON_ENUMERABLE;

        let get_has_indices = BuiltInBuilder::callable(realm, Self::get_has_indices)
            .name(js_string!("get hasIndices"))
            .build();
        let get_global = BuiltInBuilder::callable(realm, Self::get_global)
            .name(js_string!("get global"))
            .build();
        let get_ignore_case = BuiltInBuilder::callable(realm, Self::get_ignore_case)
            .name(js_string!("get ignoreCase"))
            .build();
        let get_multiline = BuiltInBuilder::callable(realm, Self::get_multiline)
            .name(js_string!("get multiline"))
            .build();
        let get_dot_all = BuiltInBuilder::callable(realm, Self::get_dot_all)
            .name(js_string!("get dotAll"))
            .build();
        let get_unicode = BuiltInBuilder::callable(realm, Self::get_unicode)
            .name(js_string!("get unicode"))
            .build();
        let get_unicode_sets = BuiltInBuilder::callable(realm, Self::get_unicode_sets)
            .name(js_string!("get unicodeSets"))
            .build();
        let get_sticky = BuiltInBuilder::callable(realm, Self::get_sticky)
            .name(js_string!("get sticky"))
            .build();
        let get_flags = BuiltInBuilder::callable(realm, Self::get_flags)
            .name(js_string!("get flags"))
            .build();
        let get_source = BuiltInBuilder::callable(realm, Self::get_source)
            .name(js_string!("get source"))
            .build();
        let regexp = BuiltInBuilder::from_standard_constructor::<Self>(realm)
            .static_method(Self::escape, js_string!("escape"), 1)
            .static_accessor(
                JsSymbol::species(),
                Some(get_species),
                None,
                Attribute::CONFIGURABLE,
            )
            .method(Self::test, js_string!("test"), 1)
            .method(Self::exec, js_string!("exec"), 1)
            .method(Self::to_string, js_string!("toString"), 0)
            .method(Self::r#match, JsSymbol::r#match(), 1)
            .method(Self::match_all, JsSymbol::match_all(), 1)
            .method(Self::replace, JsSymbol::replace(), 2)
            .method(Self::search, JsSymbol::search(), 1)
            .method(Self::split, JsSymbol::split(), 2)
            .accessor(
                js_string!("hasIndices"),
                Some(get_has_indices),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("global"),
                Some(get_global),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("ignoreCase"),
                Some(get_ignore_case),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("multiline"),
                Some(get_multiline),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("dotAll"),
                Some(get_dot_all),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("unicode"),
                Some(get_unicode),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("unicodeSets"),
                Some(get_unicode_sets),
                None,
                flag_attributes,
            )
            .accessor(
                js_string!("sticky"),
                Some(get_sticky),
                None,
                flag_attributes,
            )
            .accessor(js_string!("flags"), Some(get_flags), None, flag_attributes)
            .accessor(
                js_string!("source"),
                Some(get_source),
                None,
                flag_attributes,
            );

        #[cfg(feature = "annex-b")]
        let regexp = {
            let mut regexp = regexp.method(Self::compile, js_string!("compile"), 2);
            let set_input = BuiltInBuilder::callable(realm, Self::set_legacy_input)
                .name(js_string!("set input"))
                .length(1)
                .build();
            let getters: [(&str, &str, crate::native_function::NativeFunctionPointer); 14] = [
                ("input", "$_", Self::get_legacy_input),
                ("lastMatch", "$&", Self::get_legacy_last_match),
                ("lastParen", "$+", Self::get_legacy_last_paren),
                ("leftContext", "$`", Self::get_legacy_left_context),
                ("rightContext", "$'", Self::get_legacy_right_context),
                ("$1", "", Self::get_legacy_paren1),
                ("$2", "", Self::get_legacy_paren2),
                ("$3", "", Self::get_legacy_paren3),
                ("$4", "", Self::get_legacy_paren4),
                ("$5", "", Self::get_legacy_paren5),
                ("$6", "", Self::get_legacy_paren6),
                ("$7", "", Self::get_legacy_paren7),
                ("$8", "", Self::get_legacy_paren8),
                ("$9", "", Self::get_legacy_paren9),
            ];
            for (name, alias, getter) in getters {
                let setter = (name == "input").then(|| set_input.clone());
                for key in [name, alias] {
                    if key.is_empty() {
                        continue;
                    }
                    let get = BuiltInBuilder::callable(realm, getter)
                        .name(js_string!(format!("get {key}").as_str()))
                        .build();
                    regexp = regexp.static_accessor(
                        js_string!(key),
                        Some(get),
                        setter.clone(),
                        Attribute::CONFIGURABLE,
                    );
                }
            }
            regexp
        };

        regexp.build();
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        Self::STANDARD_CONSTRUCTOR(intrinsics.constructors()).constructor()
    }
}

impl BuiltInObject for RegExp {
    const NAME: JsString = StaticJsStrings::REG_EXP;
}

impl BuiltInConstructor for RegExp {
    const CONSTRUCTOR_ARGUMENTS: usize = 2;
    const PROTOTYPE_STORAGE_SLOTS: usize = 29;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = if cfg!(feature = "annex-b") {
        3 + 2 * 19
    } else {
        3
    };

    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::regexp;

    /// `22.2.3.1 RegExp ( pattern, flags )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp-pattern-flags
    fn constructor(
        new_target: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        let pattern = args.get_or_undefined(0);
        let flags = args.get_or_undefined(1);

        // 1. Let patternIsRegExp be ? IsRegExp(pattern).
        let pattern_is_regexp = Self::is_reg_exp(pattern, context)?;

        // 2. If NewTarget is undefined, then
        // 3. Else, let newTarget be NewTarget.
        if new_target.is_undefined() {
            // a. Let newTarget be the active function object.
            let new_target = context
                .active_function_object()
                .map_or(JsValue::undefined(), JsValue::new);

            // b. If patternIsRegExp is true and flags is undefined, then
            if let Some(pattern) = &pattern_is_regexp
                && flags.is_undefined()
            {
                // i. Let patternConstructor be ? Get(pattern, "constructor").
                let pattern_constructor = pattern.get(CONSTRUCTOR, context)?;

                // ii. If SameValue(newTarget, patternConstructor) is true, return pattern.
                if JsValue::same_value(&new_target, &pattern_constructor) {
                    return Ok(pattern.clone().into());
                }
            }
        }

        // 4. If pattern is an Object and pattern has a [[RegExpMatcher]] internal slot, then
        let object = pattern.clone().as_object();
        let (p, f) =
            if let Some(pattern) = object.as_ref().and_then(JsObject::downcast_ref::<RegExp>) {
                // a. Let P be pattern.[[OriginalSource]].
                let p = pattern.original_source.clone().into();

                // b. If flags is undefined, let F be pattern.[[OriginalFlags]].
                let f = if flags.is_undefined() {
                    pattern.original_flags.clone().into()
                // c. Else, let F be flags.
                } else {
                    flags.clone()
                };

                (p, f)
            } else if let Some(pattern) = &pattern_is_regexp {
                // a. Let P be ? Get(pattern, "source").
                let p = pattern.get(js_string!("source"), context)?;

                // b. If flags is undefined, then
                let f = if flags.is_undefined() {
                    // i. Let F be ? Get(pattern, "flags").
                    pattern.get(js_string!("flags"), context)?
                // c. Else,
                } else {
                    // i. Let F be flags.
                    flags.clone()
                };

                (p, f)
            // 6. Else,
            } else {
                // a. Let P be pattern.
                // b. Let F be flags.
                (pattern.clone(), flags.clone())
            };

        // 7. Let O be ? RegExpAlloc(newTarget).
        let proto =
            get_prototype_from_constructor(new_target, StandardConstructors::regexp, context)?;

        // 8.Return ? RegExpInitialize(O, P, F).
        let regexp = Self::initialize(Some(proto), &p, &f, context)?;

        // RegExpAlloc: if SameValue(newTarget, thisRealm.[[Intrinsics]].[[%RegExp%]]) is false,
        // set obj.[[LegacyFeaturesEnabled]] to false.
        let intrinsic = context.intrinsics().constructors().regexp().constructor();
        let legacy = new_target.is_undefined()
            || new_target
                .as_object()
                .is_some_and(|target| JsObject::equals(&target, &intrinsic));
        if !legacy
            && let Some(mut data) = regexp
                .as_object()
                .as_ref()
                .and_then(JsObject::downcast_mut::<RegExp>)
        {
            data.legacy_features = false;
        }
        Ok(regexp)
    }
}

impl RegExp {
    /// `7.2.8 IsRegExp ( argument )`
    ///
    /// This modified to return the object if it's `true`, [`None`] otherwise.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-isregexp
    pub(crate) fn is_reg_exp(
        argument: &JsValue,
        context: &mut Context,
    ) -> JsResult<Option<JsObject>> {
        // 1. If argument is not an Object, return false.
        let Some(argument) = argument.as_object() else {
            return Ok(None);
        };

        // 2. Let matcher be ? Get(argument, @@match).
        let matcher = argument.get(JsSymbol::r#match(), context)?;

        // 3. If matcher is not undefined, return ToBoolean(matcher).
        if !matcher.is_undefined() {
            return Ok(matcher.to_boolean().then_some(argument));
        }

        // 4. If argument has a [[RegExpMatcher]] internal slot, return true.
        if argument.is::<RegExp>() {
            return Ok(Some(argument));
        }

        // 5. Return false.
        Ok(None)
    }

    /// Compiles a `RegExp` from the provided pattern and flags.
    ///
    /// Equivalent to the beginning of [`RegExpInitialize ( obj, pattern, flags )`][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexpinitialize
    fn compile_native_regexp(
        pattern: &JsValue,
        flags: &JsValue,
        context: &mut Context,
    ) -> JsResult<RegExp> {
        // 1. If pattern is undefined, let P be the empty String.
        // 2. Else, let P be ? ToString(pattern).
        let p = if pattern.is_undefined() {
            js_string!()
        } else {
            pattern.to_string(context)?
        };

        // 3. If flags is undefined, let F be the empty String.
        // 4. Else, let F be ? ToString(flags).
        let f = if flags.is_undefined() {
            js_string!()
        } else {
            flags.to_string(context)?
        };

        // 5. If F contains any code unit other than "g", "i", "m", "s", "u", "v", or "y"
        //    or if it contains the same code unit more than once, throw a SyntaxError exception.
        // TODO: Should directly parse the JsString instead of converting to String
        let flags = match RegExpFlags::from_str(&f.to_std_string_escaped()) {
            Err(msg) => return Err(JsNativeError::syntax().with_message(msg).into()),
            Ok(result) => result,
        };

        // 13. Let parseResult be ParsePattern(patternText, u, v).
        // 14. If parseResult is a non-empty List of SyntaxError objects, throw a SyntaxError exception.
        let matcher = match context.regexp_cache.get(&p, flags) {
            Some(matcher) => matcher,
            None => {
                let matcher = Rc::new(Self::compile_matcher(&p, flags)?);
                context
                    .regexp_cache
                    .insert(p.clone(), flags, matcher.clone());
                matcher
            }
        };

        // 15. Assert: parseResult is a Pattern Parse Node.
        // 16. Set obj.[[OriginalSource]] to P.
        // 17. Set obj.[[OriginalFlags]] to F.
        // 18. Let capturingGroupsCount be CountLeftCapturingParensWithin(parseResult).
        // 19. Let rer be the RegExp Record { [[IgnoreCase]]: i, [[Multiline]]: m,
        //     [[DotAll]]: s, [[Unicode]]: u, [[UnicodeSets]]: v,
        //     [[CapturingGroupsCount]]: capturingGroupsCount }.
        // 20. Set obj.[[RegExpRecord]] to rer.
        // 21. Set obj.[[RegExpMatcher]] to CompilePattern of parseResult with argument rer.
        Ok(RegExp {
            matcher,
            flags,
            original_source: p,
            original_flags: f,
            realm_id: context.realm().id(),
            legacy_features: true,
        })
    }

    /// Compiles the matcher for a pattern and its flags.
    fn compile_matcher(p: &JsString, flags: RegExpFlags) -> JsResult<Regex> {
        // If u or v flag is set, fullUnicode is true — compile as full codepoints.
        let full_unicode =
            flags.contains(RegExpFlags::UNICODE) || flags.contains(RegExpFlags::UNICODE_SETS);

        let code_points: Vec<u32> = p.code_points().map(CodePoint::as_u32).collect();
        let code_points = prepare_pattern(
            code_points,
            full_unicode,
            flags.contains(RegExpFlags::UNICODE_SETS),
        );

        let matcher = if full_unicode {
            // Unicode mode (u/v flag) OR pattern has named groups:
            // compile as full Unicode codepoints.
            Regex::from_unicode(code_points.into_iter(), Flags::from(flags)).map_err(|error| {
                JsNativeError::syntax()
                    .with_message(format!("failed to create matcher: {}", error.text))
            })?
        } else {
            // Non-Unicode mode with no named groups:
            // compile as raw UTF-16 code units so that surrogate pairs
            // (e.g. 𠮷 = [0xD842, 0xDFB7]) are matched correctly by find_from_ucs2.
            let utf16_units = code_points.into_iter().flat_map(|cp| {
                let mut buf = [0u16; 2];
                match char::from_u32(cp) {
                    Some(c) => c
                        .encode_utf16(&mut buf)
                        .iter()
                        .map(|&u| u32::from(u))
                        .collect::<Vec<_>>(),
                    None => vec![cp],
                }
            });
            Regex::from_unicode(utf16_units, Flags::from(flags)).map_err(|error| {
                JsNativeError::syntax()
                    .with_message(format!("failed to create matcher: {}", error.text))
            })?
        };
        Ok(matcher)
    }

    /// `RegExpInitialize ( obj, pattern, flags )`
    ///
    /// If prototype is `None`, initializes the prototype to `%RegExp%.prototype`.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexpinitialize
    pub(crate) fn initialize(
        prototype: Option<JsObject>,
        pattern: &JsValue,
        flags: &JsValue,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // Has the steps  of `RegExpInitialize`.
        let regexp = Self::compile_native_regexp(pattern, flags, context)?;

        // 22. Perform ? Set(obj, "lastIndex", +0𝔽, true).
        let obj = if let Some(prototype) = prototype {
            let mut template = context
                .intrinsics()
                .templates()
                .regexp_without_proto()
                .clone();
            template.set_prototype(prototype);
            template.create(regexp, vec![0.into()])
        } else {
            context
                .intrinsics()
                .templates()
                .regexp()
                .create(regexp, vec![0.into()])
        };

        // 23. Return obj.
        Ok(obj.into())
    }

    /// `22.2.3.2.4 RegExpCreate ( P, F )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexpcreate
    pub(crate) fn create(p: &JsValue, f: &JsValue, context: &mut Context) -> JsResult<JsValue> {
        // 1. Let obj be ? RegExpAlloc(%RegExp%).
        // 2. Return ? RegExpInitialize(obj, P, F).
        Self::initialize(None, p, f, context)
    }

    /// `get RegExp [ @@species ]`
    ///
    /// The `RegExp [ @@species ]` accessor property returns the `RegExp` constructor.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp-@@species
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/@@species
    #[allow(clippy::unnecessary_wraps)]
    fn get_species(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        // 1. Return the this value.
        Ok(this.clone())
    }

    fn regexp_has_flag(this: &JsValue, flag: u8, context: &mut Context) -> JsResult<JsValue> {
        if let Some(object) = this.as_object() {
            if let Some(regexp) = object.downcast_ref::<RegExp>() {
                return Ok(JsValue::new(match flag {
                    b'd' => regexp.flags.contains(RegExpFlags::HAS_INDICES),
                    b'g' => regexp.flags.contains(RegExpFlags::GLOBAL),
                    b'm' => regexp.flags.contains(RegExpFlags::MULTILINE),
                    b's' => regexp.flags.contains(RegExpFlags::DOT_ALL),
                    b'i' => regexp.flags.contains(RegExpFlags::IGNORE_CASE),
                    b'u' => regexp.flags.contains(RegExpFlags::UNICODE),
                    b'v' => regexp.flags.contains(RegExpFlags::UNICODE_SETS),
                    b'y' => regexp.flags.contains(RegExpFlags::STICKY),
                    _ => unreachable!(),
                }));
            }

            if JsObject::equals(
                &object,
                &context.intrinsics().constructors().regexp().prototype(),
            ) {
                return Ok(JsValue::undefined());
            }
        }

        let name = match flag {
            b'd' => "hasIndices",
            b'g' => "global",
            b'm' => "multiline",
            b's' => "dotAll",
            b'i' => "ignoreCase",
            b'u' => "unicode",
            b'v' => "unicodeSets",
            b'y' => "sticky",
            _ => unreachable!(),
        };

        Err(JsNativeError::typ()
            .with_message(format!(
                "RegExp.prototype.{name} getter called on non-RegExp object",
            ))
            .into())
    }

    /// `get RegExp.prototype.hasIndices`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.hasindices
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/global
    pub(crate) fn get_has_indices(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'd', context)
    }

    /// `get RegExp.prototype.global`
    ///
    /// The `global` property indicates whether or not the "`g`" flag is used with the regular expression.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.global
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/global
    pub(crate) fn get_global(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'g', context)
    }

    /// `get RegExp.prototype.ignoreCase`
    ///
    /// The `ignoreCase` property indicates whether or not the "`i`" flag is used with the regular expression.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.ignorecase
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/ignoreCase
    pub(crate) fn get_ignore_case(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'i', context)
    }

    /// `get RegExp.prototype.multiline`
    ///
    /// The multiline property indicates whether or not the "m" flag is used with the regular expression.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.multiline
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/multiline
    pub(crate) fn get_multiline(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'm', context)
    }

    /// `get RegExp.prototype.dotAll`
    ///
    /// The `dotAll` property indicates whether or not the "`s`" flag is used with the regular expression.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.dotAll
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/dotAll
    pub(crate) fn get_dot_all(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b's', context)
    }

    /// `get RegExp.prototype.unicode`
    ///
    /// The unicode property indicates whether or not the "`u`" flag is used with a regular expression.
    /// unicode is a read-only property of an individual regular expression instance.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.unicode
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/unicode
    pub(crate) fn get_unicode(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'u', context)
    }

    /// `get RegExp.prototype.unicodeSets`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.unicodesets
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/unicodeSets
    pub(crate) fn get_unicode_sets(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'v', context)
    }

    /// `get RegExp.prototype.sticky`
    ///
    /// This flag indicates that it matches only from the index indicated by the `lastIndex` property
    /// of this regular expression in the target string (and does not attempt to match from any later indexes).
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.sticky
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/sticky
    pub(crate) fn get_sticky(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::regexp_has_flag(this, b'y', context)
    }

    /// `get RegExp.prototype.flags`
    ///
    /// The `flags` property returns a string consisting of the [`flags`][flags] of the current regular expression object.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.flags
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/flags
    /// [flags]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Regular_Expressions#Advanced_searching_with_flags_2
    pub(crate) fn get_flags(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let R be the this value.
        // 2. If R is not an Object, throw a TypeError exception.
        let Some(object) = this.as_object() else {
            return Err(JsNativeError::typ()
                .with_message("RegExp.prototype.flags getter called on non-object")
                .into());
        };

        // 3. Let codeUnits be a new empty List.
        let mut code_units = String::new();

        // 4. Let hasIndices be ToBoolean(? Get(R, "hasIndices")).
        // 5. If hasIndices is true, append the code unit 0x0064 (LATIN SMALL LETTER D) to codeUnits.
        if object.get(js_string!("hasIndices"), context)?.to_boolean() {
            code_units.push('d');
        }

        // 6. Let global be ToBoolean(? Get(R, "global")).
        // 7. If global is true, append the code unit 0x0067 (LATIN SMALL LETTER G) to codeUnits.
        if object.get(js_string!("global"), context)?.to_boolean() {
            code_units.push('g');
        }

        // 8. Let ignoreCase be ToBoolean(? Get(R, "ignoreCase")).
        // 9. If ignoreCase is true, append the code unit 0x0069 (LATIN SMALL LETTER I) to codeUnits.
        if object.get(js_string!("ignoreCase"), context)?.to_boolean() {
            code_units.push('i');
        }

        // 10. Let multiline be ToBoolean(? Get(R, "multiline")).
        // 11. If multiline is true, append the code unit 0x006D (LATIN SMALL LETTER M) to codeUnits.
        if object.get(js_string!("multiline"), context)?.to_boolean() {
            code_units.push('m');
        }

        // 12. Let dotAll be ToBoolean(? Get(R, "dotAll")).
        // 13. If dotAll is true, append the code unit 0x0073 (LATIN SMALL LETTER S) to codeUnits.
        if object.get(js_string!("dotAll"), context)?.to_boolean() {
            code_units.push('s');
        }

        // 14. Let unicode be ToBoolean(? Get(R, "unicode")).
        // 15. If unicode is true, append the code unit 0x0075 (LATIN SMALL LETTER U) to codeUnits.
        if object.get(js_string!("unicode"), context)?.to_boolean() {
            code_units.push('u');
        }

        // 16. Let unicodeSets be ToBoolean(? Get(R, "unicodeSets")).
        // 17. If unicodeSets is true, append the code unit 0x0076 (LATIN SMALL LETTER V) to codeUnits.
        if object.get(js_string!("unicodeSets"), context)?.to_boolean() {
            code_units.push('v');
        }

        // 18. Let sticky be ToBoolean(? Get(R, "sticky")).
        // 19. If sticky is true, append the code unit 0x0079 (LATIN SMALL LETTER Y) to codeUnits.
        if object.get(js_string!("sticky"), context)?.to_boolean() {
            code_units.push('y');
        }

        // 20. Return the String value whose code units are the elements of the List codeUnits.
        //     If codeUnits has no elements, the empty String is returned.
        Ok(JsString::from(code_units).into())
    }

    /// `get RegExp.prototype.source`
    ///
    /// The `source` property returns a `String` containing the source text of the regexp object,
    /// and it doesn't contain the two forward slashes on both sides and any flags.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-get-regexp.prototype.source
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/source
    pub(crate) fn get_source(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let R be the this value.
        // 2. If Type(R) is not Object, throw a TypeError exception.
        let Some(object) = this.as_object() else {
            return Err(JsNativeError::typ()
                .with_message("RegExp.prototype.source method called on incompatible value")
                .into());
        };

        let casted = object.downcast_ref::<RegExp>();
        match casted {
            // 3. If R does not have an [[OriginalSource]] internal slot, then
            None => {
                // a. If SameValue(R, %RegExp.prototype%) is true, return "(?:)".
                // b. Otherwise, throw a TypeError exception.
                if JsValue::same_value(
                    this,
                    &JsValue::new(context.intrinsics().constructors().regexp().prototype()),
                ) {
                    Ok(JsValue::new(js_string!("(?:)")))
                } else {
                    Err(JsNativeError::typ()
                        .with_message("RegExp.prototype.source method called on incompatible value")
                        .into())
                }
            }
            // 4. Assert: R has an [[OriginalFlags]] internal slot.
            Some(re) => {
                // 5. Let src be R.[[OriginalSource]].
                // 6. Let flags be R.[[OriginalFlags]].
                // 7. Return EscapeRegExpPattern(src, flags).
                Ok(Self::escape_pattern(
                    &re.original_source,
                    &re.original_flags,
                ))
            }
        }
    }

    /// `22.2.3.2.5 EscapeRegExpPattern ( P, F )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-escaperegexppattern
    fn escape_pattern(src: &JsString, _flags: &JsString) -> JsValue {
        if src.is_empty() {
            js_string!("(?:)").into()
        } else {
            // A `/` is escaped only outside a character class and when no backslash escapes it
            // already, and a line terminator after a backslash only needs its letter.
            let mut s = Vec::with_capacity(src.len());
            let mut escaped = false;
            let mut in_class = false;
            for unit in src.iter() {
                let terminator: Option<&[u16]> = match unit {
                    0x0A => Some(utf16!("n")),
                    0x0D => Some(utf16!("r")),
                    0x2028 => Some(utf16!("u2028")),
                    0x2029 => Some(utf16!("u2029")),
                    _ => None,
                };
                if let Some(letters) = terminator {
                    if !escaped {
                        s.push(u16::from(b'\\'));
                    }
                    s.extend_from_slice(letters);
                    escaped = false;
                    continue;
                }
                if escaped {
                    escaped = false;
                } else if unit == u16::from(b'\\') {
                    escaped = true;
                } else if unit == u16::from(b'[') {
                    in_class = true;
                } else if unit == u16::from(b']') {
                    in_class = false;
                } else if unit == u16::from(b'/') && !in_class {
                    s.push(u16::from(b'\\'));
                }
                s.push(unit);
            }

            JsValue::new(js_string!(&s[..]))
        }
    }

    /// `RegExp.escape( string )`
    ///
    /// The `RegExp.escape()` static method escapes any potential regex syntax characters in a string,
    /// and returns a new string that can be safely used as a literal pattern for the `RegExp()` constructor.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/proposal-regex-escaping/#sec-regexp.escape
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/escape
    ///
    /// Helper function to check if a character is a `WhiteSpace` character
    fn is_whitespace(ch: char) -> bool {
        matches!(
            ch,
            '\u{0009}' | // <TAB>
            '\u{000B}' | // <VT>
            '\u{000C}' | // <FF>
            '\u{0020}' | // <SP>
            '\u{00A0}' | // <NBSP>
            '\u{FEFF}' | // <ZWNBSP>
            '\u{1680}' | // Ogham Space Mark
            '\u{2000}' | '\u{2001}' | '\u{2002}' | '\u{2003}' | '\u{2004}' |
            '\u{2005}' | '\u{2006}' | '\u{2007}' | '\u{2008}' | '\u{2009}' |
            '\u{200A}' | // Various space separators
            '\u{202F}' | // Narrow No-Break Space
            '\u{205F}' | // Medium Mathematical Space
            '\u{3000}' // Ideographic Space
        )
    }

    /// Helper function to check if a character is a `LineTerminator` character
    fn is_line_terminator(ch: char) -> bool {
        matches!(
            ch,
            '\u{000A}' | // <LF>
            '\u{000D}' | // <CR>
            '\u{2028}' | // <LS>
            '\u{2029}' // <PS>
        )
    }

    pub(crate) fn escape(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let arg = args.get_or_undefined(0);

        // 1. If S is not a String, throw a TypeError exception.
        let Some(string) = arg.as_string() else {
            return Err(JsNativeError::typ()
                .with_message("RegExp.escape requires a string argument")
                .into());
        };

        // 2. Let escaped be the empty String.
        let mut escaped = CommonJsStringBuilder::new();

        // 3. Let cpList be StringToCodePoints(S).
        // 4. For each code point c of cpList, do
        for (index, c) in string.code_points().enumerate() {
            let code = c.as_u32();

            // 4.a. If escaped is the empty String and c is matched by either DecimalDigit or AsciiLetter, then
            if index == 0
                && let CodePoint::Unicode(ch) = c
                && (ch.is_ascii_digit() || ch.is_ascii_alphabetic())
            {
                // 4.a.ii-v. Escape using \xXX format
                let escape_seq = format!("\\x{code:02x}");
                escaped.push(escape_seq.as_str());
                continue;
            }

            // 4.b. Else, set escaped to the string-concatenation of escaped and EncodeForRegExpEscape(c).
            match c {
                CodePoint::Unicode(ch) => {
                    // EncodeForRegExpEscape step 1: SyntaxCharacter or U+002F (SOLIDUS)
                    if matches!(
                        ch,
                        '^' | '$'
                            | '\\'
                            | '.'
                            | '*'
                            | '+'
                            | '?'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '|'
                            | '/'
                    ) {
                        escaped.push('\\');
                        escaped.push(ch);
                    }
                    // Step 2: ControlEscape characters (Table 64)
                    else if ch == '\x09' {
                        escaped.push("\\t");
                    } else if ch == '\x0A' {
                        escaped.push("\\n");
                    } else if ch == '\x0B' {
                        escaped.push("\\v");
                    } else if ch == '\x0C' {
                        escaped.push("\\f");
                    } else if ch == '\x0D' {
                        escaped.push("\\r");
                    }
                    // Step 3-5: otherPunctuators or WhiteSpace or LineTerminator
                    else if matches!(
                        ch,
                        ',' | '-'
                            | '='
                            | '<'
                            | '>'
                            | '#'
                            | '&'
                            | '!'
                            | '%'
                            | ':'
                            | ';'
                            | '@'
                            | '~'
                            | '\''
                            | '`'
                            | '"'
                    ) || Self::is_whitespace(ch)
                        || Self::is_line_terminator(ch)
                    {
                        let code = ch as u32;
                        if code <= 0xFF {
                            // Use \xXX format
                            let escape_seq = format!("\\x{code:02x}");
                            escaped.push(escape_seq.as_str());
                        } else {
                            // Use \uXXXX format
                            let escape_seq = format!("\\u{code:04x}");
                            escaped.push(escape_seq.as_str());
                        }
                    }
                    // Step 6: All other Unicode characters
                    else {
                        escaped.push(ch);
                    }
                }
                CodePoint::UnpairedSurrogate(surr) => {
                    // Escape unpaired surrogates using \uXXXX format
                    let escape_seq = format!("\\u{surr:04x}");
                    escaped.push(escape_seq.as_str());
                }
            }
        }

        // 5. Return escaped.
        Ok(JsValue::new(escaped.build()))
    }

    /// `RegExp.prototype.test( string )`
    ///
    /// The `test()` method executes a search for a match between a regular expression and a specified string.
    ///
    /// Returns `true` or `false`.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype.test
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/test
    pub(crate) fn test(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let R be the this value.
        // 2. If Type(R) is not Object, throw a TypeError exception.
        let this = this.as_object().ok_or_else(|| {
            JsNativeError::typ()
                .with_message("RegExp.prototype.test method called on incompatible value")
        })?;

        // 3. Let string be ? ToString(S).
        let arg_str = args
            .first()
            .cloned()
            .unwrap_or_default()
            .to_string(context)?;

        // 4. Let match be ? RegExpExec(R, string).
        let m = Self::abstract_exec(&this, arg_str, context)?;

        // 5. If match is not null, return true; else return false.
        if m.is_some() {
            Ok(JsValue::new(true))
        } else {
            Ok(JsValue::new(false))
        }
    }

    /// `RegExp.prototype.exec( string )`
    ///
    /// The `exec()` method executes a search for a match in a specified string.
    ///
    /// Returns a result array, or `null`.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype.exec
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/exec
    pub(crate) fn exec(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let R be the this value.
        // 2. Perform ? RequireInternalSlot(R, [[RegExpMatcher]]).
        let this = this.as_object();
        let obj = this
            .and_then(|o| o.downcast::<RegExp>().ok())
            .ok_or_else(|| {
                JsNativeError::typ().with_message("RegExp.prototype.exec called with invalid value")
            })?;

        // 3. Let S be ? ToString(string).
        let arg_str = args.get_or_undefined(0).to_string(context)?;

        // 4. Return ? RegExpBuiltinExec(R, S).
        (Self::abstract_builtin_exec(obj, &arg_str, context)?)
            .map_or_else(|| Ok(JsValue::null()), |v| Ok(v.into()))
    }

    /// `22.2.5.2.1 RegExpExec ( R, S )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexpexec
    pub(crate) fn abstract_exec(
        this: &JsObject,
        input: JsString,
        context: &mut Context,
    ) -> JsResult<Option<JsObject>> {
        // 1. Assert: Type(R) is Object.
        // 2. Assert: Type(S) is String.

        // 3. Let exec be ? Get(R, "exec").
        let exec = this.get(js_string!("exec"), context)?;

        // 4. If IsCallable(exec) is true, then
        if let Some(exec) = exec.as_callable() {
            // a. Let result be ? Call(exec, R, « S »).
            let result = exec.call(&this.clone().into(), &[input.into()], context)?;

            // b. If Type(result) is neither Object nor Null, throw a TypeError exception.
            if !result.is_object() && !result.is_null() {
                return Err(JsNativeError::typ()
                    .with_message("regexp exec returned neither object nor null")
                    .into());
            }

            // c. Return result.
            return Ok(result.as_object());
        }

        // 5. Perform ? RequireInternalSlot(R, [[RegExpMatcher]]).
        let Ok(this) = this.clone().downcast::<RegExp>() else {
            return Err(JsNativeError::typ()
                .with_message("RegExpExec called with invalid value")
                .into());
        };

        // 6. Return ? RegExpBuiltinExec(R, S).
        Self::abstract_builtin_exec(this, &input, context)
    }

    /// `22.2.7.2 RegExpBuiltinExec ( R, S )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexpbuiltinexec
    pub(crate) fn abstract_builtin_exec(
        this: JsObject<RegExp>,
        input: &JsString,
        context: &mut Context,
    ) -> JsResult<Option<JsObject>> {
        let regexp = this.clone();
        let this = this.upcast();

        // 1. Let length be the length of S.
        let length = input.len() as u64;

        // 2. Let lastIndex be ℝ(? ToLength(? Get(R, "lastIndex"))).
        let mut last_index = this
            .get(js_string!("lastIndex"), context)?
            .to_length(context)?;

        // The flags and the matcher are read after `lastIndex`, whose conversion can recompile
        // the RegExp through `RegExp.prototype.compile`.
        let rx = regexp.borrow().data().clone();

        // 3. Let flags be R.[[OriginalFlags]].
        let flags = &rx.original_flags;

        // 4. If flags contains "g", let global be true; else let global be false.
        let global = flags.contains(b'g');

        // 5. If flags contains "y", let sticky be true; else let sticky be false.
        let sticky = flags.contains(b'y');

        // 6. If flags contains "d", let hasIndices be true; else let hasIndices be false.
        let has_indices = flags.contains(b'd');

        // 7. If global is false and sticky is false, set lastIndex to 0.
        if !global && !sticky {
            last_index = 0;
        }

        // 8. Let matcher be R.[[RegExpMatcher]].
        let matcher = &rx.matcher;

        // 9. If flags contains "u" or flags contains "v", let fullUnicode be true; else let fullUnicode be false.
        let full_unicode = flags.contains(b'u') || flags.contains(b'v');

        // NOTE: The following steps are take care of by regress:
        //
        // SKIP: 10. Let matchSucceeded be false.
        // SKIP: 11. If fullUnicode is true, let input be StringToCodePoints(S). Otherwise, let input be a List whose elements are the code units that are the elements of S.
        // SKIP: 12. NOTE: Each element of input is considered to be a character.
        // SKIP: 13. Repeat, while matchSucceeded is false,

        // 13.a. If lastIndex > length, then
        if last_index > length {
            // i. If global is true or sticky is true, then
            if global || sticky {
                // 1. Perform ? Set(R, "lastIndex", +0𝔽, true).
                this.set(js_string!("lastIndex"), 0, true, context)?;
            }

            // ii. Return null.
            return Ok(None);
        }

        // 13.b. Let inputIndex be the index into input of the character that was obtained from element lastIndex of S.
        // 13.c. Let r be matcher(input, inputIndex).
        let r: Option<regress::Match> = match (full_unicode, input.as_str().variant()) {
            (true | false, JsStrVariant::Latin1(bytes)) => {
                if let Some(text) = bytes
                    .is_ascii()
                    .then(|| std::str::from_utf8(bytes).ok())
                    .flatten()
                {
                    // ASCII text is its own UTF-8 encoding, with one byte per code unit, so the
                    // UTF-8 matcher sees the same characters at the same indices without a copy.
                    matcher.find_from(text, last_index as usize).next()
                } else {
                    // NOTE: regress does not support latin1 encoding.
                    let input = input.to_vec();

                    // NOTE: We can use the faster ucs2 variant since there will never be two byte unicode.
                    matcher.find_from_ucs2(&input, last_index as usize).next()
                }
            }
            (true, JsStrVariant::Utf16(input)) => {
                matcher.find_from_utf16(input, last_index as usize).next()
            }
            (false, JsStrVariant::Utf16(input)) => {
                matcher.find_from_ucs2(input, last_index as usize).next()
            }
        };

        let Some(match_value) = r else {
            // d. If r is failure, then
            //
            // NOTE: Merged the following steps (since we no longer have a loop):
            //       13.d.i. If sticky is true, then
            //       13.a.i. If global is true or sticky is true, then
            if global || sticky {
                // 1. Perform ? Set(R, "lastIndex", +0𝔽, true).
                this.set(js_string!("lastIndex"), 0, true, context)?;
            }

            // MOVE: ii. Set lastIndex to AdvanceStringIndex(S, lastIndex, fullUnicode).
            // NOTE: Handled within the regress matches iterator, see below for last_index assignment.

            // NOTE: Merged  and  steps:
            //       13.a.ii.  Return null.
            //       13.d.i.2. Return null.
            return Ok(None);
        };

        // e. Else
        // SKIP: i. Assert: r is a MatchState.
        // SKIP: ii. Set matchSucceeded to true.

        // NOTE: regress currently doesn't support the sticky flag so we have to emulate it.
        if sticky && match_value.start() != last_index as usize {
            // 1. Perform ? Set(R, "lastIndex", +0𝔽, true).
            this.set(js_string!("lastIndex"), 0, true, context)?;

            // 2. Return null.
            return Ok(None);
        }

        // 13.d.ii. Set lastIndex to AdvanceStringIndex(S, lastIndex, fullUnicode).
        // NOTE: Calculation of last_index is done in regress.
        last_index = match_value.start() as u64;

        // 14. Let e be r's endIndex value.
        // 15. If fullUnicode is true, set e to GetStringIndex(S, e).
        // NOTE: Step 15 is already taken care of by regress.
        let e = match_value.end();

        #[cfg(feature = "annex-b")]
        Self::update_legacy_statics(&rx, input, last_index as usize, e, &match_value, context);

        // 16. If global is true or sticky is true, then
        if global || sticky {
            // a. Perform ? Set(R, "lastIndex", 𝔽(e), true).
            this.set(js_string!("lastIndex"), e, true, context)?;
        }

        // 17. Let n be the number of elements in r's captures List.
        let n = match_value.captures.len() as u64;
        // 18. Assert: n = R.[[RegExpRecord]].[[CapturingGroupsCount]].
        // 19. Assert: n < 232 - 1.
        debug_assert!(n < (1u64 << 32) - 1);

        // NOTE: A is created below, once its elements and its "groups" value are known, from a
        //       template holding "length", "index", "input" and "groups" in the order steps 20 to
        //       32 define them; nothing observable runs in between.

        // NOTE: The indices list is only observable through the "indices" property, so it is
        //       built only when hasIndices is true.
        let indices = if has_indices {
            // 24. Let match be the Match Record { [[StartIndex]]: lastIndex, [[EndIndex]]: e }.
            // Immediately convert it to an array according to 22.2.7.7 GetMatchIndexPair(S, match)
            // 1. Assert: match.[[StartIndex]] ≤ match.[[EndIndex]] ≤ the length of S.
            // 2. Return CreateArrayFromList(« 𝔽(match.[[StartIndex]]), 𝔽(match.[[EndIndex]]) »).
            let match_record = Array::create_array_from_list(
                [match_value.start().into(), match_value.end().into()],
                context,
            );

            // 25. Let indices be a new empty List.
            let indices = Array::array_create(n + 1, None, context)?;

            // 27. Append match to indices.
            indices.create_data_property_or_throw(0, match_record, context)?;
            Some(indices)
        } else {
            None
        };

        // 28. Let matchedSubstr be GetMatchString(S, match).
        let matched_substr = input.get_expect((last_index as usize)..(e));

        // 29. Perform ! CreateDataPropertyOrThrow(A, "0", matchedSubstr).
        let mut elements = ThinVec::with_capacity(n as usize + 1);
        elements.push(matched_substr.into());

        let named_groups = match_value
            .named_groups()
            .collect::<Vec<(&str, Option<Range>)>>();

        // Combines:
        // 26. Let groupNames be a new empty List.
        // 30. If R contains any GroupName, then
        // 31. Else,
        // 33. For each integer i such that 1 ≤ i ≤ n, in ascending order, do
        #[allow(clippy::if_not_else)]
        let (groups, group_names) = if !named_groups.clone().is_empty() {
            // a. Let groups be OrdinaryObjectCreate(null).
            let groups = JsObject::with_null_proto();
            let group_names = JsObject::with_null_proto();

            // e. If the ith capture of R was defined with a GroupName, then
            // i. Let s be the CapturingGroupName of that GroupName.
            // ii. Perform ! CreateDataPropertyOrThrow(groups, s, capturedValue).
            // iii. Append s to groupNames.
            for (name, range) in named_groups {
                let name = js_string!(name);
                // Of several groups with the same name (in different alternatives) the one
                // that participated in the match supplies the value.
                if range.is_none() && !groups.get(name.clone(), context)?.is_undefined() {
                    continue;
                }
                if let Some(range) = range {
                    let value = input.get_expect(range.clone());

                    groups.create_data_property_or_throw(name.clone(), value, context)?;

                    // 22.2.7.8 MakeMatchIndicesIndexPairArray ( S, indices, groupNames, hasGroups )
                    // a. Let matchIndices be indices[i].
                    // b. If matchIndices is not undefined, then
                    // i. Let matchIndexPair be GetMatchIndexPair(S, matchIndices).
                    // d. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(i)), matchIndexPair).
                    group_names.create_data_property_or_throw(
                        name.clone(),
                        Array::create_array_from_list(
                            [range.start.into(), range.end.into()],
                            context,
                        ),
                        context,
                    )?;
                } else {
                    groups.create_data_property_or_throw(
                        name.clone(),
                        JsValue::undefined(),
                        context,
                    )?;

                    // 22.2.7.8 MakeMatchIndicesIndexPairArray ( S, indices, groupNames, hasGroups )
                    // c. Else,
                    // i. Let matchIndexPair be undefined.
                    // d. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(i)), matchIndexPair).
                    group_names.create_data_property_or_throw(
                        name,
                        JsValue::undefined(),
                        context,
                    )?;
                }
            }

            (groups.into(), group_names.into())
        } else {
            // a. Let groups be undefined.
            (JsValue::undefined(), JsValue::undefined())
        };

        // 22.2.7.8 MakeMatchIndicesIndexPairArray ( S, indices, groupNames, hasGroups )
        // 8. Perform ! CreateDataPropertyOrThrow(A, "groups", groups).
        if let Some(indices) = &indices {
            indices.create_data_property_or_throw(js_string!("groups"), group_names, context)?;
        }

        // 27. For each integer i such that i ≥ 1 and i ≤ n, in ascending order, do
        for i in 1..=n {
            // a. Let captureI be ith element of r's captures List.
            let capture = match_value.group(i as usize);

            // b. If captureI is undefined, let capturedValue be undefined.
            // c. Else if fullUnicode is true, then
            // d. Else,
            let captured_value = capture.clone().map_or_else(JsValue::undefined, |range| {
                js_string!(input.get_expect(range)).into()
            });

            // e. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(i)), capturedValue).
            elements.push(captured_value);

            // 22.2.7.8 MakeMatchIndicesIndexPairArray ( S, indices, groupNames, hasGroups )
            if let Some(indices) = &indices {
                // b. If matchIndices is not undefined, then
                // i. Let matchIndexPair be GetMatchIndexPair(S, matchIndices).
                // c. Else,
                // i. Let matchIndexPair be undefined.
                let indices_range = capture.map_or_else(JsValue::undefined, |range| {
                    Array::create_array_from_list([range.start.into(), range.end.into()], context)
                        .into()
                });

                // d. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(i)), matchIndexPair).
                indices.create_data_property_or_throw(i, indices_range, context)?;
            }
        }

        // 20. Let A be ! ArrayCreate(n + 1).
        // 21. Assert: The mathematical value of A's "length" property is n + 1.
        // 22. Perform ! CreateDataPropertyOrThrow(A, "index", 𝔽(lastIndex)).
        // 23. Perform ! CreateDataPropertyOrThrow(A, "input", S).
        // 32. Perform ! CreateDataPropertyOrThrow(A, "groups", groups).
        let a = context
            .intrinsics()
            .templates()
            .regexp_match()
            .create_with_indexed_properties(
                Array,
                vec![
                    JsValue::new(n + 1),
                    JsValue::new(last_index),
                    input.clone().into(),
                    groups,
                ],
                IndexedProperties::from_dense_js_value(elements),
            );

        // 34. If hasIndices is true, then
        // a. Let indicesArray be MakeMatchIndicesIndexPairArray(S, indices, groupNames, hasGroups).
        // b. Perform ! CreateDataPropertyOrThrow(A, "indices", indicesArray).
        if let Some(indices) = indices {
            a.create_data_property_or_throw(js_string!("indices"), indices, context)?;
        }

        // 35. Return A.
        Ok(Some(a))
    }

    /// `RegExp.prototype[ @@match ]( string )`
    ///
    /// This method retrieves the matches when matching a string against a regular expression.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype-@@match
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/@@match
    pub(crate) fn r#match(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let rx be the this value.
        // 2. If rx is not an Object, throw a TypeError exception.
        let Some(rx) = this.as_object() else {
            return Err(JsNativeError::typ()
                .with_message("RegExp.prototype.match method called on incompatible value")
                .into());
        };

        // 3. Let S be ? ToString(string).
        let arg_str = args.get_or_undefined(0).to_string(context)?;

        // 4. Let flags be ? ToString(? Get(rx, "flags")).
        let flags = rx.get(js_string!("flags"), context)?.to_string(context)?;

        // 5. If flags does not contain "g", then
        if !flags.contains(b'g') {
            // a. Return ? RegExpExec(rx, S).
            return (Self::abstract_exec(&rx, arg_str, context)?)
                .map_or_else(|| Ok(JsValue::null()), |v| Ok(v.into()));
        }

        // 6. Else,

        // a. If flags contains "u" or flags contains "v", let fullUnicode be true. Otherwise, let fullUnicode be false.
        let full_unicode = flags.contains(b'u') || flags.contains(b'v');

        // b. Perform ? Set(rx, "lastIndex", +0𝔽, true).
        rx.set(js_string!("lastIndex"), 0, true, context)?;

        // c. Let A be ! ArrayCreate(0).
        let a = Array::array_create(0, None, context)?;

        // d. Let n be 0.
        let mut n = 0;

        // e. Repeat,
        loop {
            // i. Let result be ? RegExpExec(rx, S).
            let result = Self::abstract_exec(&rx, arg_str.clone(), context)?;

            // ii. If result is null, then
            // iii. Else,
            if let Some(result) = result {
                // 1. Let matchStr be ? ToString(? Get(result, "0")).
                let match_str = result.get(0, context)?.to_string(context)?;

                // 2. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(n)), matchStr).
                a.create_data_property_or_throw(n, match_str.clone(), context)?;

                // 3. If matchStr is the empty String, then
                if match_str.is_empty() {
                    // a. Let thisIndex be ℝ(? ToLength(? Get(rx, "lastIndex"))).
                    let this_index = rx
                        .get(js_string!("lastIndex"), context)?
                        .to_length(context)?;

                    // b. Let nextIndex be AdvanceStringIndex(S, thisIndex, fullUnicode).
                    let next_index = advance_string_index(&arg_str, this_index, full_unicode);

                    // c. Perform ? Set(rx, "lastIndex", 𝔽(nextIndex), true).
                    rx.set(
                        js_string!("lastIndex"),
                        JsValue::new(next_index),
                        true,
                        context,
                    )?;
                }

                // 4. Set n to n + 1.
                n += 1;
            } else {
                // 1. If n = 0, return null.
                if n == 0 {
                    return Ok(JsValue::null());
                }
                // 2. Return A.
                return Ok(a.into());
            }
        }
    }

    /// `RegExp.prototype.toString()`
    ///
    /// Return a string representing the regular expression.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype.tostring
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/toString
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_string(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let R be the this value.
        // 2. If R is not an Object, throw a TypeError exception.
        let regexp = this.as_object().ok_or_else(|| {
            JsNativeError::typ()
                .with_message("RegExp.prototype.toString method called on incompatible value")
        })?;

        // 3. Let pattern be ? ToString(? Get(R, "source")).
        let pattern = regexp
            .get(js_string!("source"), context)?
            .to_string(context)?;

        // 4. Let flags be ? ToString(? Get(R, "flags")).
        let flags = regexp
            .get(js_string!("flags"), context)?
            .to_string(context)?;

        // 5. Let result be the string-concatenation of "/", pattern, "/", and flags.
        // 6. Return result.
        Ok(js_string!(js_str!("/"), &pattern, js_str!("/"), &flags).into())
    }

    /// `RegExp.prototype[ @@matchAll ]( string )`
    ///
    /// The `[@@matchAll]` method returns all matches of the regular expression against a string.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp-prototype-matchall
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/@@matchAll
    pub(crate) fn match_all(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let R be the this value.
        // 2. If Type(R) is not Object, throw a TypeError exception.
        let regexp = this.as_object().ok_or_else(|| {
            JsNativeError::typ()
                .with_message("RegExp.prototype.match_all method called on incompatible value")
        })?;

        // 3. Let S be ? ToString(string).
        let arg_str = args.get_or_undefined(0).to_string(context)?;

        // 4. Let C be ? SpeciesConstructor(R, %RegExp%).
        let c = regexp.species_constructor(StandardConstructors::regexp, context)?;

        // 5. Let flags be ? ToString(? Get(R, "flags")).
        let flags = regexp
            .get(js_string!("flags"), context)?
            .to_string(context)?;

        // 6. Let matcher be ? Construct(C, « R, flags »).
        let matcher = c.construct(&[this.clone(), flags.clone().into()], Some(&c), context)?;

        // 7. Let lastIndex be ? ToLength(? Get(R, "lastIndex")).
        let last_index = regexp
            .get(js_string!("lastIndex"), context)?
            .to_length(context)?;

        // 8. Perform ? Set(matcher, "lastIndex", lastIndex, true).
        matcher.set(js_string!("lastIndex"), last_index, true, context)?;

        // 9. If flags contains "g", let global be true.
        // 10. Else, let global be false.
        let global = flags.contains(b'g');

        // 11. If flags contains "u", let fullUnicode be true.
        // 12. Else, let fullUnicode be false.
        let unicode = flags.contains(b'u') || flags.contains(b'v');

        // 13. Return ! CreateRegExpStringIterator(matcher, S, global, fullUnicode).
        Ok(RegExpStringIterator::create_regexp_string_iterator(
            matcher.clone(),
            arg_str,
            global,
            unicode,
            context,
        ))
    }

    /// `RegExp.prototype [ @@replace ] ( string, replaceValue )`
    ///
    /// The [@@replace]() method replaces some or all matches of a this pattern in a string by a replacement,
    /// and returns the result of the replacement as a new string.
    /// The replacement can be a string or a function to be called for each match.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype-@@replace
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/@@replace
    pub(crate) fn replace(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // Helper enum.
        enum CallableOrString {
            FunctionalReplace(JsObject),
            ReplaceValue(JsString),
        }

        // 1. Let rx be the this value.
        // 2. If rx is not an Object, throw a TypeError exception.
        let rx = this.as_object().ok_or_else(|| {
            JsNativeError::typ().with_message(
                "RegExp.prototype[Symbol.replace] method called on incompatible value",
            )
        })?;

        // 3. Let S be ? ToString(string).
        let s = args.get_or_undefined(0).to_string(context)?;

        // 4. Let lengthS be the length of S.
        let length_s = s.len();

        let replace_value = args.get_or_undefined(1);

        // 5. Let functionalReplace be IsCallable(replaceValue).
        let functional_replace = replace_value.as_callable();

        // 6. If functionalReplace is false, then
        let replace_value = if let Some(callable) = functional_replace {
            CallableOrString::FunctionalReplace(callable)
        } else {
            // a. Set replaceValue to ? ToString(replaceValue).
            CallableOrString::ReplaceValue(replace_value.to_string(context)?)
        };

        // 7. Let flags be ? ToString(? Get(rx, "flags")).
        let flags = rx.get(js_string!("flags"), context)?.to_string(context)?;

        // 8. If flags contains "g", let global be true. Otherwise, let global be false.
        let global = flags.contains(b'g');

        // 9. If global is true, then
        let full_unicode = if global {
            // a. If flags contains "u", let fullUnicode be true. Otherwise, let fullUnicode be false.
            let full_unicode = flags.contains(b'u');

            // b. Perform ? Set(rx, "lastIndex", +0𝔽, true).
            rx.set(js_string!("lastIndex"), 0, true, context)?;

            full_unicode
        } else {
            false
        };

        // 10. Let results be a new empty List.
        let mut results = Vec::new();

        // SKIPPED: 11. Let done be false.
        //
        // NOTE(HalidOdat): We don't keep track of `done`, we just break when done is true.

        // 12. Repeat, while done is false,
        loop {
            // a. Let result be ? RegExpExec(rx, S).
            let result = Self::abstract_exec(&rx, s.clone(), context)?;

            // b. If result is null, set done to true.
            let Some(result) = result else {
                // SKIPPED: 1. Set done to true.
                break;
            };

            // c. Else,
            //  i. Append result to results.
            results.push(result.clone());

            //  ii. If global is false, then
            if !global {
                // SKIPPED: 1. Set done to true.
                break;
            }

            //  iii. Else,
            //    1. Let matchStr be ? ToString(? Get(result, "0")).
            let match_str = result.get(0, context)?.to_string(context)?;

            //    2. If matchStr is the empty String, then
            if match_str.is_empty() {
                // a. Let thisIndex be ℝ(? ToLength(? Get(rx, "lastIndex"))).
                let this_index = rx
                    .get(js_string!("lastIndex"), context)?
                    .to_length(context)?;

                // b. Let nextIndex be AdvanceStringIndex(S, thisIndex, fullUnicode).
                let next_index = advance_string_index(&s, this_index, full_unicode);

                // c. Perform ? Set(rx, "lastIndex", 𝔽(nextIndex), true).
                rx.set(
                    js_string!("lastIndex"),
                    JsValue::new(next_index),
                    true,
                    context,
                )?;
            }
        }

        // 16. If nextSourcePosition ≥ lengthS, return accumulatedResult.
        // 17. Return the string-concatenation of accumulatedResult and the substring of S from nextSourcePosition.

        // 13. Let accumulatedResult be the empty String.
        let mut accumulated_result = vec![];

        // 14. Let nextSourcePosition be 0.
        let mut next_source_position = 0;

        // 15. For each element result of results, do
        for result in results {
            // a. Let resultLength be ? LengthOfArrayLike(result).
            let result_length = result.length_of_array_like(context)? as i64;

            // b. Let nCaptures be max(resultLength - 1, 0).
            let n_captures = std::cmp::max(result_length - 1, 0);

            // c. Let matched be ? ToString(? Get(result, "0")).
            let matched = result.get(0, context)?.to_string(context)?;

            // d. Let matchLength be the length of matched.
            let match_length = matched.len();

            // e. Let position be ? ToIntegerOrInfinity(? Get(result, "index")).
            let position = result
                .get(js_string!("index"), context)?
                .to_integer_or_infinity(context)?;

            // f. Set position to the result of clamping position between 0 and lengthS.
            let position = position.clamp_finite(0, length_s as i64) as usize;

            // g. Let captures be a new empty List.
            let mut captures = Vec::new();

            // h. Let n be 1.
            // i. Repeat, while n ≤ nCaptures,
            for n in 1..=n_captures {
                // i. Let capN be ? Get(result, ! ToString(𝔽(n))).
                let mut cap_n = result.get(n, context)?;

                // ii. If capN is not undefined, then
                if !cap_n.is_undefined() {
                    // 1. Set capN to ? ToString(capN).
                    cap_n = cap_n.to_string(context)?.into();
                }

                // iii. Append capN to captures.
                captures.push(cap_n);

                // iv. NOTE: When n = 1, the preceding step puts the first element into captures (at index 0).
                //     More generally, the nth capture (the characters captured by the nth set of capturing parentheses)
                //     is at captures[n - 1].
                //
                // v. Set n to n + 1.
            }

            // j. Let namedCaptures be ? Get(result, "groups").
            let mut named_captures = result.get(js_string!("groups"), context)?;

            let replacement = match replace_value {
                // k. If functionalReplace is true, then
                CallableOrString::FunctionalReplace(ref replace_value) => {
                    // i. Let replacerArgs be the list-concatenation of « matched », captures, and « 𝔽(position), S ».
                    let mut replacer_args = vec![JsValue::new(matched)];
                    replacer_args.extend(captures);
                    replacer_args.push(position.into());
                    replacer_args.push(s.clone().into());

                    // ii. If namedCaptures is not undefined, then
                    if !named_captures.is_undefined() {
                        // 1. Append namedCaptures to replacerArgs.
                        replacer_args.push(named_captures);
                    }

                    // iii. Let replValue be ? Call(replaceValue, undefined, replacerArgs).
                    let repl_value =
                        replace_value.call(&JsValue::undefined(), &replacer_args, context)?;

                    // iv. Let replacement be ? ToString(replValue).
                    repl_value.to_string(context)?
                }
                // l. Else,
                CallableOrString::ReplaceValue(ref replace_value) => {
                    // i. If namedCaptures is not undefined, then
                    if !named_captures.is_undefined() {
                        // 1. Set namedCaptures to ? ToObject(namedCaptures).
                        named_captures = named_captures.to_object(context)?.into();
                    }

                    // ii. Let replacement be ? GetSubstitution(matched, S, position, captures, namedCaptures, replaceValue).
                    string::get_substitution(
                        &matched,
                        &s,
                        position,
                        &captures,
                        &named_captures,
                        replace_value,
                        context,
                    )?
                }
            };

            // m. If position ≥ nextSourcePosition, then
            if position >= next_source_position {
                // i. NOTE: position should not normally move backwards.
                //    If it does, it is an indication of an ill-behaving RegExp subclass or use of
                //    an access triggered side-effect to change the global flag or other characteristics of rx.
                //    In such cases, the corresponding substitution is ignored.

                // ii. Set accumulatedResult to the string-concatenation of accumulatedResult, the substring of S from nextSourcePosition to position, and replacement.
                accumulated_result.extend(s.get_expect(next_source_position..position).iter());
                accumulated_result.extend(replacement.iter());

                // iii. Set nextSourcePosition to position + matchLength.
                next_source_position = position + match_length;
            }
        }

        // 16. If nextSourcePosition ≥ lengthS, return accumulatedResult.
        if next_source_position >= length_s {
            return Ok(js_string!(&accumulated_result[..]).into());
        }

        // 17. Return the string-concatenation of accumulatedResult and the substring of S from nextSourcePosition.
        Ok(js_string!(
            &JsString::from(&accumulated_result[..]),
            &s.get_expect(next_source_position..)
        )
        .into())
    }

    /// `RegExp.prototype[ @@search ]( string )`
    ///
    /// This method executes a search for a match between a this regular expression and a string.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype-@@search
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/@@search
    pub(crate) fn search(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let rx be the this value.
        // 2. If Type(rx) is not Object, throw a TypeError exception.
        let rx = this.as_object().ok_or_else(|| {
            JsNativeError::typ()
                .with_message("RegExp.prototype[Symbol.search] method called on incompatible value")
        })?;

        // 3. Let S be ? ToString(string).
        let arg_str = args.get_or_undefined(0).to_string(context)?;

        // 4. Let previousLastIndex be ? Get(rx, "lastIndex").
        let previous_last_index = rx.get(js_string!("lastIndex"), context)?;

        // 5. If SameValue(previousLastIndex, +0𝔽) is false, then
        if !JsValue::same_value(&previous_last_index, &JsValue::new(0)) {
            // a. Perform ? Set(rx, "lastIndex", +0𝔽, true).
            rx.set(js_string!("lastIndex"), 0, true, context)?;
        }

        // 6. Let result be ? RegExpExec(rx, S).
        let result = Self::abstract_exec(&rx, arg_str, context)?;

        // 7. Let currentLastIndex be ? Get(rx, "lastIndex").
        let current_last_index = rx.get(js_string!("lastIndex"), context)?;

        // 8. If SameValue(currentLastIndex, previousLastIndex) is false, then
        if !JsValue::same_value(&current_last_index, &previous_last_index) {
            // a. Perform ? Set(rx, "lastIndex", previousLastIndex, true).
            rx.set(js_string!("lastIndex"), previous_last_index, true, context)?;
        }

        // 9. If result is null, return -1𝔽.
        // 10. Return ? Get(result, "index").
        result.map_or_else(
            || Ok(JsValue::new(-1)),
            |result| result.get(js_string!("index"), context),
        )
    }

    /// `RegExp.prototype [ @@split ] ( string, limit )`
    ///
    /// The [@@split]() method splits a String object into an array of strings by separating the string into substrings.
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///  - [MDN documentation][mdn]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype-@@split
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/@@split
    pub(crate) fn split(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let rx be the this value.
        // 2. If Type(rx) is not Object, throw a TypeError exception.
        let rx = this.as_object().ok_or_else(|| {
            JsNativeError::typ()
                .with_message("RegExp.prototype.split method called on incompatible value")
        })?;

        // 3. Let S be ? ToString(string).
        let arg_str = args.get_or_undefined(0).to_string(context)?;

        // 4. Let C be ? SpeciesConstructor(rx, %RegExp%).
        let constructor = rx.species_constructor(StandardConstructors::regexp, context)?;

        // 5. Let flags be ? ToString(? Get(rx, "flags")).
        let flags = rx.get(js_string!("flags"), context)?.to_string(context)?;

        // 6. If flags contains "u", let unicodeMatching be true.
        // 7. Else, let unicodeMatching be false.
        let unicode = flags.contains(b'u');

        // 8. If flags contains "y", let newFlags be flags.
        // 9. Else, let newFlags be the string-concatenation of flags and "y".
        let new_flags = if flags.contains(b'y') {
            flags
        } else {
            js_string!(&flags, js_str!("y"))
        };

        // 10. Let splitter be ? Construct(C, « rx, newFlags »).
        let splitter = constructor.construct(
            &[this.clone(), new_flags.into()],
            Some(&constructor),
            context,
        )?;

        // 11. Let A be ! ArrayCreate(0).
        let a = Array::array_create(0, None, context)?;

        // 12. Let lengthA be 0.
        let mut length_a = 0;

        // 13. If limit is undefined, let lim be 2^32 - 1; else let lim be ℝ(? ToUint32(limit)).
        let limit = args.get_or_undefined(1);
        let lim = if limit.is_undefined() {
            u32::MAX
        } else {
            limit.to_u32(context)?
        };

        // 14. If lim is 0, return A.
        if lim == 0 {
            return Ok(a.into());
        }

        // 15. Let size be the length of S.
        let size = arg_str.len() as u64;

        // 16. If size is 0, then
        if size == 0 {
            // a. Let z be ? RegExpExec(splitter, S).
            let result = Self::abstract_exec(&splitter, arg_str.clone(), context)?;

            // b. If z is not null, return A.
            if result.is_some() {
                return Ok(a.into());
            }

            // c. Perform ! CreateDataPropertyOrThrow(A, "0", S).
            a.create_data_property_or_throw(0, arg_str, context)?;

            // d. Return A.
            return Ok(a.into());
        }

        // 17. Let p be 0.
        // 18. Let q be p.
        let mut p = 0;
        let mut q = p;

        // 19. Repeat, while q < size,
        while q < size {
            // a. Perform ? Set(splitter, "lastIndex", 𝔽(q), true).
            splitter.set(js_string!("lastIndex"), JsValue::new(q), true, context)?;

            // b. Let z be ? RegExpExec(splitter, S).
            let result = Self::abstract_exec(&splitter, arg_str.clone(), context)?;

            // c. If z is null, set q to AdvanceStringIndex(S, q, unicodeMatching).
            // d. Else,
            if let Some(result) = result {
                // i. Let e be ℝ(? ToLength(? Get(splitter, "lastIndex"))).
                let mut e = splitter
                    .get(js_string!("lastIndex"), context)?
                    .to_length(context)?;

                // ii. Set e to min(e, size).
                e = std::cmp::min(e, size);

                // iii. If e = p, set q to AdvanceStringIndex(S, q, unicodeMatching).
                // iv. Else,
                if e == p {
                    q = advance_string_index(&arg_str, q, unicode);
                } else {
                    // 1. Let T be the substring of S from p to q.
                    let arg_str_substring = arg_str.get_expect(p as usize..q as usize);

                    // 2. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(lengthA)), T).
                    a.create_data_property_or_throw(length_a, arg_str_substring, context)?;

                    // 3. Set lengthA to lengthA + 1.
                    length_a += 1;

                    // 4. If lengthA = lim, return A.
                    if length_a == lim {
                        return Ok(a.into());
                    }

                    // 5. Set p to e.
                    p = e;

                    // 6. Let numberOfCaptures be ? LengthOfArrayLike(z).
                    let mut number_of_captures = result.length_of_array_like(context)? as isize;

                    // 7. Set numberOfCaptures to max(numberOfCaptures - 1, 0).
                    number_of_captures = std::cmp::max(number_of_captures - 1, 0);

                    // 8. Let i be 1.
                    // 9. Repeat, while i ≤ numberOfCaptures,
                    for i in 1..=number_of_captures {
                        // a. Let nextCapture be ? Get(z, ! ToString(𝔽(i))).
                        let next_capture = result.get(i, context)?;

                        // b. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(lengthA)), nextCapture).
                        a.create_data_property_or_throw(length_a, next_capture, context)?;

                        // d. Set lengthA to lengthA + 1.
                        length_a += 1;

                        // e. If lengthA = lim, return A.
                        if length_a == lim {
                            return Ok(a.into());
                        }
                    }

                    // 10. Set q to p.
                    q = p;
                }
            } else {
                q = advance_string_index(&arg_str, q, unicode);
            }
        }

        // 20. Let T be the substring of S from p to size.
        let arg_str_substring = arg_str.get_expect(p as usize..size as usize);

        // 21. Perform ! CreateDataPropertyOrThrow(A, ! ToString(𝔽(lengthA)), T).
        a.create_data_property_or_throw(length_a, arg_str_substring, context)?;

        // 22. Return A.
        Ok(a.into())
    }

    /// [`RegExp.prototype.compile ( pattern, flags )`][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-regexp.prototype.compile
    #[cfg(feature = "annex-b")]
    fn compile(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. Perform ? RequireInternalSlot(O, [[RegExpMatcher]]).

        let this = this
            .as_object()
            .filter(|o| o.is::<RegExp>())
            .ok_or_else(|| {
                JsNativeError::typ()
                    .with_message("`RegExp.prototype.compile` cannot be called for a non-object")
            })?;
        let (realm_id, legacy_features) = {
            let regexp = this
                .downcast_ref::<RegExp>()
                .js_expect("already checked that the object was a RegExp")?;
            (regexp.realm_id, regexp.legacy_features)
        };
        // 3. Let thisRealm be the current Realm Record.
        // 4. Let oRealm be O.[[Realm]].
        // 5. If SameValue(thisRealm, oRealm) is false, throw a TypeError exception.
        // 6. If O.[[LegacyFeaturesEnabled]] is false, throw a TypeError exception.
        if realm_id != context.realm().id() || !legacy_features {
            return Err(JsNativeError::typ()
                .with_message(
                    "`RegExp.prototype.compile` cannot be called on a RegExp from another realm or a RegExp subclass",
                )
                .into());
        }
        let pattern = args.get_or_undefined(0);
        let flags = args.get_or_undefined(1);
        // 3. If pattern is an Object and pattern has a [[RegExpMatcher]] internal slot, then
        let (pattern, flags) = if let Some((p, f)) = pattern.as_object().and_then(|o| {
            o.downcast_ref::<RegExp>()
                .map(|rx| (rx.original_source.clone(), rx.original_flags.clone()))
        }) {
            //     a. If flags is not undefined, throw a TypeError exception.
            if !flags.is_undefined() {
                return Err(JsNativeError::typ()
                    .with_message(
                        "`RegExp.prototype.compile` cannot be \
                called with both a RegExp initializer and new flags",
                    )
                    .into());
            }
            //     b. Let P be pattern.[[OriginalSource]].
            //     c. Let F be pattern.[[OriginalFlags]].
            (p.into(), f.into())
        } else {
            // 4. Else,
            //     a. Let P be pattern.
            //     b. Let F be flags.
            (pattern.clone(), flags.clone())
        };

        let mut regexp = Self::compile_native_regexp(&pattern, &flags, context)?;
        regexp.realm_id = realm_id;
        regexp.legacy_features = legacy_features;

        // 5. Return ? RegExpInitialize(O, P, F).
        {
            *this
                .downcast_mut::<RegExp>()
                .expect("already checked that the object was a RegExp") = regexp;
        }

        this.set(js_string!("lastIndex"), 0, true, context)?;

        Ok(this.into())
    }
}

#[cfg(feature = "annex-b")]
impl RegExp {
    /// The legacy RegExp features' steps of `RegExpBuiltinExec`: a successful match updates the
    /// legacy static properties of the current realm's `%RegExp%`, or invalidates them when the
    /// `RegExp` belongs to a subclass.
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://github.com/tc39/proposal-regexp-legacy-features#regexpbuiltinexec--r-s-
    fn update_legacy_statics(
        rx: &RegExp,
        input: &JsString,
        start: usize,
        end: usize,
        match_value: &regress::Match,
        context: &mut Context,
    ) {
        // a. Let thisRealm be the current Realm Record.
        // b. Let rRealm be the value of R's [[Realm]] internal slot.
        // c. If SameValue(thisRealm, rRealm) is true, then
        let realm = context.realm().clone();
        if realm.id() != rx.realm_id {
            return;
        }
        let mut statics = realm.legacy_regexp().borrow_mut();
        // i. If the value of R's [[LegacyFeaturesEnabled]] internal slot is true, then
        if rx.legacy_features {
            // 1. Perform UpdateLegacyRegExpStaticProperties(%RegExp%, S, lastIndex, e, capturedValues).
            // NOTE: The captured values are kept as ranges of S and sliced when read.
            let statics = statics.get_or_insert_with(LegacyRegExpStatics::default);
            statics.input = input.clone();
            statics.matched = input.clone();
            statics.start = start;
            statics.end = end;
            statics.captures.clear();
            statics.captures.extend_from_slice(&match_value.captures);
        } else {
            // ii. Else, perform InvalidateLegacyRegExpStaticProperties(%RegExp%).
            *statics = None;
        }
    }

    /// `GetLegacyRegExpStaticProperty ( C, thisValue, internalSlotName )` for `C` = `%RegExp%`.
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://github.com/tc39/proposal-regexp-legacy-features#getlegacyregexpstaticproperty-c-thisvalue-internalslotname-
    fn get_legacy_static(
        this: &JsValue,
        slot: LegacySlot,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Assert C is an object that has an internal slot named internalSlotName.
        // 2. If SameValue(C, thisValue) is false, throw a TypeError exception.
        let constructor = context.intrinsics().constructors().regexp().constructor();
        if !this
            .as_object()
            .is_some_and(|this| JsObject::equals(&this, &constructor))
        {
            return Err(JsNativeError::typ()
                .with_message("legacy RegExp static properties can only be read from %RegExp%")
                .into());
        }
        // 3. Let val be the value of the internal slot of C named internalSlotName.
        // 4. If val is empty, throw a TypeError exception.
        // 5. Return val.
        let realm = context.realm().clone();
        let statics = realm.legacy_regexp().borrow();
        let Some(statics) = statics.as_ref() else {
            return Err(JsNativeError::typ()
                .with_message(
                    "the legacy RegExp static properties were invalidated by a RegExp subclass",
                )
                .into());
        };
        Ok(statics.value(slot).into())
    }

    /// `set RegExp.input`: `SetLegacyRegExpStaticProperty(%RegExp%, this value, [[RegExpInput]], val)`.
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://github.com/tc39/proposal-regexp-legacy-features#setlegacyregexpstaticproperty-c-thisvalue-internalslotname-val-
    fn set_legacy_input(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Assert C is an object that has an internal slot named internalSlotName.
        // 2. If SameValue(C, thisValue) is false, throw a TypeError exception.
        let constructor = context.intrinsics().constructors().regexp().constructor();
        if !this
            .as_object()
            .is_some_and(|this| JsObject::equals(&this, &constructor))
        {
            return Err(JsNativeError::typ()
                .with_message("legacy RegExp static properties can only be set on %RegExp%")
                .into());
        }
        // 3. Let strVal be ? ToString(val).
        let value = args.get_or_undefined(0).to_string(context)?;
        // 4. Set the value of the internal slot of C named internalSlotName to strVal.
        let realm = context.realm().clone();
        realm
            .legacy_regexp()
            .borrow_mut()
            .get_or_insert_with(LegacyRegExpStatics::default)
            .input = value;
        Ok(JsValue::undefined())
    }
}

/// `22.2.5.2.3 AdvanceStringIndex ( S, index, unicode )`
///
/// More information:
///  - [ECMAScript reference][spec]
///
/// [spec]: https://tc39.es/ecma262/#sec-advancestringindex
fn advance_string_index(s: &JsString, index: u64, unicode: bool) -> u64 {
    // Regress only works with utf8, so this function differs from the spec.

    // 1. Assert: index ≤ 2^53 - 1.

    // 2. If unicode is false, return index + 1.
    if !unicode {
        return index + 1;
    }

    // 3. Let length be the number of code units in S.
    let length = s.len() as u64;

    // 4. If index + 1 ≥ length, return index + 1.
    if index + 1 > length {
        return index + 1;
    }

    // 5. Let cp be ! CodePointAt(S, index).
    let code_point = s.code_point_at(index as usize);

    index + code_point.code_unit_count() as u64
}

/// A capturing group name or a `\k<name>` backreference found in a pattern: the range of code
/// points of the name, and the number of the group (zero for a backreference).
struct PatternName {
    start: usize,
    end: usize,
    group: usize,
}

/// Rewrites a pattern for `regress` where it lacks ECMAScript semantics:
///
/// - a backreference to a group name that several groups share (duplicate named groups, in
///   different alternatives) becomes a sequence of numbered backreferences to all of them, since
///   a backreference to a group that did not participate matches the empty string;
/// - without the `u` or `v` flag, a supplementary code point written literally in a group name
///   becomes a `\u{...}` escape, so the name survives the conversion to UTF-16 code units.
fn prepare_pattern(pattern: Vec<u32>, full_unicode: bool, unicode_sets: bool) -> Vec<u32> {
    const BACKSLASH: u32 = '\\' as u32;
    let names = scan_pattern_names(&pattern, unicode_sets);
    let groups_named = |name: &[u32]| -> Vec<usize> {
        names
            .iter()
            .filter(|n| n.group != 0 && pattern[n.start..n.end] == *name)
            .map(|n| n.group)
            .collect()
    };
    let rewrite_backreference =
        |n: &PatternName| n.group == 0 && groups_named(&pattern[n.start..n.end]).len() > 1;
    let escape_name =
        |n: &PatternName| !full_unicode && pattern[n.start..n.end].iter().any(|&c| c > 0xFFFF);
    if !names
        .iter()
        .any(|n| rewrite_backreference(n) || escape_name(n))
    {
        return pattern;
    }

    let mut out = Vec::with_capacity(pattern.len() + 16);
    let mut position = 0;
    for name in &names {
        if rewrite_backreference(name) {
            // `\k<` precedes the name and `>` follows it.
            out.extend_from_slice(&pattern[position..name.start - 3]);
            out.extend("(?:".chars().map(u32::from));
            for group in groups_named(&pattern[name.start..name.end]) {
                out.push(BACKSLASH);
                out.extend(group.to_string().chars().map(u32::from));
            }
            out.push(u32::from(')'));
            position = name.end + 1;
        } else if escape_name(name) {
            out.extend_from_slice(&pattern[position..name.start]);
            for &c in &pattern[name.start..name.end] {
                if c > 0xFFFF {
                    out.extend(format!("\\u{{{c:x}}}").chars().map(u32::from));
                } else {
                    out.push(c);
                }
            }
            position = name.end;
        }
    }
    out.extend_from_slice(&pattern[position..]);
    out
}

/// Finds the capturing group names and the `\k<name>` backreferences of a pattern, numbering
/// the capturing groups from the left.
fn scan_pattern_names(pattern: &[u32], unicode_sets: bool) -> Vec<PatternName> {
    let at = |index: usize| pattern.get(index).copied().and_then(char::from_u32);
    let name_end = |start: usize| (start..pattern.len()).find(|&i| at(i) == Some('>'));
    let mut names = Vec::new();
    let mut groups = 0;
    let mut class_depth = 0usize;
    let mut i = 0;
    while i < pattern.len() {
        match at(i) {
            Some('\\') => {
                if class_depth == 0
                    && at(i + 1) == Some('k')
                    && at(i + 2) == Some('<')
                    && let Some(end) = name_end(i + 3)
                {
                    names.push(PatternName {
                        start: i + 3,
                        end,
                        group: 0,
                    });
                    i = end + 1;
                    continue;
                }
                i += 2;
                continue;
            }
            Some('[') => {
                if class_depth == 0 || unicode_sets {
                    class_depth += 1;
                }
            }
            Some(']') if class_depth > 0 => class_depth -= 1,
            Some('(') if class_depth == 0 => {
                if at(i + 1) != Some('?') {
                    groups += 1;
                } else if at(i + 2) == Some('<')
                    && !matches!(at(i + 3), Some('=' | '!'))
                    && let Some(end) = name_end(i + 3)
                {
                    groups += 1;
                    names.push(PatternName {
                        start: i + 3,
                        end,
                        group: groups,
                    });
                    i = end + 1;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    names
}
