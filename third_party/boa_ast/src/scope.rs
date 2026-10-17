//! This module implements the binding scope for various AST nodes.
//!
//! Scopes are used to track the bindings of identifiers in the AST.

use bitflags::bitflags;
use boa_string::JsString;
use rustc_hash::FxHashMap;
use std::{
    cell::{Cell, RefCell},
    fmt::Debug,
    rc::Rc,
};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct BindingFlags: u8 {
        const MUTABLE  = 1 << 0;
        const LEX      = 1 << 1;
        const STRICT   = 1 << 2;
        const ESCAPES  = 1 << 3;
        const ACCESSED = 1 << 4;
        const DELETABLE = 1 << 5;
    }
}

impl BindingFlags {
    fn is_mutable(self) -> bool {
        self.contains(BindingFlags::MUTABLE)
    }
    fn is_lex(self) -> bool {
        self.contains(BindingFlags::LEX)
    }
    fn is_strict(self) -> bool {
        self.contains(BindingFlags::STRICT)
    }
    fn escapes(self) -> bool {
        self.contains(BindingFlags::ESCAPES)
    }
    fn is_accessed(self) -> bool {
        self.contains(BindingFlags::ACCESSED)
    }
    fn is_deletable(self) -> bool {
        self.contains(BindingFlags::DELETABLE)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Binding {
    name: JsString,
    index: u32,
    flags: BindingFlags,
}

impl Binding {
    fn is_mutable(&self) -> bool {
        self.flags.is_mutable()
    }
    fn is_lex(&self) -> bool {
        self.flags.is_lex()
    }
    fn is_strict(&self) -> bool {
        self.flags.is_strict()
    }
    fn escapes(&self) -> bool {
        self.flags.escapes()
    }
    fn is_accessed(&self) -> bool {
        self.flags.is_accessed()
    }
    fn is_deletable(&self) -> bool {
        self.flags.is_deletable()
    }
}

/// A scope maps bound identifiers to their binding positions.
///
/// It can be either a global scope or a function scope or a declarative scope.
#[derive(Clone, PartialEq)]
pub struct Scope {
    inner: Rc<Inner>,
}

impl Debug for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scope")
            .field("outer", &self.inner.outer)
            .field("index", &self.inner.index)
            .field("bindings", &self.inner.bindings)
            .field("function", &self.inner.function)
            .finish()
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new_global()
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Scope {
    fn arbitrary(_u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new_global())
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Inner {
    unique_id: u32,
    outer: Option<Scope>,
    index: Cell<u32>,
    bindings: RefCell<Vec<Binding>>,
    positions: RefCell<FxHashMap<JsString, usize>>,
    function: bool,
    // Is this the scope of the parameter of a `catch` clause.
    catch_clause: bool,
    // Has the `this` been accessed/escaped outside the function environment boundary.
    this_escaped: Cell<bool>,
    // Is this the function scope of an arrow function, which has no `this` of its own.
    lexical_this: Cell<bool>,
    // Has a direct `eval` added a deletable binding to this scope.
    has_deletable_bindings: Cell<bool>,

    context: Rc<ScopeContext>,
}

impl Scope {
    /// Creates a new global scope.
    #[must_use]
    pub fn new_global() -> Self {
        Self {
            inner: Rc::new(Inner {
                unique_id: 0,
                outer: None,
                index: Cell::default(),
                bindings: RefCell::default(),
                positions: RefCell::default(),
                function: true,
                catch_clause: false,
                this_escaped: Cell::new(false),
                lexical_this: Cell::new(false),
                has_deletable_bindings: Cell::new(false),
                context: Rc::default(),
            }),
        }
    }

    /// Creates a new scope.
    #[must_use]
    pub fn new(parent: Self, function: bool) -> Self {
        Self::with_kind(parent, function, false)
    }

    /// Creates the scope of the parameter of a `catch` clause.
    #[must_use]
    pub fn new_catch(parent: Self) -> Self {
        Self::with_kind(parent, false, true)
    }

    fn with_kind(parent: Self, function: bool, catch_clause: bool) -> Self {
        Self {
            inner: Rc::new(Inner {
                unique_id: parent.inner.context.next_unique_id(),
                index: Cell::new(parent.inner.index.get() + 1),
                bindings: RefCell::default(),
                positions: RefCell::default(),
                function,
                catch_clause,
                this_escaped: Cell::new(false),
                lexical_this: Cell::new(false),
                has_deletable_bindings: Cell::new(false),
                context: parent.inner.context.clone(),
                outer: Some(parent),
            }),
        }
    }

    fn position(&self, name: &JsString) -> Option<usize> {
        self.inner.positions.borrow().get(name).copied()
    }

    fn with_binding<T>(&self, name: &JsString, f: impl FnOnce(&Binding) -> T) -> Option<T> {
        let position = self.position(name)?;
        Some(f(&self.inner.bindings.borrow()[position]))
    }

    fn push_binding(&self, binding: Binding) {
        let mut bindings = self.inner.bindings.borrow_mut();
        self.inner
            .positions
            .borrow_mut()
            .insert(binding.name.clone(), bindings.len());
        bindings.push(binding);
    }

    /// Marks this function scope as the scope of an arrow function, whose `this` is the one of
    /// its enclosing function.
    pub fn set_lexical_this(&self) {
        self.inner.lexical_this.set(true);
    }

    /// Returns `true` if this is the scope of the parameter of a `catch` clause.
    #[must_use]
    pub fn is_catch_clause(&self) -> bool {
        self.inner.catch_clause
    }

    /// Checks if the scope has only local bindings.
    #[must_use]
    pub fn all_bindings_local(&self) -> bool {
        // if self.inner.function && self.inn
        self.inner
            .bindings
            .borrow()
            .iter()
            .all(|binding| !binding.escapes())
    }

    /// Marks all bindings in this scope as escaping.
    pub fn escape_all_bindings(&self) {
        for binding in self.inner.bindings.borrow_mut().iter_mut() {
            binding.flags.insert(BindingFlags::ESCAPES);
        }
    }

    /// Has this binding escaped.
    #[must_use]
    pub fn escaped_this(&self) -> bool {
        self.inner.this_escaped.get()
    }

    /// Check if the scope has a lexical binding with the given name.
    #[must_use]
    pub fn has_lex_binding(&self, name: &JsString) -> bool {
        self.with_binding(name, Binding::is_lex).unwrap_or(false)
    }

    /// Check if the scope has a binding with the given name.
    #[must_use]
    pub fn has_binding(&self, name: &JsString) -> bool {
        self.position(name).is_some()
    }

    /// Get the binding locator for a binding with the given name.
    /// Fall back to the global scope if the binding is not found.
    #[must_use]
    pub fn get_identifier_reference(&self, name: JsString) -> IdentifierReference {
        if let Some(reference) = self.get_binding_reference(&name) {
            reference
        } else if let Some(outer) = &self.inner.outer {
            outer.get_identifier_reference(name)
        } else {
            IdentifierReference::new(BindingLocator::global(name), false, true)
        }
    }

    /// Returns the number of bindings in this scope.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn num_bindings(&self) -> u32 {
        self.inner.bindings.borrow().len() as u32
    }

    /// Returns the number of bindings in this scope that are not local.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn num_bindings_non_local(&self) -> u32 {
        self.inner
            .bindings
            .borrow()
            .iter()
            .filter(|binding| binding.escapes())
            .count() as u32
    }

    /// Adjust the binding indices to exclude local bindings.
    pub(crate) fn reorder_binding_indices(&self) {
        let mut bindings = self.inner.bindings.borrow_mut();
        let mut index = 0;
        for binding in bindings.iter_mut() {
            if !binding.escapes() {
                binding.index = 0;
                continue;
            }
            binding.index = index;
            index += 1;
        }
    }

    /// Returns the index of this scope.
    #[must_use]
    pub fn scope_index(&self) -> u32 {
        self.inner.index.get()
    }

    /// Set the index of this scope.
    pub(crate) fn set_index(&self, index: u32) {
        self.inner.index.set(index);
    }

    /// Check if the scope is a function scope.
    #[must_use]
    pub fn is_function(&self) -> bool {
        self.inner.function
    }

    /// Check if the scope is a global scope.
    #[must_use]
    pub fn is_global(&self) -> bool {
        self.inner.outer.is_none()
    }

    /// Check if a binding with the given name is mutable.
    ///
    /// Returns `Some(true)` for mutable bindings (`let`, `var`),
    /// `Some(false)` for immutable bindings (`const`),
    /// or `None` if the binding is not found in this or any outer scope.
    #[must_use]
    pub fn is_binding_mutable(&self, name: &JsString) -> Option<bool> {
        if let Some(mutable) = self.with_binding(name, Binding::is_mutable) {
            Some(mutable)
        } else if let Some(outer) = &self.inner.outer {
            outer.is_binding_mutable(name)
        } else {
            None
        }
    }

    /// Get the locator for a binding name among the bindings created after the first `known`
    /// ones, and whether that binding is mutable.
    #[must_use]
    pub fn get_binding_created_after(
        &self,
        name: &JsString,
        known: u32,
    ) -> Option<(BindingLocator, bool)> {
        let position = self.position(name).filter(|&p| p >= known as usize)?;
        let bindings = self.inner.bindings.borrow();
        let binding = &bindings[position];
        {
            let locator = BindingLocator::declarative(
                name.clone(),
                self.inner.index.get(),
                binding.index,
                self.inner.unique_id,
            );
            Some((locator, binding.is_mutable()))
        }
    }

    /// Get the locator for a binding name.
    #[must_use]
    pub fn get_binding(&self, name: &JsString) -> Option<BindingLocator> {
        self.with_binding(name, |binding| {
            BindingLocator::declarative(
                name.clone(),
                self.inner.index.get(),
                binding.index,
                self.inner.unique_id,
            )
        })
    }

    /// Get the locator for a binding name.
    #[must_use]
    pub fn get_binding_reference(&self, name: &JsString) -> Option<IdentifierReference> {
        self.with_binding(name, |binding| {
            IdentifierReference::new(
                BindingLocator::declarative(
                    name.clone(),
                    self.inner.index.get(),
                    binding.index,
                    self.inner.unique_id,
                ),
                binding.is_lex(),
                binding.escapes(),
            )
        })
    }

    /// Simulate a binding access.
    ///
    /// - If the binding access crosses a function border, the binding is marked as escaping.
    /// - If the binding access is in an eval or with scope, the binding is marked as escaping.
    pub fn access_binding(&self, name: &JsString, eval_or_with: bool) {
        let mut crossed_function_border = false;
        let mut current = self;
        loop {
            if let Some(position) = current.position(name) {
                let binding = &mut current.inner.bindings.borrow_mut()[position];
                binding.flags.insert(BindingFlags::ACCESSED);
                if crossed_function_border || eval_or_with {
                    binding.flags.insert(BindingFlags::ESCAPES);
                }
                return;
            }
            if let Some(outer) = &current.inner.outer {
                if current.inner.function {
                    crossed_function_border = true;
                }
                current = outer;
            } else {
                return;
            }
        }
    }

    /// Escape enclosing function environment's `this`.
    pub fn escape_this_in_enclosing_function_scope(&self) {
        let mut current = self;
        let mut crossed_function_border = false;

        loop {
            if crossed_function_border && current.is_function() && !current.inner.lexical_this.get()
            {
                current.inner.this_escaped.set(true);
                return;
            }
            if let Some(outer) = &current.inner.outer {
                if current.is_function() {
                    crossed_function_border = true;
                }
                current = outer;
            } else {
                return;
            }
        }
    }

    /// Creates a mutable binding.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn create_mutable_binding(&self, name: JsString, function_scope: bool) -> BindingLocator {
        if let Some(locator) = self.get_binding(&name) {
            return locator;
        }
        let binding_index = self.num_bindings();
        let mut flags = BindingFlags::MUTABLE;
        flags.set(BindingFlags::LEX, !function_scope);
        flags.set(BindingFlags::ESCAPES, self.is_global());
        self.push_binding(Binding {
            name: name.clone(),
            index: binding_index,
            flags,
        });
        BindingLocator::declarative(
            name,
            self.inner.index.get(),
            binding_index,
            self.inner.unique_id,
        )
    }

