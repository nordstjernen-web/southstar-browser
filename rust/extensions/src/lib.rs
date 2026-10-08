//! Southstar — the WebExtensions host: loads manifests, assembles content scripts and decides which requests extensions block.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod rules;

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use southstar_js_engine::{Attributes, Engine, NativeFn, Scope, Value};

use crate::rules::{CosmeticRule, Rule};

const APP_DIR_NAME: &[u8] = b"southstar";
const JSON_STACK_SIZE: usize = 1024 * 1024;

const SHIM_PRELUDE: &[u8] = b";(function(G){\
var M=G.__nd_ext_manifest,B=G.__nd_ext_base,\
SR=G.__nd_ext_sread,SW=G.__nd_ext_swrite,\
PL=G.__nd_ext_platform,UL=G.__nd_ext_uilang;\
delete G.__nd_ext_manifest;delete G.__nd_ext_base;\
delete G.__nd_ext_sread;delete G.__nd_ext_swrite;\
delete G.__nd_ext_platform;delete G.__nd_ext_uilang;\
function area(id,name){\
function rd(){try{return JSON.parse(SR(id,name))||{};}catch(e){return{};}}\
function wr(o){return SW(id,name,JSON.stringify(o));}\
return {get:function(keys){return new Promise(function(res){var a=rd(),o={};\
if(keys==null)o=a;\
else if(typeof keys==='string'){if(keys in a)o[keys]=a[keys];}\
else if(Array.isArray(keys)){keys.forEach(function(k){if(k in a)o[k]=a[k];});}\
else if(typeof keys==='object'){Object.keys(keys).forEach(function(k){o[k]=(k in a)?a[k]:keys[k];});}\
res(o);});},\
set:function(items){return new Promise(function(res){var a=rd();\
Object.keys(items||{}).forEach(function(k){a[k]=items[k];});wr(a);res();});},\
remove:function(keys){return new Promise(function(res){var a=rd();\
(Array.isArray(keys)?keys:[keys]).forEach(function(k){delete a[k];});wr(a);res();});},\
clear:function(){return new Promise(function(res){wr({});res();});}};}\
function make_api(id){\
var man=null;\
function getManifest(){if(man===null){try{man=JSON.parse(M(id));}catch(e){man={};}}return man;}\
function getURL(p){var b=B(id);p=String(p==null?'':p);\
if(p.charAt(0)==='/')p=p.slice(1);return b?('file://'+b+'/'+p):p;}\
var listeners=[];\
var runtime={id:id,lastError:null,getManifest:getManifest,getURL:getURL,\
getPlatformInfo:function(){return Promise.resolve({os:PL(),arch:'x86-64'});},\
sendMessage:function(){var msg=arguments.length>1?arguments[1]:arguments[0];\
return new Promise(function(res){var s={id:id},rep;\
for(var i=0;i<listeners.length;i++){try{var r=listeners[i](msg,s,function(x){rep=x;});\
if(r&&typeof r.then==='function'){r.then(res);return;}}catch(e){}}\
res(rep);});},\
onMessage:{addListener:function(f){if(typeof f==='function')listeners.push(f);},\
removeListener:function(f){var i=listeners.indexOf(f);if(i>=0)listeners.splice(i,1);},\
hasListener:function(f){return listeners.indexOf(f)>=0;}}};\
var i18n={getMessage:function(k){return k==null?'':String(k);},\
getUILanguage:function(){return UL();},\
getAcceptLanguages:function(){return Promise.resolve([UL()]);}};\
return {runtime:runtime,i18n:i18n,extension:{getURL:getURL},\
storage:{local:area(id,'local'),sync:area(id,'sync'),managed:area(id,'managed')}};\
}\n";

const SHIM_EPILOGUE: &[u8] = b"})(this);\n";

struct ContentScript {
    matches: Vec<Vec<u8>>,
    js: Option<Vec<u8>>,
    css: Option<Vec<u8>>,
    at_start: bool,
}

struct Extension {
    id: Vec<u8>,
    base_dir: Vec<u8>,
    manifest: Vec<u8>,
    content_scripts: Vec<ContentScript>,
    rules: Vec<Rule>,
    cosmetic: Vec<CosmeticRule>,
}

#[derive(Default)]
struct Registry {
    extensions: Vec<Extension>,
    blocked_hosts: HashSet<Vec<u8>>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::load)
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&c| c == 0)
        .map_or(bytes, |nul| &bytes[..nul])
}

