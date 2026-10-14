//! Southstar — setHTML and the HTML insertion methods appendHTML, prependHTML, beforeHTML, afterHTML and replaceWithHTML with their Unsafe variants, sanitizer and runScripts options.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SCRIPTING_DISABLED, Kind, Tree};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::sanitize::{self, Config};
use crate::{Element, JsResult, children, insert_after, is_fragment, prepend, scripting_enabled};

const HIERARCHY_REQUEST_ERR: i32 = 3;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Replace,
    Append,
    Prepend,
    Before,
    After,
    ReplaceWith,
}

impl Place {
    fn beside_self(self) -> bool {
        matches!(self, Place::Before | Place::After | Place::ReplaceWith)
    }
}

enum Context {
    Element(Element),
    Template,
}

struct Target {
    container: Element,
    context: Context,
}

enum Resolved {
    Insert(Target),
    Nothing,
    Refuse,
}

fn is_html_element(n: Element, name: &[u8]) -> bool {
    n.kind() == Kind::Element && n.element_name() == Some(name)
}

fn is_script(n: Element) -> bool {
    let Some(name) = n
        .element_name()
        .filter(|_| n.flags() & FLAG_FOREIGN_NS == 0)
    else {
        return false;
    };
    let local = name.rsplit(|&c| c == b':').next().unwrap_or(name);
    local == b"script"
}

fn is_template_content(n: Element) -> bool {
    is_fragment(n) && n.flags() & FLAG_TEMPLATE_CONTENT != 0
}

fn is_shadow_root(n: Element) -> bool {
    ffi::is_shadow_root(n)
}

fn receiver_target(this: Element, place: Place, safe: bool) -> Resolved {
    if is_shadow_root(this) {
        return match this.parent() {
            Some(host) => Resolved::Insert(Target {
                container: this,
                context: Context::Element(host),
            }),
            None => Resolved::Nothing,
        };
    }
    if this.kind() != Kind::Element {
        return Resolved::Nothing;
    }
    if place != Place::Replace && is_html_element(this, b"html") {
        return Resolved::Refuse;
    }
    if safe && is_script(this) {
        return Resolved::Nothing;
    }
    if is_html_element(this, b"template") {
        return Resolved::Insert(Target {
            container: southstar_dom::node::template_content(this),
            context: Context::Element(this),
        });
    }
    Resolved::Insert(Target {
        container: this,
        context: Context::Element(this),
    })
}

fn sibling_target(this: Element, safe: bool) -> Resolved {
    let Some(parent) = this.parent() else {
        return Resolved::Nothing;
    };
    if is_shadow_root(parent) {
        return match parent.parent() {
            Some(host) => Resolved::Insert(Target {
                container: parent,
                context: Context::Element(host),
            }),
            None => Resolved::Nothing,
        };
    }
    if is_template_content(parent) {
        return Resolved::Insert(Target {
            container: parent,
            context: Context::Template,
        });
    }
    if parent.kind() != Kind::Element
        || is_html_element(parent, b"html")
        || is_html_element(parent, b"template")
    {
        return Resolved::Refuse;
    }
    if safe && is_script(parent) {
        return Resolved::Nothing;
    }
    Resolved::Insert(Target {
        container: parent,
        context: Context::Element(parent),
    })
}

struct Options {
    config: Config,
    run_scripts: bool,
}

fn read_options(scope: &mut Scope<'_>, args: &[Value], safe: bool) -> JsResult<Options> {
    let mut options = Options {
        config: Config::allow_all(),
        run_scripts: false,
    };
    let Some(dict) = args.get(1).filter(|a| a.is_object()) else {
        return Ok(options);
    };
    let sanitizer = scope.get(dict, "sanitizer")?;
    options.config = sanitize::from_value(scope, &sanitizer)?;
    if !safe {
        let run = scope.get(dict, "runScripts")?;
        options.run_scripts = scope.to_bool(&run);
    }
    Ok(options)
}

fn parse(target: &Target, markup: &[u8], scripting: bool) -> Tree {
    match target.context {
        Context::Element(el) => ffi::parse_fragment_in_context(el, markup, scripting),
        Context::Template => ffi::parse_fragment_for_tag(c"template", markup, scripting),
    }
}