    /// Creates a mutable binding that can be deleted, as the declarations of a direct `eval`
    /// create in the variable environment that encloses it (`CreateMutableBinding(N, true)`).
    ///
    /// The binding is added to a scope that every run of the enclosing code shares, so the
    /// environments created for it later start without the binding, until an `eval` declares
    /// it again.
    pub(crate) fn create_deletable_binding(&self, name: JsString) -> BindingLocator {
        let created = !self.has_binding(&name);
        let locator = self.create_mutable_binding(name, true);
        if created && let Some(binding) = self.inner.bindings.borrow_mut().last_mut() {
            binding.flags.insert(BindingFlags::DELETABLE);
            self.inner.has_deletable_bindings.set(true);
        }
        locator
    }

    /// Returns `true` if a direct `eval` created the binding `name` of this scope.
    #[must_use]
    pub(crate) fn is_deletable_binding(&self, name: &JsString) -> bool {
        self.with_binding(name, Binding::is_deletable)
            .unwrap_or(false)
    }

    /// Calls `f` with the index of each binding of this scope that a direct `eval` created.
    pub fn for_each_deletable_binding(&self, mut f: impl FnMut(u32)) {
        if !self.inner.has_deletable_bindings.get() {
            return;
        }
        for binding in self.inner.bindings.borrow().iter() {
            if binding.is_deletable() {
                f(binding.index);
            }
        }
    }