fn c_string(scope: &mut Scope<'_>, value: &Value) -> Result<Vec<u8>, Value> {
    scope
        .to_bytes(value)
        .map(|bytes| until_nul(&bytes).to_vec())
}

fn string_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Vec<u8>> {
    let value = scope.get(object, key).ok()?;
    if !value.is_string() {
        return None;
    }
    c_string(scope, &value).ok()
}

fn object_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Value> {
    scope.get(object, key).ok().filter(Value::is_object)
}

fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    scope
        .get(array, "length")
        .and_then(|length| scope.to_int32(&length))
        .map_or(0, |length| length as u32)
}

fn collect_strings(scope: &mut Scope<'_>, array: &Value, out: &mut Vec<Vec<u8>>) {
    if !array.is_object() {
        return;
    }
    for i in 0..length(scope, array) {
        let Ok(item) = scope.get_index(array, i) else {
            continue;
        };
        if let Ok(text) = c_string(scope, &item) {
            out.push(text);
        }
    }
}

fn strings_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Ok(list) = scope.get(object, key) {
        collect_strings(scope, &list, &mut out);
    }
    out
}

fn domains_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Vec<Vec<u8>> {
    let mut domains = strings_property(scope, object, key);
    for domain in &mut domains {
        domain.make_ascii_lowercase();
    }
    domains
}

fn safe_path(base_dir: &[u8], relative: &[u8]) -> Option<Vec<u8>> {
    if relative.is_empty() {
        return None;
    }
    let base = ffi::canonicalize(base_dir);
    let path = ffi::canonicalize(&ffi::build_filename(&[base_dir, relative]));
    let inside = path.starts_with(&base)
        && path
            .get(base.len())
            .is_none_or(|&c| c == ffi::DIR_SEPARATOR);
    inside.then_some(path)
}

fn read_files(scope: &mut Scope<'_>, base_dir: &[u8], entry: &Value, key: &str) -> Option<Vec<u8>> {
    let mut source = Vec::new();
    for relative in strings_property(scope, entry, key) {
        let Some(data) = safe_path(base_dir, &relative).and_then(|path| ffi::read_file(&path))
        else {
            continue;
        };
        source.extend_from_slice(until_nul(&data));
        source.push(b'\n');
    }
    (!source.is_empty()).then_some(source)
}

fn parse_rule(scope: &mut Scope<'_>, value: &Value) -> Option<Rule> {
    if !value.is_object() {
        return None;
    }
    let kind = object_property(scope, value, "action")
        .and_then(|action| string_property(scope, &action, "type"));
    let allow = match kind.as_deref() {
        Some(b"block") => false,
        Some(b"allow" | b"allowAllRequests") => true,
        _ => return None,
    };
    let mut rule = Rule::new(allow);
    if let Ok(priority) = scope.get(value, "priority") {
        if priority.is_number() {
            rule.priority = scope.to_int32(&priority).unwrap_or(1);
        }
    }
    if let Some(condition) = object_property(scope, value, "condition") {
        rule.url_filter = string_property(scope, &condition, "urlFilter");
        rule.case_sensitive = scope
            .get(&condition, "isUrlFilterCaseSensitive")
            .is_ok_and(|flag| scope.to_bool(&flag));
        if let Some(pattern) = string_property(scope, &condition, "regexFilter") {
            rule.regex = ffi::Regex::new(&pattern, rule.case_sensitive);
        }
        rule.request_domains = domains_property(scope, &condition, "requestDomains");
        rule.excluded_request_domains =
            domains_property(scope, &condition, "excludedRequestDomains");
        rule.initiator_domains = domains_property(scope, &condition, "initiatorDomains");
        rule.excluded_initiator_domains =
            domains_property(scope, &condition, "excludedInitiatorDomains");
        rule.resource_types = domains_property(scope, &condition, "resourceTypes");
        rule.excluded_resource_types = domains_property(scope, &condition, "excludedResourceTypes");
    }
    Some(rule)
}

fn parse_json(scope: &mut Scope<'_>, path: &[u8]) -> Option<(Vec<u8>, Value)> {
    let raw = ffi::read_file(path)?;
    let value = scope
        .parse_json(&raw, &String::from_utf8_lossy(path))
        .ok()?;
    Some((raw, value))
}

