//! This module implements the `ForInIterator` object.
//!
//! More information:
//!  - [ECMAScript reference][spec]
//!
//! [spec]: https://tc39.es/ecma262/#sec-for-in-iterator-objects

use crate::{
    Context, JsData, JsExpect, JsResult, JsString, JsValue,
    object::{JsObject, internal_methods::InternalMethodPropertyContext},
    property::PropertyKey,
};
use boa_gc::{Finalize, Trace};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

/// Below this many visited keys a linear scan beats building a hash set.
const VISITED_SCAN_LIMIT: usize = 16;

/// The `ForInIterator` object represents an iteration over some specific object.
///
/// The iterator is internal to a `for-in` loop and never reachable from script, so the VM steps
/// it directly through [`ForInIterator::step`] instead of calling a `next` method and reading a
/// result object for every key.
///
/// More information:
///  - [ECMAScript reference][spec]
///
/// [spec]: https://tc39.es/ecma262/#sec-for-in-iterator-objects
#[derive(Debug, Clone, Finalize, Trace, JsData)]
pub(crate) struct ForInIterator {
    object: JsValue,
    #[unsafe_ignore_trace]
    visited_keys: Vec<PropertyKey>,
    #[unsafe_ignore_trace]
    visited_set: FxHashSet<PropertyKey>,
    #[unsafe_ignore_trace]
    remaining_keys: VecDeque<PropertyKey>,
    object_was_visited: bool,
}

impl ForInIterator {
    fn new(object: JsValue) -> Self {
        Self {
            object,
            visited_keys: Vec::new(),
            visited_set: FxHashSet::default(),
            remaining_keys: VecDeque::default(),
            object_was_visited: false,
        }
    }

    /// `CreateForInIterator( object )`
    ///
    /// Creates a new iterator over the given object.
    ///
    /// More information:
    ///  - [ECMA reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-createforiniterator
    pub(crate) fn create_for_in_iterator(object: JsValue, context: &Context) -> JsObject {
        JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            context.intrinsics().constructors().iterator().prototype(),
            Self::new(object),
        )
        .upcast()
    }

    fn was_visited(&mut self, key: &PropertyKey) -> bool {
        if self.visited_keys.len() <= VISITED_SCAN_LIMIT {
            return self.visited_keys.contains(key);
        }
        if self.visited_set.len() < self.visited_keys.len() {
            self.visited_set
                .extend(self.visited_keys[self.visited_set.len()..].iter().cloned());
        }
        self.visited_set.contains(key)
    }

    /// `%ForInIteratorPrototype%.next( )`
    ///
    /// Returns the next enumerable key of the iterated object or its prototype chain, or `None`
    /// once every key was visited.
    ///
    /// More information:
    ///  - [ECMA reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-%foriniteratorprototype%.next
    pub(crate) fn step(iterator: &JsObject, context: &mut Context) -> JsResult<Option<JsValue>> {
        let mut iterator = iterator
            .downcast_mut::<Self>()
            .js_expect("for-in iterator must be a ForInIterator")?;
        let mut object = iterator.object.to_object(context)?;
        loop {
            if !iterator.object_was_visited {
                let keys = object
                    .__own_property_keys__(&mut InternalMethodPropertyContext::new(context))?;
                iterator.remaining_keys.extend(
                    keys.into_iter()
                        .filter(|key| !matches!(key, PropertyKey::Symbol(_))),
                );
                iterator.object_was_visited = true;
            }
            while let Some(key) = iterator.remaining_keys.pop_front() {
                if iterator.was_visited(&key) {
                    continue;
                }
                let Some(desc) = object
                    .__get_own_property__(&key, &mut InternalMethodPropertyContext::new(context))?
                else {
                    continue;
                };
                let name = match &key {
                    PropertyKey::String(name) => name.clone(),
                    PropertyKey::Index(index) => JsString::from(index.get()),
                    PropertyKey::Symbol(_) => continue,
                };
                iterator.visited_keys.push(key);
                if desc.expect_enumerable() {
                    return Ok(Some(JsValue::new(name)));
                }
            }
            let Some(proto) = object.prototype().clone() else {
                return Ok(None);
            };
            object = proto;
            iterator.object = JsValue::new(object.clone());
            iterator.object_was_visited = false;
        }
    }
}