    /// Crate an immutable binding.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn create_immutable_binding(&self, name: JsString, strict: bool) {
        if self.has_binding(&name) {
            return;
        }
        let binding_index = self.num_bindings();
        let mut flags = BindingFlags::LEX;
        flags.set(BindingFlags::STRICT, strict);
        flags.set(BindingFlags::ESCAPES, self.is_global());
        self.push_binding(Binding {
            name,
            index: binding_index,
            flags,
        });
    }

    /// Return the binding locator for a mutable binding.
    ///
    /// # Errors
    /// Returns an error if the binding is not mutable or does not exist.
    pub fn set_mutable_binding(
        &self,
        name: JsString,
    ) -> Result<IdentifierReference, BindingLocatorError> {
        Ok(match self.with_binding(&name, Binding::clone) {
            Some(binding) if binding.is_mutable() => IdentifierReference::new(
                BindingLocator::declarative(
                    name,
                    self.inner.index.get(),
                    binding.index,
                    self.inner.unique_id,
                ),
                binding.is_lex(),
                binding.escapes(),
            ),
            Some(binding) if binding.is_strict() => {
                return Err(BindingLocatorError::MutateImmutable);
            }
            Some(_) => return Err(BindingLocatorError::Silent),
            None => self.inner.outer.as_ref().map_or_else(
                || {
                    Ok(IdentifierReference::new(
                        BindingLocator::global(name.clone()),
                        false,
                        true,
                    ))
                },
                |outer| outer.set_mutable_binding(name.clone()),
            )?,
        })
    }

    #[cfg(feature = "annex-b")]
    /// Return the binding locator for a set operation on an existing var binding.
    ///
    /// # Errors
    /// Returns an error if the binding is not mutable or does not exist.
    pub fn set_mutable_binding_var(
        &self,
        name: JsString,
    ) -> Result<IdentifierReference, BindingLocatorError> {
        if !self.is_function() {
            return self.inner.outer.as_ref().map_or_else(
                || {
                    Ok(IdentifierReference::new(
                        BindingLocator::global(name.clone()),
                        false,
                        true,
                    ))
                },
                |outer| outer.set_mutable_binding_var(name.clone()),
            );
        }

        Ok(match self.with_binding(&name, Binding::clone) {
            Some(binding) if binding.is_mutable() => IdentifierReference::new(
                BindingLocator::declarative(
                    name,
                    self.inner.index.get(),
                    binding.index,
                    self.inner.unique_id,
                ),
                binding.is_lex(),
                binding.escapes(),
            ),
            Some(binding) if binding.is_strict() => {
                return Err(BindingLocatorError::MutateImmutable);
            }
            Some(_) => return Err(BindingLocatorError::Silent),
            None => self.inner.outer.as_ref().map_or_else(
                || {
                    Ok(IdentifierReference::new(
                        BindingLocator::global(name.clone()),
                        false,
                        true,
                    ))
                },
                |outer| outer.set_mutable_binding_var(name.clone()),
            )?,
        })
    }

    /// Gets the outer scope of this scope.
    #[must_use]
    pub fn outer(&self) -> Option<&Self> {
        self.inner.outer.as_ref()
    }

    /// Returns the unique ID of this scope.
    #[must_use]
    pub fn unique_id(&self) -> u32 {
        self.inner.unique_id
    }
}