fn parse_id(scope: &mut Scope<'_>, manifest: &Value, dir: &[u8]) -> Vec<u8> {
    for key in ["browser_specific_settings", "applications"] {
        let id = object_property(scope, manifest, key)
            .and_then(|settings| object_property(scope, &settings, "gecko"))
            .and_then(|gecko| string_property(scope, &gecko, "id"));
        if let Some(id) = id {
            return id;
        }
    }
    ffi::basename(dir)
}

impl Extension {
    fn parse_content_scripts(&mut self, scope: &mut Scope<'_>, manifest: &Value) {
        let Some(list) = object_property(scope, manifest, "content_scripts") else {
            return;
        };
        for i in 0..length(scope, &list) {
            let Some(entry) = scope.get_index(&list, i).ok().filter(Value::is_object) else {
                continue;
            };
            let matches = strings_property(scope, &entry, "matches");
            let at_start = string_property(scope, &entry, "run_at")
                .is_some_and(|run_at| run_at == b"document_start");
            let js = read_files(scope, &self.base_dir, &entry, "js");
            let css = read_files(scope, &self.base_dir, &entry, "css");
            if !matches.is_empty() && (js.is_some() || css.is_some()) {
                self.content_scripts.push(ContentScript {
                    matches,
                    js,
                    css,
                    at_start,
                });
            }
        }
    }

    fn load_rule_file(&mut self, scope: &mut Scope<'_>, relative: &[u8]) {
        let Some((_, list)) =
            safe_path(&self.base_dir, relative).and_then(|path| parse_json(scope, &path))
        else {
            return;
        };
        if !list.is_object() {
            return;
        }
        for i in 0..length(scope, &list) {
            let Ok(value) = scope.get_index(&list, i) else {
                continue;
            };
            if let Some(rule) = parse_rule(scope, &value) {
                self.rules.push(rule);
            }
        }
    }

    fn parse_declarative_net_request(&mut self, scope: &mut Scope<'_>, manifest: &Value) {
        let Some(resources) = object_property(scope, manifest, "declarative_net_request")
            .and_then(|dnr| object_property(scope, &dnr, "rule_resources"))
        else {
            return;
        };
        for i in 0..length(scope, &resources) {
            let Some(item) = scope.get_index(&resources, i).ok().filter(Value::is_object) else {
                continue;
            };
            let enabled = match scope.get(&item, "enabled") {
                Ok(flag) if flag.is_bool() => scope.to_bool(&flag),
                _ => true,
            };
            if let Some(path) = string_property(scope, &item, "path") {
                if enabled {
                    self.load_rule_file(scope, &path);
                }
            }
        }
    }

    fn parse_filter_lists(
        &mut self,
        scope: &mut Scope<'_>,
        manifest: &Value,
        hosts: &mut HashSet<Vec<u8>>,
    ) {
        let Some(list) = object_property(scope, manifest, "southstar_filter_lists") else {
            return;
        };
        let mut paths = Vec::new();
        collect_strings(scope, &list, &mut paths);
        for path in paths {
            let Some(text) =
                safe_path(&self.base_dir, &path).and_then(|full| ffi::read_file(&full))
            else {
                continue;
            };
            rules::parse_filter_list(until_nul(&text), &mut self.rules, &mut self.cosmetic, hosts);
        }
    }
}

impl Registry {
    fn load() -> Registry {
        let mut registry = Registry::default();
        let mut engine = Engine::new(Path::new(""));
        engine.set_max_stack_size(JSON_STACK_SIZE);
        engine.enter(|scope| {
            if let Some(dirs) = ffi::getenv(c"NS_EXTENSIONS_DIR") {
                for root in dirs.split(|&c| c == ffi::SEARCHPATH_SEPARATOR) {
                    registry.scan_root(scope, root);
                }
            }
            let default =
                ffi::build_filename(&[&ffi::user_data_dir(), APP_DIR_NAME, b"extensions"]);
            registry.scan_root(scope, &default);
        });
        registry
    }

    fn scan_root(&mut self, scope: &mut Scope<'_>, root: &[u8]) {
        if root.is_empty() {
            return;
        }
        if ffi::exists(&ffi::build_filename(&[root, b"manifest.json"])) {
            self.load_one(scope, root);
            return;
        }
        for name in ffi::list_dir(root) {
            let child = ffi::build_filename(&[root, &name]);
            if ffi::is_dir(&child) {
                self.load_one(scope, &child);
            }
        }
    }

