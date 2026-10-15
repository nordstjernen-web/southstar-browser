//! Boa's implementation of ECMAScript's `WeakRef` object.

pub(crate) mod table;
mod weak_ref;

pub(crate) use weak_ref::WeakRef;