/// Additional state that all Scopes of a single AST share for bookkeeping.
#[derive(Debug, PartialEq, Default)]
struct ScopeContext {
    /// A counter for unique IDs for Scopes generated as part of this scope tree.
    ///
    /// The value is the highest unique ID assigned so far (initialized with 0
    /// which is also the unique ID of the global scope).
    ///
    /// Only used in the root scope.
    unique_id_ctr: Cell<u32>,
}

impl ScopeContext {
    /// Returns the next unique ID for a scope in this scope tree.
    fn next_unique_id(&self) -> u32 {
        let id = self.unique_id_ctr.get() + 1;
        self.unique_id_ctr.set(id);
        id
    }
}

/// A reference to an identifier in a scope.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdentifierReference {
    locator: BindingLocator,
    lexical: bool,
    escapes: bool,
}

impl IdentifierReference {
    /// Create a new identifier reference.
    pub(crate) fn new(locator: BindingLocator, lexical: bool, escapes: bool) -> Self {
        Self {
            locator,
            lexical,
            escapes,
        }
    }

    /// Get the binding locator for this identifier reference.
    #[must_use]
    pub fn locator(&self) -> BindingLocator {
        self.locator.clone()
    }

    /// Returns if the binding can be function local.
    #[must_use]
    pub fn local(&self) -> bool {
        self.locator.scope > 0 && !self.escapes
    }