    fn load_one(&mut self, scope: &mut Scope<'_>, dir: &[u8]) {
        let Some((raw, manifest)) =
            parse_json(scope, &ffi::build_filename(&[dir, b"manifest.json"]))
        else {
            return;
        };
        let mut extension = Extension {
            id: parse_id(scope, &manifest, dir),
            base_dir: dir.to_vec(),
            manifest: raw,
            content_scripts: Vec::new(),
            rules: Vec::new(),
            cosmetic: Vec::new(),
        };
        extension.parse_content_scripts(scope, &manifest);
        extension.parse_declarative_net_request(scope, &manifest);
        extension.parse_filter_lists(scope, &manifest, &mut self.blocked_hosts);
        self.extensions.push(extension);
    }

    fn lookup(&self, id: &[u8]) -> Option<&Extension> {
        self.extensions.iter().find(|extension| extension.id == id)
    }

    fn cosmetic_css(&self, host: &[u8]) -> Option<Vec<u8>> {
        if host.is_empty() {
            return None;
        }
        let mut css = Vec::new();
        for rule in self
            .extensions
            .iter()
            .flat_map(|extension| &extension.cosmetic)
        {
            if rule.applies_to(host) {
                css.extend_from_slice(rule.selector());
                css.extend_from_slice(b"{display:none !important}\n");
            }
        }
        (!css.is_empty()).then_some(css)
    }
}

fn append_id(out: &mut Vec<u8>, id: &[u8]) {
    for &c in id {
        if c == b'\\' || c == b'"' {
            out.push(b'\\');
        }
        out.push(c);
    }
}

fn append_js_string(out: &mut Vec<u8>, text: &[u8]) {
    out.push(b'"');
    for &c in text {
        match c {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'"' => out.extend_from_slice(b"\\\""),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            _ => out.push(c),
        }
    }
    out.push(b'"');
}

fn storage_path(id: &[u8], area: &[u8]) -> Option<Vec<u8>> {
    if southstar_config::private_mode() || !matches!(area, b"local" | b"sync" | b"managed") {
        return None;
    }
    let dir = ffi::build_filename(&[
        &ffi::user_data_dir(),
        APP_DIR_NAME,
        b"ext-storage",
        &ffi::sha256_hex(id),
    ]);
    ffi::make_private_dir(&dir);
    Some(ffi::build_filename(&[&dir, &[area, b".json"].concat()]))
}

fn native_manifest(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(id) = args.first() else {
        return Ok(scope.string("{}"));
    };
    let id = c_string(scope, id)?;
    let text = registry()
        .lookup(&id)
        .map_or(&b"{}"[..], |extension| until_nul(&extension.manifest));
    Ok(scope.string_from_bytes(text))
}

fn native_base(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(id) = args.first() else {
        return Ok(scope.string(""));
    };
    let id = c_string(scope, id)?;
    let base = registry()
        .lookup(&id)
        .map_or(&b""[..], |extension| &extension.base_dir);
    Ok(scope.string_from_bytes(base))
}

fn native_storage_read(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let [id, area, ..] = args else {
        return Ok(scope.string("{}"));
    };
    let id = c_string(scope, id)?;
    let area = c_string(scope, area)?;
    let data = storage_path(&id, &area).and_then(|path| ffi::read_file(&path));
    Ok(match data {
        Some(data) => scope.string_from_bytes(until_nul(&data)),
        None => scope.string("{}"),
    })
}

fn native_storage_write(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let [id, area, json, ..] = args else {
        return Ok(Value::boolean(false));
    };
    let id = c_string(scope, id)?;
    let area = c_string(scope, area)?;
    let json = c_string(scope, json)?;
    let written =
        storage_path(&id, &area).is_some_and(|path| ffi::write_private_file(&path, &json));
    Ok(Value::boolean(written))
}

fn native_platform(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let os = if cfg!(windows) {
        "win"
    } else if cfg!(target_vendor = "apple") {
        "mac"
    } else {
        "linux"
    };
    Ok(scope.string(os))
}

fn native_ui_language(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let name = ffi::language_name().unwrap_or_else(|| b"en".to_vec());
    let mut tag: Vec<u8> = name
        .iter()
        .take_while(|&&c| c != b'.' && c != b'@')
        .map(|&c| if c == b'_' { b'-' } else { c })
        .collect();
    if tag.is_empty() {
        tag.extend_from_slice(b"en");
    }
    Ok(scope.string_from_bytes(&tag))
}

