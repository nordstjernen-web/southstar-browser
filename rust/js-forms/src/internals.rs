//! Southstar — ElementInternals from attachInternals: the form-associated custom element surface, its ARIA reflection and its CustomStateSet.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::props::arg;
use southstar_js_engine::props::global_get;
use std::cell::RefCell;
use std::rc::Rc;

use southstar_dom::internals::{self, Entry};
use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, WeakNode, ancestors_and_self, children};
use southstar_js_engine::{Attributes, BoundFn, NativeFn, Scope, Trace, Value};

use crate::ffi::{self, Page};
use crate::{Element, JsResult, MAX_DEPTH, form_owner};

const MEMBER: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};
const ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};
const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};
const INTERNALS_KEY: &str = "__nd_internals";
const INTERNALS_PROTO: &str = "__ndElementInternalsProto";
const STATES_PROTO: &str = "__ndCustomStateSetProto";
const LIVE_LABELS: i32 = 13;
const STATES_ITERATOR: &str = "(function(P){ Object.defineProperty(P, Symbol.iterator, {value: P.values, writable: true, configurable: true}); })";

const FLAG_NAMES: [&str; 10] = [
    "badInput",
    "customError",
    "patternMismatch",
    "rangeOverflow",
    "rangeUnderflow",
    "stepMismatch",
    "tooLong",
    "tooShort",
    "typeMismatch",
    "valueMissing",
];

const ARIA: [&str; 44] = [
    "role",
    "ariaAtomic",
    "ariaAutoComplete",
    "ariaBrailleLabel",
    "ariaBrailleRoleDescription",
    "ariaBusy",
    "ariaChecked",
    "ariaColCount",
    "ariaColIndex",
    "ariaColIndexText",
    "ariaColSpan",
    "ariaCurrent",
    "ariaDescription",
    "ariaDisabled",
    "ariaExpanded",
    "ariaHasPopup",
    "ariaHidden",
    "ariaInvalid",
    "ariaKeyShortcuts",
    "ariaLabel",
    "ariaLevel",
    "ariaLive",
    "ariaModal",
    "ariaMultiLine",
    "ariaMultiSelectable",
    "ariaOrientation",
    "ariaPlaceholder",
    "ariaPosInSet",
    "ariaPressed",
    "ariaReadOnly",
    "ariaRelevant",
    "ariaRequired",
    "ariaRoleDescription",
    "ariaRowCount",
    "ariaRowIndex",
    "ariaRowIndexText",
    "ariaRowSpan",
    "ariaSelected",
    "ariaSetSize",
    "ariaSort",
    "ariaValueMax",
    "ariaValueMin",
    "ariaValueNow",
    "ariaValueText",
];

const METHODS: [(&str, u32, NativeFn); 4] = [
    ("setFormValue", 1, set_form_value),
    ("setValidity", 0, set_validity),
    ("checkValidity", 0, check_validity),
    ("reportValidity", 0, check_validity),
];

const GETTERS: [(&str, NativeFn); 7] = [
    ("shadowRoot", shadow_root),
    ("form", form),
    ("willValidate", will_validate),
    ("validity", validity),
    ("validationMessage", validation_message),
    ("labels", labels),
    ("states", states),
];

const STATE_METHODS: [(&str, u32, NativeFn); 7] = [
    ("add", 1, state_add),
    ("delete", 1, state_delete),
    ("has", 1, state_has),
    ("clear", 0, state_clear),
    ("forEach", 1, state_for_each),
    ("entries", 0, state_entries),
    ("values", 0, state_values),
];

struct Slots {
    element: Value,
    states: Value,
    value: Value,
    state: Value,
    anchor: Value,
    validity: Value,
    message: String,
    flags: [bool; 10],
    aria: Vec<Value>,
}

struct Internals {
    target: WeakNode,
    slots: RefCell<Slots>,
}

#[derive(Clone)]
struct Handle(Rc<Internals>);

