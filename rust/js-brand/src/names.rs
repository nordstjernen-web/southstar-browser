//! Southstar — gives the engine's native members their WebIDL names: "get x" and "set x" for accessors, the property name for methods and the global name for interface objects.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::Scope;

const MEMBER_NAMES_SRC: &str = concat!(
    "(function(G){",
    "  var toStr = Function.prototype.toString, done = new Set(), seen = new Set();",
    "  function engine(f){ try { return / \\{ \\[native code\\] \\}$/.test(toStr.call(f)); } catch (e) { return false; } }",
    "  function name(f, n){",
    "    if (typeof f !== 'function' || done.has(f) || f.name === n || !engine(f)) return;",
    "    done.add(f);",
    "    try { Object.defineProperty(f, 'name', { value: n, configurable: true }); } catch (e) {}",
    "  }",
    "  function alias(o, f){",
    "    return typeof f === 'function' && typeof f.name === 'string' && f.name !== '' && o[f.name] === f;",
    "  }",
    "  function fix(o){",
    "    if (!o || (typeof o !== 'object' && typeof o !== 'function') || seen.has(o)) return;",
    "    seen.add(o);",
    "    var keys; try { keys = Object.getOwnPropertyNames(o); } catch (e) { return; }",
    "    keys.forEach(function(k){",
    "      if (/^[A-Z]/.test(k) || k === 'constructor' || k === 'prototype' || k.slice(0, 2) === '__') return;",
    "      var d; try { d = Object.getOwnPropertyDescriptor(o, k); } catch (e) { return; }",
    "      if (!d) return;",
    "      if (d.get) name(d.get, 'get ' + k);",
    "      if (d.set) name(d.set, 'set ' + k);",
    "      if ('value' in d && !alias(o, d.value)) name(d.value, k);",
    "    });",
    "  }",
    "  var gkeys = Object.getOwnPropertyNames(G);",
    "  gkeys.forEach(function(k){",
    "    var v; try { v = G[k]; } catch (e) { return; }",
    "    if (/^[A-Z]/.test(k) && typeof v === 'function') {",
    "      if (v.name !== k && G[v.name] !== v) name(v, k);",
    "      fix(v); fix(v.prototype);",
    "    }",
    "  });",
    "  fix(G); fix(Object.getPrototypeOf(G));",
    "  gkeys.forEach(function(k){",
    "    if (/^[A-Z]/.test(k)) return;",
    "    var v; try { v = G[k]; } catch (e) { return; }",
    "    if (v && typeof v === 'object') fix(v);",
    "  });",
    "})(globalThis)",
);

pub fn name_engine_members(scope: &mut Scope<'_>) {
    let _ = scope.eval_native_script(MEMBER_NAMES_SRC, "<member-names>");
}