fn install_natives(scope: &mut Scope<'_>, global: &Value) {
    let natives: [(&str, &str, u32, NativeFn); 6] = [
        ("__nd_ext_manifest", "m", 1, native_manifest),
        ("__nd_ext_base", "b", 1, native_base),
        ("__nd_ext_sread", "r", 2, native_storage_read),
        ("__nd_ext_swrite", "w", 3, native_storage_write),
        ("__nd_ext_platform", "p", 0, native_platform),
        ("__nd_ext_uilang", "l", 0, native_ui_language),
    ];
    let attributes = Attributes {
        writable: true,
        enumerable: true,
        configurable: true,
    };
    for (key, name, arity, native) in natives {
        let function = scope.function(name, arity, native);
        let _ = scope.define(global, key, function, attributes);
    }
}

pub fn content_scripts_for_url(
    scope: &mut Scope<'_>,
    global: &Value,
    url: Option<&[u8]>,
    at_start: bool,
) -> Option<Vec<u8>> {
    let registry = registry();
    let url = url.filter(|url| !url.is_empty())?;
    if registry.extensions.is_empty() {
        return None;
    }
    let mut body = Vec::new();
    if at_start {
        let css = rules::split_url(url)
            .and_then(|parts| registry.cosmetic_css(rules::strip_port(&parts.host)));
        if let Some(css) = css {
            body.extend_from_slice(
                b";(function(){try{var __ndch=document.createElement('style');__ndch.textContent=",
            );
            append_js_string(&mut body, &css);
            body.extend_from_slice(
                b";(document.head||document.documentElement||document).appendChild(__ndch);}catch(e){}})();\n",
            );
        }
    }
    for extension in &registry.extensions {
        for script in &extension.content_scripts {
            if script.at_start != at_start
                || !script
                    .matches
                    .iter()
                    .any(|pattern| rules::pattern_match(pattern, url))
            {
                continue;
            }
            body.extend_from_slice(
                b";(function(browser){var chrome=browser,make_api,area,SR,SW,M,B,PL,UL;try{\n",
            );
            if let Some(css) = &script.css {
                body.extend_from_slice(
                    b"var __ndcss=document.createElement('style');__ndcss.textContent=",
                );
                append_js_string(&mut body, css);
                body.extend_from_slice(
                    b";(document.head||document.documentElement||document).appendChild(__ndcss);\n",
                );
            }
            if let Some(js) = &script.js {
                body.extend_from_slice(js);
                body.push(b'\n');
            }
            body.extend_from_slice(
                b"}catch(e){try{console.error(\"[southstar ext]\",e);}catch(_){}}})(",
            );
            if script.js.is_some() {
                body.extend_from_slice(b"make_api(\"");
                append_id(&mut body, &extension.id);
                body.extend_from_slice(b"\")");
            } else {
                body.extend_from_slice(b"null");
            }
            body.extend_from_slice(b");\n");
        }
    }
    if body.is_empty() {
        return None;
    }
    install_natives(scope, global);
    Some([SHIM_PRELUDE, &body, SHIM_EPILOGUE].concat())
}

pub fn should_block(url: Option<&[u8]>, initiator: Option<&[u8]>) -> bool {
    let registry = registry();
    let Some(url) = url else {
        return false;
    };
    if registry.extensions.is_empty()
        || !(url.starts_with(b"http://") || url.starts_with(b"https://"))
    {
        return false;
    }
    let Some(request) = rules::split_url(url) else {
        return false;
    };
    let initiator_host = initiator.and_then(rules::split_url).map(|parts| parts.host);
    let party = rules::third_party(&request.host, initiator_host.as_deref());
    let host = rules::strip_port(&request.host);
    let initiator = initiator_host.as_deref().map(rules::strip_port);
    let (mut best, mut allow, mut block) = (-1, false, false);
    if rules::host_indexed(&registry.blocked_hosts, &request.host) {
        best = 1;
        block = true;
    }
    for rule in registry
        .extensions
        .iter()
        .flat_map(|extension| &extension.rules)
    {
        if !rule.matches(url, host, initiator, party) {
            continue;
        }
        match rule.priority.cmp(&best) {
            Ordering::Greater => {
                best = rule.priority;
                allow = rule.allow;
                block = !rule.allow;
            }
            Ordering::Equal if rule.allow => allow = true,
            Ordering::Equal => block = true,
            Ordering::Less => {}
        }
    }
    best >= 0 && block && !allow
}