impl Trace for Handle {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        if let Ok(slots) = self.0.slots.try_borrow() {
            for value in [
                &slots.element,
                &slots.states,
                &slots.value,
                &slots.state,
                &slots.anchor,
                &slots.validity,
            ] {
                visit(value);
            }
            slots.aria.iter().for_each(&mut *visit);
        }
    }
}

#[derive(Clone)]
struct States {
    target: WeakNode,
    set: Value,
}

impl Trace for States {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.set);
    }
}

fn not_supported(scope: &mut Scope<'_>, message: &str) -> Value {
    scope.dom_exception("NotSupportedError", message)
}

fn handle(scope: &mut Scope<'_>, this: &Value) -> JsResult<Rc<Internals>> {
    match this.with_host::<Handle, _>(|handle| handle.0.clone()) {
        Some(internals) => Ok(internals),
        None => Err(scope.type_error("Illegal invocation")),
    }
}

fn target(scope: &mut Scope<'_>, this: &Value) -> JsResult<(Rc<Internals>, Element)> {
    let internals = handle(scope, this)?;
    match internals.target.get() {
        Some(node) => Ok((internals, node)),
        None => Err(not_supported(
            scope,
            "ElementInternals: the target element is gone",
        )),
    }
}

fn face(scope: &mut Scope<'_>, this: &Value) -> JsResult<(Rc<Internals>, Element)> {
    let (internals, node) = target(scope, this)?;
    if internals::is_form_associated(node) {
        Ok((internals, node))
    } else {
        Err(not_supported(
            scope,
            "ElementInternals: the target element is not a form-associated custom element",
        ))
    }
}

fn is_instance(scope: &mut Scope<'_>, value: &Value, interface: &str) -> bool {
    let constructor = global_get(scope, interface);
    value.is_object()
        && scope.is_constructor(&constructor)
        && scope.instance_of(value, &constructor)
}

fn text_bytes(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    crate::c_bytes(scope, value).unwrap_or_default()
}

fn entry_bytes(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    if value.is_string() {
        return text_bytes(scope, value);
    }
    let name = scope
        .get(value, "name")
        .unwrap_or_else(|_| Value::undefined());
    text_bytes(scope, &name)
}

fn clone_entries(scope: &mut Scope<'_>, form_data: &Value) -> JsResult<(Value, Vec<Entry>)> {
    let list = scope.get(form_data, "_entries")?;
    let copy = scope.new_array();
    let mut entries = Vec::new();
    if !list.is_array() {
        return Ok((copy, entries));
    }
    for i in 0..crate::length(scope, &list) {
        let pair = crate::index(scope, &list, i);
        let name = crate::index(scope, &pair, 0);
        let value = crate::index(scope, &pair, 1);
        let cloned = scope.new_array();
        scope.set_index(&cloned, 0, name.clone())?;
        scope.set_index(&cloned, 1, value.clone())?;
        scope.set_index(&copy, i, cloned)?;
        entries.push(Entry {
            name: Some(text_bytes(scope, &name)),
            value: entry_bytes(scope, &value),
        });
    }
    Ok((copy, entries))
}

fn submission_value(scope: &mut Scope<'_>, value: &Value) -> JsResult<(Value, Option<Vec<Entry>>)> {
    if value.is_null() || value.is_undefined() {
        return Ok((Value::null(), None));
    }
    if ffi::is_form_data(scope, value) {
        let (copy, entries) = clone_entries(scope, value)?;
        return Ok((copy, Some(entries)));
    }
    let single = if is_instance(scope, value, "File") {
        value.clone()
    } else {
        scope.to_string_value(value)?
    };
    let entry = Entry {
        name: None,
        value: entry_bytes(scope, &single),
    };
    Ok((single, Some(vec![entry])))
}