    /// Returns if the binding is on the global object.
    #[must_use]
    pub fn is_global_object(&self) -> bool {
        self.locator.scope == 0
    }

    /// Check if this identifier reference is lexical.
    #[must_use]
    pub fn is_lexical(&self) -> bool {
        self.lexical
    }
}

/// A binding locator contains all information about a binding that is needed to resolve it at runtime.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BindingLocator {
    /// Name of the binding.
    name: JsString,

    /// Scope of the binding.
    /// - 0: Global object
    /// - 1: Global declarative scope
    /// - n: Stack scope at index n - 2
    scope: u32,

    /// Index of the binding in the scope.
    binding_index: u32,

    unique_scope_id: u32,
}

impl BindingLocator {
    /// Creates a new declarative binding locator that has knows indices.
    pub(crate) const fn declarative(
        name: JsString,
        scope_index: u32,
        binding_index: u32,
        unique_scope_id: u32,
    ) -> Self {
        Self {
            name,
            scope: scope_index + 1,
            binding_index,
            unique_scope_id,
        }
    }

    /// Creates a binding locator that indicates that the binding is on the global object.
    pub(super) const fn global(name: JsString) -> Self {
        Self {
            name,
            scope: 0,
            binding_index: 0,
            unique_scope_id: 0,
        }
    }

    /// Returns the name of the binding.
    #[must_use]
    pub const fn name(&self) -> &JsString {
        &self.name
    }

    /// Returns if the binding is located on the global object.
    #[must_use]
    pub const fn is_global(&self) -> bool {
        self.scope == 0
    }

    /// Returns the scope of the binding.
    #[must_use]
    pub fn scope(&self) -> BindingLocatorScope {
        match self.scope {
            0 => BindingLocatorScope::GlobalObject,
            1 => BindingLocatorScope::GlobalDeclarative,
            n => BindingLocatorScope::Stack(n - 2),
        }
    }

    /// Sets the scope of the binding.
    pub fn set_scope(&mut self, scope: BindingLocatorScope) {
        self.scope = match scope {
            BindingLocatorScope::GlobalObject => 0,
            BindingLocatorScope::GlobalDeclarative => 1,
            BindingLocatorScope::Stack(index) => index + 2,
        };
    }

    /// Returns the binding index of the binding.
    #[must_use]
    pub const fn binding_index(&self) -> u32 {
        self.binding_index
    }

    /// Sets the binding index of the binding.
    pub fn set_binding_index(&mut self, index: u32) {
        self.binding_index = index;
    }

    /// Returns the unique scope ID of the binding.
    #[must_use]
    pub fn unique_scope_id(&self) -> u32 {
        self.unique_scope_id
    }
}

/// Action that is returned when a fallible binding operation.
#[derive(Copy, Clone, Debug)]
pub enum BindingLocatorError {
    /// Trying to mutate immutable binding,
    MutateImmutable,

    /// Indicates that any action is silently ignored.
    Silent,
}

/// The scope in which a binding is located.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingLocatorScope {
    /// The binding is located on the global object.
    GlobalObject,

    /// The binding is located in the global declarative scope.
    GlobalDeclarative,

    /// The binding is located in the scope stack at the given index.
    Stack(u32),
}