fn take_children(fragment: Element, run_scripts: bool) -> Vec<Element> {
    if !run_scripts {
        ffi::mark_scripts_already_started(fragment);
    }
    children(fragment)
}

fn record(js: Option<Js>, parent: Element, kid: Element) {
    if let Some(js) = js {
        ffi::record_child_change(
            js,
            parent,
            Some(kid),
            kid.prev_sibling(),
            kid.next_sibling(),
        );
    }
}

fn remove_recorded(js: Option<Js>, parent: Element, node: Element) {
    let previous = node.prev_sibling();
    let next = node.next_sibling();
    if let Some(js) = js {
        ffi::ce_disconnect_subtree(js, node);
    }
    node.detach();
    if let Some(js) = js {
        ffi::track_orphan(js, node);
        ffi::record_child_changes(js, parent, &[], &[node], previous, next);
    }
}

fn place_children(
    js: Option<Js>,
    this: Element,
    container: Element,
    kids: &[Element],
    place: Place,
) {
    match place {
        Place::Replace => {
            for child in children(container) {
                remove_recorded(js, container, child);
            }
            for &kid in kids {
                container.append(kid);
                record(js, container, kid);
            }
        }
        Place::Append => {
            for &kid in kids {
                container.append(kid);
                record(js, container, kid);
            }
        }
        Place::Prepend => {
            for &kid in kids.iter().rev() {
                prepend(container, kid);
                record(js, container, kid);
            }
        }
        Place::Before | Place::ReplaceWith => {
            for &kid in kids {
                kid.detach();
                ffi::insert_sibling_before(this, kid);
                record(js, container, kid);
            }
            if place == Place::ReplaceWith {
                remove_recorded(js, container, this);
            }
        }
        Place::After => {
            let mut reference = this;
            for &kid in kids {
                insert_after(reference, kid);
                record(js, container, kid);
                reference = kid;
            }
        }
    }
}

fn insert_html(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    place: Place,
    safe: bool,
) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let markup = match args.first() {
        Some(v) if !v.is_null() => scope.to_bytes(v)?,
        _ => Vec::new(),
    };
    let mut options = read_options(scope, args, safe)?;
    if safe {
        options.run_scripts = false;
    }
    let resolved = if place.beside_self() {
        sibling_target(node, safe)
    } else {
        receiver_target(node, place, safe)
    };
    let target = match resolved {
        Resolved::Insert(target) => target,
        Resolved::Nothing => return Ok(Value::undefined()),
        Resolved::Refuse => {
            return Err(ffi::dom_exception(
                scope,
                c"HierarchyRequestError",
                HIERARCHY_REQUEST_ERR,
                c"The node cannot receive parsed HTML in this position",
            ));
        }
    };
    let in_template = is_template_content(target.container)
        || ffi::in_template_content(target.container)
        || target.container.flags() & FLAG_SCRIPTING_DISABLED != 0;
    let run_scripts = options.run_scripts && !in_template;
    let fragment = parse(&target, &markup, scripting_enabled(node));
    ffi::convert_declarative_shadow(fragment.root());
    sanitize::sanitize(fragment.root(), &options.config, safe);
    let kids = take_children(fragment.root(), run_scripts);
    let js = ffi::js_of(scope);
    place_children(js, node, target.container, &kids, place);
    drop(fragment);
    if let Some(js) = js {
        ffi::mark_mutated(js);
        if !in_template {
            ffi::ce_upgrade_subtree_all(js, target.container);
            ffi::run_inserted_scripts(js, target.container);
        }
    }
    Ok(Value::undefined())
}

macro_rules! insertion_methods {
    ($($name:ident => $place:expr, $safe:expr);* $(;)?) => {
        $(
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                insert_html(scope, this, args, $place, $safe)
            }
        )*
    };
}

insertion_methods! {
    set_html => Place::Replace, true;
    append_html => Place::Append, true;
    append_html_unsafe => Place::Append, false;
    prepend_html => Place::Prepend, true;
    prepend_html_unsafe => Place::Prepend, false;
    before_html => Place::Before, true;
    before_html_unsafe => Place::Before, false;
    after_html => Place::After, true;
    after_html_unsafe => Place::After, false;
    replace_with_html => Place::ReplaceWith, true;
    replace_with_html_unsafe => Place::ReplaceWith, false;
}