fn set_form_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (internals, node) = face(scope, this)?;
    if args.is_empty() {
        return Err(scope.type_error(
            "Failed to execute 'setFormValue' on 'ElementInternals': 1 argument required, but only 0 present.",
        ));
    }
    let (value, entries) = submission_value(scope, &args[0])?;
    let state = match args.get(1) {
        Some(state) => submission_value(scope, state)?.0,
        None => value.clone(),
    };
    {
        let mut slots = internals.slots.borrow_mut();
        slots.value = value;
        slots.state = state;
    }
    internals::set_submission(node, entries);
    Ok(Value::undefined())
}

fn form(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (_, node) = face(scope, this)?;
    Ok(form_owner(node).map_or_else(Value::null, |form| ffi::wrap(scope, Some(form))))
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn read_flags(scope: &mut Scope<'_>, flags: &Value) -> JsResult<[bool; 10]> {
    let mut out = [false; 10];
    if flags.is_null() || flags.is_undefined() {
        return Ok(out);
    }
    if !flags.is_object() {
        return Err(scope.type_error(
            "Failed to execute 'setValidity' on 'ElementInternals': The provided value is not of type 'ValidityStateFlags'.",
        ));
    }
    for (slot, name) in out.iter_mut().zip(FLAG_NAMES) {
        let value = scope.get(flags, name)?;
        *slot = scope.to_bool(&value);
    }
    Ok(out)
}

fn is_html_element(node: Element) -> bool {
    node.is_element() && node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
}

fn set_validity(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (internals, node) = face(scope, this)?;
    let flags = read_flags(scope, &arg(args, 0))?;
    let message = match args.get(1).filter(|m| !m.is_undefined()) {
        Some(message) => Some(scope.to_string(message)?),
        None => None,
    };
    let anchor_arg = arg(args, 2);
    let anchor = if anchor_arg.is_undefined() {
        None
    } else {
        match ffi::element(&anchor_arg).filter(|a| is_html_element(*a)) {
            Some(anchor) => Some(anchor),
            None => {
                return Err(scope.type_error(
                    "Failed to execute 'setValidity' on 'ElementInternals': parameter 3 is not of type 'HTMLElement'.",
                ));
            }
        }
    };
    let invalid = flags.iter().any(|&flag| flag);
    if invalid && message.as_deref().is_none_or(str::is_empty) {
        return Err(scope.type_error(
            "Failed to execute 'setValidity' on 'ElementInternals': The second argument should not be empty if one or more flags in the first argument are true.",
        ));
    }
    let message = match message {
        Some(message) if invalid => normalize_newlines(&message),
        _ => String::new(),
    };
    {
        let mut slots = internals.slots.borrow_mut();
        slots.flags = flags;
        slots.message = message;
        slots.anchor = Value::null();
    }
    let state = internals.slots.borrow().validity.clone();
    if state.is_object() {
        let (fields, valid) = validity_fields(flags);
        crate::validity::update_validity_state(scope, &state, &fields, valid);
    }
    internals::set_invalid(node, invalid);
    southstar_css::mark_attr_dirty(node, None, None);
    Page::mark_mutated(Page::of(scope));
    if let Some(anchor) = anchor {
        if !ancestors_and_self(anchor).any(|a| a == node) {
            return Err(ffi::not_found(
                scope,
                "ElementInternals.setValidity: the anchor is not a shadow-including descendant of the element",
            ));
        }
        internals.slots.borrow_mut().anchor = anchor_arg;
    }
    Ok(Value::undefined())
}

fn will_validate(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (_, node) = face(scope, this)?;
    Ok(Value::boolean(internals::will_validate(node)))
}

fn validity_fields(flags: [bool; 10]) -> (Vec<(&'static str, bool)>, bool) {
    let fields = FLAG_NAMES.into_iter().zip(flags).collect();
    (fields, !flags.iter().any(|&flag| flag))
}

fn validity(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (internals, _) = face(scope, this)?;
    let (existing, flags) = {
        let slots = internals.slots.borrow();
        (slots.validity.clone(), slots.flags)
    };
    if existing.is_object() {
        return Ok(existing);
    }
    let (fields, valid) = validity_fields(flags);
    let state = crate::validity::validity_state(scope, &fields, valid);
    internals.slots.borrow_mut().validity = state.clone();
    Ok(state)
}

fn validation_message(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (internals, _) = face(scope, this)?;
    let message = internals.slots.borrow().message.clone();
    Ok(scope.string(&message))
}

fn check_validity(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (_, node) = face(scope, this)?;
    if !internals::fails_validation(node) {
        return Ok(Value::boolean(true));
    }
    if let Some(page) = Page::of(scope) {
        page.dispatch_event(node, c"invalid");
    }
    Ok(Value::boolean(false))
}

fn labels(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (internals, _) = face(scope, this)?;
    let element = internals.slots.borrow().element.clone();
    ffi::make_live(scope, &element, LIVE_LABELS, None)
}

fn shadow_root(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let (internals, _) = target(scope, this)?;
    let element = internals.slots.borrow().element.clone();
    Ok(ffi::internals_shadow_root(scope, &element))
}

fn states(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let internals = handle(scope, this)?;
    let states = internals.slots.borrow().states.clone();
    Ok(states)
}

fn aria_index(scope: &mut Scope<'_>, data: &[Value]) -> usize {
    let index = data.first().cloned().unwrap_or_else(Value::undefined);
    crate::uint32(scope, &index) as usize
}

fn aria_get(scope: &mut Scope<'_>, this: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let internals = handle(scope, this)?;
    let index = aria_index(scope, data);
    let value = internals.slots.borrow().aria.get(index).cloned();
    Ok(value.unwrap_or_else(Value::null))
}

fn aria_set(scope: &mut Scope<'_>, this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let internals = handle(scope, this)?;
    let index = aria_index(scope, data);
    let value = arg(args, 0);
    let value = if value.is_null() {
        value
    } else {
        scope.to_string_value(&value)?
    };
    if let Some(slot) = internals.slots.borrow_mut().aria.get_mut(index) {
        *slot = value;
    }
    Ok(Value::undefined())
}

fn state_set(scope: &mut Scope<'_>, this: &Value) -> JsResult<States> {
    match this.with_host::<States, _>(States::clone) {
        Some(states) => Ok(states),
        None => Err(scope.type_error("Illegal invocation")),
    }
}

fn call_set(scope: &mut Scope<'_>, states: &States, method: &str, args: &[Value]) -> JsResult {
    let function = scope.get(&states.set, method)?;
    scope.call(&function, &states.set, args)
}

fn state_name(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<(Value, String)> {
    let value = scope.to_string_value(&arg(args, 0))?;
    let text = scope.to_string(&value)?;
    Ok((value, text))
}

fn restyle_states(scope: &mut Scope<'_>, node: Element) {
    southstar_css::mark_states_dirty(node);
    Page::mark_mutated(Page::of(scope));
}

fn state_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    let (value, text) = state_name(scope, args)?;
    call_set(scope, &states, "add", &[value])?;
    if let Some(node) = states.target.get()
        && internals::add_state(node, &text)
    {
        restyle_states(scope, node);
    }
    Ok(this.clone())
}

fn state_delete(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    let (value, text) = state_name(scope, args)?;
    let removed = call_set(scope, &states, "delete", &[value])?;
    if let Some(node) = states.target.get()
        && internals::delete_state(node, &text)
    {
        restyle_states(scope, node);
    }
    Ok(removed)
}

fn state_has(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    let (value, _) = state_name(scope, args)?;
    call_set(scope, &states, "has", &[value])
}

fn state_clear(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    call_set(scope, &states, "clear", &[])?;
    if let Some(node) = states.target.get()
        && internals::clear_states(node)
    {
        restyle_states(scope, node);
    }
    Ok(Value::undefined())
}

fn state_size(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    scope.get(&states.set, "size")
}

fn state_for_each(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    let callback = arg(args, 0);
    if !scope.is_function(&callback) {
        return Err(scope.type_error(
            "Failed to execute 'forEach' on 'CustomStateSet': The callback provided as parameter 1 is not a function.",
        ));
    }
    let this_arg = arg(args, 1);
    let snapshot = states
        .target
        .get()
        .map(internals::states)
        .unwrap_or_default();
    for state in snapshot {
        let value = scope.string(&state);
        scope.call(&callback, &this_arg, &[value.clone(), value, this.clone()])?;
    }
    Ok(Value::undefined())
}

fn state_entries(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    call_set(scope, &states, "entries", &[])
}

fn state_values(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let states = state_set(scope, this)?;
    call_set(scope, &states, "values", &[])
}

fn illegal_constructor(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Err(scope.type_error("Illegal constructor"))
}

fn define_method(scope: &mut Scope<'_>, proto: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    let _ = scope.define(proto, name, function, MEMBER);
}

fn define_getter(scope: &mut Scope<'_>, proto: &Value, name: &str, f: NativeFn) {
    let getter = scope.function(&format!("get {name}"), 0, f);
    let _ = scope.define_accessor(proto, name, Some(&getter), None, ACCESSOR);
}

fn define_bound_accessor(
    scope: &mut Scope<'_>,
    proto: &Value,
    name: &str,
    index: usize,
    (get, set): (BoundFn, BoundFn),
) {
    let data = [Value::int(index as i32)];
    let getter = scope.bound_function(&format!("get {name}"), 0, get, &data);
    let setter = scope.bound_function(&format!("set {name}"), 1, set, &data);
    let _ = scope.define_accessor(proto, name, Some(&getter), Some(&setter), ACCESSOR);
}

fn interface(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let constructor = scope.constructor_or_function(name, 0, illegal_constructor);
    let proto = scope.new_object();
    let _ = scope.define(
        &proto,
        "constructor",
        constructor.clone(),
        Attributes::METHOD,
    );
    let _ = scope.define_to_string_tag(&proto, name);
    let _ = scope.define(&constructor, "prototype", proto.clone(), HIDDEN);
    let _ = scope.define(global, name, constructor, Attributes::METHOD);
    proto
}

pub fn install(scope: &mut Scope<'_>, global: &Value) {
    let proto = interface(scope, global, "ElementInternals");
    for (name, arity, f) in METHODS {
        define_method(scope, &proto, name, arity, f);
    }
    for (name, f) in GETTERS {
        define_getter(scope, &proto, name, f);
    }
    for (index, name) in ARIA.into_iter().enumerate() {
        define_bound_accessor(scope, &proto, name, index, (aria_get, aria_set));
    }
    let _ = scope.define(global, INTERNALS_PROTO, proto, HIDDEN);

    let states_proto = interface(scope, global, "CustomStateSet");
    for (name, arity, f) in STATE_METHODS {
        define_method(scope, &states_proto, name, arity, f);
    }
    let values = scope.get(&states_proto, "values");
    if let Ok(values) = values {
        let _ = scope.define(&states_proto, "keys", values, MEMBER);
    }
    define_getter(scope, &states_proto, "size", state_size);
    if let Ok(helper) = scope.eval_native_script(STATES_ITERATOR, "<customstateset>") {
        let _ = scope.call(
            &helper,
            &Value::undefined(),
            std::slice::from_ref(&states_proto),
        );
    }
    let _ = scope.define(global, STATES_PROTO, states_proto, HIDDEN);
}

fn new_states(scope: &mut Scope<'_>, node: Element) -> JsResult {
    let set_constructor = global_get(scope, "Set");
    let set = scope.construct(&set_constructor, &[])?;
    let proto = global_get(scope, STATES_PROTO);
    let states = States {
        target: node.downgrade(),
        set,
    };
    Ok(scope.new_traced_host_object(proto.is_object().then_some(&proto), states))
}

pub fn attach_internals(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::element(this) else {
        return Err(scope.type_error("Illegal invocation"));
    };
    let class = scope.get(this, "__nd_ce_class")?;
    let autonomous = node.element_name().is_some_and(|name| name.contains(&b'-'));
    if !class.is_object() || !autonomous {
        return Err(not_supported(
            scope,
            "attachInternals: the element is not an autonomous custom element",
        ));
    }
    let extends = scope.get(&class, "__nd_ce_extends")?;
    let disabled = scope.get(&class, "__nd_ce_disable_internals")?;
    if extends.is_string() || scope.to_bool(&disabled) {
        return Err(not_supported(
            scope,
            "attachInternals: internals are disabled for this custom element",
        ));
    }
    let existing = scope.get(this, INTERNALS_KEY)?;
    if !existing.is_undefined() {
        return Err(not_supported(
            scope,
            "attachInternals: ElementInternals for this element were already attached",
        ));
    }
    let states = new_states(scope, node)?;
    let internals = Internals {
        target: node.downgrade(),
        slots: RefCell::new(Slots {
            element: this.clone(),
            states,
            value: Value::null(),
            state: Value::null(),
            anchor: Value::null(),
            validity: Value::null(),
            message: String::new(),
            flags: [false; 10],
            aria: vec![Value::null(); ARIA.len()],
        }),
    };
    let proto = global_get(scope, INTERNALS_PROTO);
    let object = scope.new_traced_host_object(
        proto.is_object().then_some(&proto),
        Handle(Rc::new(internals)),
    );
    scope.define(this, INTERNALS_KEY, object.clone(), HIDDEN)?;
    Ok(object)
}

fn internals_of(scope: &mut Scope<'_>, node: Element) -> Option<Rc<Internals>> {
    let wrapper = ffi::node_wrapper(scope, node);
    if !wrapper.is_object() {
        return None;
    }
    let object = scope.get(&wrapper, INTERNALS_KEY).ok()?;
    object.with_host::<Handle, _>(|handle| handle.0.clone())
}

pub(crate) fn append_submission(scope: &mut Scope<'_>, fd: &Value, node: Element) {
    let Some(internals) = internals_of(scope, node) else {
        return;
    };
    let value = internals.slots.borrow().value.clone();
    if value.is_null() {
        return;
    }
    if value.is_array() {
        for i in 0..crate::length(scope, &value) {
            let pair = crate::index(scope, &value, i);
            let name = crate::index(scope, &pair, 0);
            let item = crate::index(scope, &pair, 1);
            crate::form_data::push_entry(scope, fd, name, item);
        }
        return;
    }
    let Some(name) = crate::non_empty(node.attr(c"name")) else {
        return;
    };
    let name = scope.string_from_bytes(name.to_bytes());
    crate::form_data::push_entry(scope, fd, name, value);
}

fn call_reaction(scope: &mut Scope<'_>, node: Element, callback: &str) {
    let wrapper = ffi::node_wrapper(scope, node);
    if !wrapper.is_object() {
        return;
    }
    let Ok(class) = scope.get(&wrapper, "__nd_ce_class") else {
        return;
    };
    let proto = if class.is_object() {
        scope
            .get(&class, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    if !proto.is_object() {
        return;
    }
    let function = scope
        .get(&proto, callback)
        .unwrap_or_else(|_| Value::undefined());
    if scope.is_function(&function) {
        let _ = scope.call(&function, &wrapper, &[]);
    }
}

fn reset_walk(scope: &mut Scope<'_>, form: Element, scan: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(scan) {
        if internals::is_form_associated(child) && form_owner(child) == Some(form) {
            call_reaction(scope, child, "formResetCallback");
        }
        reset_walk(scope, form, child, depth + 1);
    }
}

pub(crate) fn reset_owned(scope: &mut Scope<'_>, form: Element, root: Element) {
    reset_walk(scope, form, root, 0);
}