/// A collection of function scopes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FunctionScopes {
    pub(crate) function_scope: Scope,
    pub(crate) parameters_eval_scope: Option<Scope>,
    pub(crate) parameters_scope: Option<Scope>,
    pub(crate) lexical_scope: Option<Scope>,
    pub(crate) mapped_arguments_object: bool,
    pub(crate) requires_function_scope: bool,
}

impl FunctionScopes {
    /// Returns the function scope for this function.
    #[must_use]
    pub fn function_scope(&self) -> &Scope {
        &self.function_scope
    }

    /// Returns if the arguments object is accessed in this function.
    #[must_use]
    pub fn arguments_object_accessed(&self) -> bool {
        if self
            .function_scope
            .inner
            .bindings
            .borrow()
            .first()
            .as_ref()
            .is_some_and(|b| b.name == "arguments" && b.is_accessed())
        {
            return true;
        }

        if let Some(scope) = &self.parameters_eval_scope
            && scope
                .inner
                .bindings
                .borrow()
                .first()
                .as_ref()
                .is_some_and(|b| b.name == "arguments" && b.is_accessed())
        {
            return true;
        }

        false
    }

    /// Check if the creation of the function scope is required.
    #[must_use]
    pub fn requires_function_scope(&self) -> bool {
        self.requires_function_scope
    }

    /// Returns the parameters eval scope for this function.
    #[must_use]
    pub fn parameters_eval_scope(&self) -> Option<&Scope> {
        self.parameters_eval_scope.as_ref()
    }

    /// Returns the parameters scope for this function.
    #[must_use]
    pub fn parameters_scope(&self) -> Option<&Scope> {
        self.parameters_scope.as_ref()
    }

    /// Returns the lexical scope for this function.
    #[must_use]
    pub fn lexical_scope(&self) -> Option<&Scope> {
        self.lexical_scope.as_ref()
    }

    /// Returns the effective parameter scope for this function.
    #[must_use]
    pub fn parameter_scope(&self) -> Scope {
        if let Some(parameters_eval_scope) = &self.parameters_eval_scope {
            return parameters_eval_scope.clone();
        }
        self.function_scope.clone()
    }

    /// Returns the effective body scope for this function.
    pub(crate) fn body_scope(&self) -> Scope {
        if let Some(lexical_scope) = &self.lexical_scope {
            return lexical_scope.clone();
        }
        if let Some(parameters_scope) = &self.parameters_scope {
            return parameters_scope.clone();
        }
        if let Some(parameters_eval_scope) = &self.parameters_eval_scope {
            return parameters_eval_scope.clone();
        }
        self.function_scope.clone()
    }

    /// Marks the `arguments` binding of the function, if it has one, as accessed: a direct
    /// `eval` in the function can read it.
    pub(crate) fn access_arguments_binding(&self) {
        for scope in std::iter::once(&self.function_scope).chain(&self.parameters_eval_scope) {
            if let Some(binding) = scope.inner.bindings.borrow_mut().first_mut()
                && binding.name == "arguments"
            {
                binding.flags.insert(BindingFlags::ACCESSED);
            }
        }
    }

    /// Marks all bindings in all scopes as escaping.
    pub(crate) fn escape_all_bindings(&self) {
        self.function_scope.escape_all_bindings();
        if let Some(parameters_eval_scope) = &self.parameters_eval_scope {
            parameters_eval_scope.escape_all_bindings();
        }
        if let Some(parameters_scope) = &self.parameters_scope {
            parameters_scope.escape_all_bindings();
        }
        if let Some(lexical_scope) = &self.lexical_scope {
            lexical_scope.escape_all_bindings();
        }
    }

    pub(crate) fn reorder_binding_indices(&self) {
        self.function_scope.reorder_binding_indices();
        if let Some(parameters_eval_scope) = &self.parameters_eval_scope {
            parameters_eval_scope.reorder_binding_indices();
        }
        if let Some(parameters_scope) = &self.parameters_scope {
            parameters_scope.reorder_binding_indices();
        }
        if let Some(lexical_scope) = &self.lexical_scope {
            lexical_scope.reorder_binding_indices();
        }
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for FunctionScopes {
    fn arbitrary(_u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            function_scope: Scope::new_global(),
            parameters_eval_scope: None,
            parameters_scope: None,
            lexical_scope: None,
            mapped_arguments_object: false,
            requires_function_scope: false,
        })
    }
}
