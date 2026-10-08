//! Southstar — IndexedDB storage in per-origin SQLite files, behind the __nd_idb backend object the IndexedDB bindings call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::CStr;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};
use southstar_sqlite::{
    Db, SQLITE_CONSTRAINT, SQLITE_OPEN_CREATE, SQLITE_OPEN_NOFOLLOW, SQLITE_OPEN_READONLY,
    SQLITE_OPEN_READWRITE, SharedDb, Step,
};

const MAX_OPEN: usize = 16;
const SIBLING_TTL_US: i64 = 5_000_000;
const MAX_PAGES: i64 = 65_536;
const MAX_ORIGIN_PAGES: i64 = 256 * 1024;
const MEMORY: &[u8] = b":memory:";

struct Handle {
    db: SharedDb,
    path: Vec<u8>,
}

struct Entry {
    handle: Arc<Handle>,
    last_used: u64,
}

#[derive(Default)]
struct Cache {
    handles: HashMap<Vec<u8>, Entry>,
    clock: u64,
    siblings: HashMap<Vec<u8>, (i64, i64)>,
}

fn cache() -> MutexGuard<'static, Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn c(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn partition(scope: &Scope<'_>) -> Option<Vec<u8>> {
    ffi::partition(scope).filter(|p| !p.is_empty() && p != b"null" && !p.starts_with(b"opaque://"))
}

fn partition_dir(scope: &Scope<'_>) -> Option<Vec<u8>> {
    if southstar_config::private_mode() {
        return None;
    }
    let partition = partition(scope)?;
    let hash = ffi::sha256_hex(&partition);
    let dir = ffi::build_filename(&[&ffi::user_data_dir(), b"southstar", b"indexeddb", &hash]);
    ffi::make_private_dir(&dir);
    Some(dir)
}

fn path_for_name(scope: &Scope<'_>, name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() {
        return None;
    }
    if southstar_config::private_mode() {
        return Some(MEMORY.to_vec());
    }
    let dir = partition_dir(scope)?;
    let mut file = ffi::sha256_hex(name);
    file.extend_from_slice(b".sqlite");
    Some(ffi::build_filename(&[&dir, &file]))
}

fn cache_key(scope: &Scope<'_>, name: &[u8]) -> Option<Vec<u8>> {
    let mut key = partition(scope).filter(|_| !name.is_empty())?;
    key.push(0x1f);
    key.extend_from_slice(name);
    Some(key)
}

fn exec(db: &Db, sql: &CStr) -> bool {
    match db.exec(sql) {
        Ok(()) => true,
        Err(message) => {
            let mut warning = b"idb: sqlite exec failed: ".to_vec();
            warning.extend_from_slice(&message);
            ffi::warning(&warning);
            false
        }
    }
}

fn schema(db: &Db) -> bool {
    let max_pages = c(format!("PRAGMA max_page_count={MAX_PAGES}").as_bytes());
    [
        c"PRAGMA foreign_keys=ON",
        c"PRAGMA journal_mode=WAL",
        c"PRAGMA synchronous=NORMAL",
        c"PRAGMA cache_size=-512",
        &max_pages,
        c"CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
        c"CREATE TABLE IF NOT EXISTS stores(name TEXT PRIMARY KEY,key_path TEXT NOT NULL,auto_increment INTEGER NOT NULL,key_gen INTEGER NOT NULL DEFAULT 1)",
        c"CREATE TABLE IF NOT EXISTS records(store TEXT NOT NULL,key TEXT NOT NULL,value BLOB NOT NULL,PRIMARY KEY(store,key),FOREIGN KEY(store) REFERENCES stores(name) ON DELETE CASCADE ON UPDATE CASCADE)",
        c"CREATE TABLE IF NOT EXISTS indexes(store TEXT NOT NULL,name TEXT NOT NULL,key_path TEXT NOT NULL,unique_index INTEGER NOT NULL,multi_entry INTEGER NOT NULL,PRIMARY KEY(store,name),FOREIGN KEY(store) REFERENCES stores(name) ON DELETE CASCADE ON UPDATE CASCADE)",
        c"CREATE TABLE IF NOT EXISTS index_records(store TEXT NOT NULL,name TEXT NOT NULL,index_key TEXT NOT NULL,primary_key TEXT NOT NULL,PRIMARY KEY(store,name,index_key,primary_key),FOREIGN KEY(store,name) REFERENCES indexes(store,name) ON DELETE CASCADE ON UPDATE CASCADE,FOREIGN KEY(store,primary_key) REFERENCES records(store,key) ON DELETE CASCADE ON UPDATE CASCADE)",
        c"CREATE INDEX IF NOT EXISTS idx_records_store ON records(store)",
        c"CREATE INDEX IF NOT EXISTS idx_index_records_lookup ON index_records(store,name,index_key)",
    ]
    .iter()
    .all(|sql| exec(db, sql))
}

fn trim(cache: &mut Cache) {
    if cache.handles.len() <= MAX_OPEN {
        return;
    }
    let lru = |memory: bool| {
        cache
            .handles
            .iter()
            .filter(|(_, e)| (e.handle.path == MEMORY) == memory)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| k.clone())
    };
    if let Some(victim) = lru(false).or_else(|| lru(true)) {
        cache.handles.remove(&victim);
    }
}

fn open_db(scope: &Scope<'_>, name: &[u8]) -> Option<Arc<Handle>> {
    let key = cache_key(scope, name)?;
    {
        let mut guard = cache();
        let cache = &mut *guard;
        if let Some(entry) = cache.handles.get_mut(&key) {
            cache.clock += 1;
            entry.last_used = cache.clock;
            return Some(entry.handle.clone());
        }
    }
    let path = path_for_name(scope, name)?;
    let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_NOFOLLOW;
    let db = match SharedDb::open(&c(&path), flags) {
        Ok(db) => db,
        Err(message) => {
            let mut warning = b"idb: could not open ".to_vec();
            warning.extend_from_slice(&path);
            warning.extend_from_slice(b": ");
            warning.extend_from_slice(&message);
            ffi::warning(&warning);
            return None;
        }
    };
    db.harden();
    db.busy_timeout(2500);
    if !schema(&db) {
        return None;
    }
    let handle = Arc::new(Handle { db, path });
    let mut guard = cache();
    let cache = &mut *guard;
    cache.clock += 1;
    let entry = Entry {
        handle: handle.clone(),
        last_used: cache.clock,
    };
    cache.handles.insert(key, entry);
    trim(cache);
    Some(handle)
}

fn set_meta(db: &Db, key: &CStr, value: &[u8]) -> bool {
    let Some(st) = db.prepare(c"INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
    else {
        return false;
    };
    st.bind_text(1, Some(key));
    st.bind_text(2, Some(&c(value)));
    st.step_result() == Step::Done
}

fn get_meta(db: &Db, key: &CStr) -> Option<Vec<u8>> {
    let st = db.prepare(c"SELECT value FROM meta WHERE key=?")?;
    st.bind_text(1, Some(key));
    if st.step() { st.column_text(0) } else { None }
}

fn get_version(db: &Db) -> i64 {
    match get_meta(db, c"version") {
        Some(v) if !v.is_empty() => ffi::ascii_strtoll(&v),
        _ => 0,
    }
}

fn read_value(scope: &mut Scope<'_>, blob: Option<&[u8]>) -> Value {
    match blob {
        Some(blob) => scope
            .read_object(blob)
            .unwrap_or_else(|_| Value::undefined()),
        None => Value::undefined(),
    }
}

fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

fn string(scope: &mut Scope<'_>, bytes: Option<&[u8]>, default: &[u8]) -> Value {
    scope.string_from_bytes(bytes.unwrap_or(default))
}

fn store_array(scope: &mut Scope<'_>, db: &Db) -> Value {
    let arr = scope.new_array();
    let Some(st) =
        db.prepare(c"SELECT name,key_path,auto_increment,key_gen FROM stores ORDER BY name")
    else {
        return arr;
    };
    let mut i = 0;
    while st.step() {
        let name = st.column_text(0);
        let key_path = st.column_text(1);
        let store = scope.new_object();
        let v = string(scope, name.as_deref(), b"");
        set(scope, &store, "name", v);
        let v = string(scope, key_path.as_deref(), b"null");
        set(scope, &store, "keyPath", v);
        set(
            scope,
            &store,
            "autoIncrement",
            Value::boolean(st.column_int(2) != 0),
        );
        set(
            scope,
            &store,
            "keyGenerator",
            Value::int64(st.column_int64(3)),
        );
        let indexes = scope.new_array();
        if let Some(ist) = db.prepare(c"SELECT name,key_path,unique_index,multi_entry FROM indexes WHERE store=? ORDER BY name") {
            ist.bind_text(1, Some(&c(name.as_deref().unwrap_or_default())));
            let mut j = 0;
            while ist.step() {
                let index = scope.new_object();
                let v = string(scope, ist.column_text(0).as_deref(), b"");
                set(scope, &index, "name", v);
                let v = string(scope, ist.column_text(1).as_deref(), b"null");
                set(scope, &index, "keyPath", v);
                set(scope, &index, "unique", Value::boolean(ist.column_int(2) != 0));
                set(scope, &index, "multiEntry", Value::boolean(ist.column_int(3) != 0));
                let _ = scope.set_index(&indexes, j, index);
                j += 1;
            }
        }
        set(scope, &store, "indexes", indexes);
        let _ = scope.set_index(&arr, i, store);
        i += 1;
    }
    arr
}

fn info_for(scope: &mut Scope<'_>, handle: &Handle, name: &[u8]) -> Value {
    if get_meta(&handle.db, c"name").is_none() {
        set_meta(&handle.db, c"name", name);
    }
    let obj = scope.new_object();
    let v = scope.string_from_bytes(name);
    set(scope, &obj, "name", v);
    set(
        scope,
        &obj,
        "version",
        Value::int64(get_version(&handle.db)),
    );
    let stores = store_array(scope, &handle.db);
    set(scope, &obj, "stores", stores);
    obj
}

fn throw(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    scope.dom_exception(name, message)
}

fn cannot_open(scope: &mut Scope<'_>) -> Value {
    throw(scope, "UnknownError", "Could not open IndexedDB database")
}

fn throw_sql(scope: &mut Scope<'_>, db: &Db) -> Value {
    throw(scope, "UnknownError", &text(&db.errmsg()))
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_else(Value::undefined)
}

fn c_arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> Result<Vec<u8>, Value> {
    let bytes = scope.to_bytes(&arg(args, i))?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(bytes[..end].to_vec())
}

fn c_args<const N: usize>(scope: &mut Scope<'_>, args: &[Value]) -> Result<[Vec<u8>; N], Value> {
    let mut error = None;
    let values: [Vec<u8>; N] = core::array::from_fn(|i| match c_arg(scope, args, i) {
        Ok(v) => v,
        Err(e) => {
            error = Some(e);
            Vec::new()
        }
    });
    match error {
        Some(e) => Err(e),
        None => Ok(values),
    }
}

fn result(scope: &mut Scope<'_>, ok: bool, db: &Db) -> Result<Value, Value> {
    if ok {
        Ok(Value::boolean(true))
    } else {
        Err(throw_sql(scope, db))
    }
}

fn backend_open(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.is_empty() {
        return Err(throw(scope, "TypeError", "Database name is required"));
    }
    let name = c_arg(scope, args, 0)?;
    let Some(handle) = open_db(scope, &name) else {
        return Err(cannot_open(scope));
    };
    Ok(info_for(scope, &handle, &name))
}

fn backend_info(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.is_empty() {
        return Ok(Value::null());
    }
    let name = c_arg(scope, args, 0)?;
    let Some(handle) = open_db(scope, &name) else {
        return Err(cannot_open(scope));
    };
    Ok(info_for(scope, &handle, &name))
}

fn backend_set_version(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 2 {
        return Ok(Value::boolean(false));
    }
    let name = c_arg(scope, args, 0)?;
    let version = scope.to_int64(&args[1])?;
    let Some(handle) = open_db(scope, &name) else {
        return Err(cannot_open(scope));
    };
    Ok(Value::boolean(set_meta(
        &handle.db,
        c"version",
        version.to_string().as_bytes(),
    )))
}

fn insert_unique(
    scope: &mut Scope<'_>,
    db: &Db,
    sql: &CStr,
    texts: &[&[u8]],
    ints: &[bool],
    exists: &str,
) -> Result<Value, Value> {
    let Some(st) = db.prepare(sql) else {
        return result(scope, false, db);
    };
    for (i, t) in texts.iter().enumerate() {
        st.bind_text(i as i32 + 1, Some(&c(t)));
    }
    for (i, flag) in ints.iter().enumerate() {
        st.bind_int((texts.len() + i) as i32 + 1, i32::from(*flag));
    }
    let ok = st.step_result() == Step::Done;
    if !ok && db.errcode() == SQLITE_CONSTRAINT {
        drop(st);
        return Err(throw(scope, "ConstraintError", exists));
    }
    drop(st);
    result(scope, ok, db)
}

fn backend_create_store(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 4 {
        return Ok(Value::boolean(false));
    }
    let strings = c_args::<3>(scope, args);
    let auto_increment = scope.to_bool(&args[3]);
    let [db_name, store, key_path] = strings?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    insert_unique(
        scope,
        &handle.db,
        c"INSERT INTO stores(name,key_path,auto_increment,key_gen) VALUES(?,?,?,1)",
        &[&store, &key_path],
        &[auto_increment],
        "Object store already exists",
    )
}

fn run(db: &Db, sql: &CStr, texts: &[&[u8]]) -> bool {
    let Some(st) = db.prepare(sql) else {
        return false;
    };
    for (i, t) in texts.iter().enumerate() {
        st.bind_text(i as i32 + 1, Some(&c(t)));
    }
    st.step_result() == Step::Done
}

fn backend_delete_store(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 2 {
        return Ok(Value::boolean(false));
    }
    let [db_name, store] = c_args::<2>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let ok = run(&handle.db, c"DELETE FROM stores WHERE name=?", &[&store]);
    result(scope, ok, &handle.db)
}

fn backend_create_index(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 6 {
        return Ok(Value::boolean(false));
    }
    let strings = c_args::<4>(scope, args);
    let unique = scope.to_bool(&args[4]);
    let multi = scope.to_bool(&args[5]);
    let [db_name, store, name, key_path] = strings?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    insert_unique(
        scope,
        &handle.db,
        c"INSERT INTO indexes(store,name,key_path,unique_index,multi_entry) VALUES(?,?,?,?,?)",
        &[&store, &name, &key_path],
        &[unique, multi],
        "Index already exists",
    )
}

fn backend_delete_index(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 3 {
        return Ok(Value::boolean(false));
    }
    let [db_name, store, name] = c_args::<3>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let ok = run(
        &handle.db,
        c"DELETE FROM indexes WHERE store=? AND name=?",
        &[&store, &name],
    );
    result(scope, ok, &handle.db)
}

fn next_key_in(db: &Db, store: &[u8]) -> Option<i64> {
    let st = db.prepare(c"SELECT key_gen FROM stores WHERE name=?")?;
    st.bind_text(1, Some(&c(store)));
    if !st.step() {
        return None;
    }
    let key = st.column_int64(0);
    drop(st);
    let st = db.prepare(c"UPDATE stores SET key_gen=? WHERE name=?")?;
    st.bind_int64(1, key.wrapping_add(1));
    st.bind_text(2, Some(&c(store)));
    (st.step_result() == Step::Done).then_some(key)
}

fn backend_next_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let [db_name, store] = c_args::<2>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let db = &handle.db;
    if !db.exec_quiet(c"BEGIN IMMEDIATE") {
        return Err(throw_sql(scope, db));
    }
    let key = next_key_in(db, &store);
    db.exec_quiet(if key.is_some() {
        c"COMMIT"
    } else {
        c"ROLLBACK"
    });
    match key {
        Some(key) => Ok(Value::int64(key)),
        None => Err(throw_sql(scope, db)),
    }
}

enum Failure {
    Sql,
    Thrown(Value),
}

fn kept<T>(thrown: &mut Option<Value>, result: Result<T, Value>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            *thrown = Some(error);
            None
        }
    }
}

fn insert_index_entries(
    scope: &mut Scope<'_>,
    db: &Db,
    store: &[u8],
    key: &[u8],
    entries: &Value,
) -> Result<(), Failure> {
    if !entries.is_array() {
        return Ok(());
    }
    let length = scope
        .get(entries, "length")
        .unwrap_or_else(|_| Value::undefined());
    let len = scope.to_int32(&length).unwrap_or(0) as u32;
    let Some(st) = db.prepare(
        c"INSERT OR IGNORE INTO index_records(store,name,index_key,primary_key) VALUES(?,?,?,?)",
    ) else {
        return Err(Failure::Sql);
    };
    for i in 0..len {
        let mut thrown = None;
        let entry = kept(&mut thrown, scope.get_index(entries, i));
        let name = entry
            .as_ref()
            .and_then(|e| kept(&mut thrown, scope.get(e, "name")));
        let index_key = entry
            .as_ref()
            .and_then(|e| kept(&mut thrown, scope.get(e, "key")));
        let name = name.and_then(|v| kept(&mut thrown, scope.to_bytes(&v)));
        let index_key = index_key.and_then(|v| kept(&mut thrown, scope.to_bytes(&v)));
        let (Some(name), Some(index_key)) = (name, index_key) else {
            return Err(thrown.map_or(Failure::Sql, Failure::Thrown));
        };
        st.reset();
        st.bind_text(1, Some(&c(store)));
        st.bind_text(2, Some(&c(&name)));
        st.bind_text(3, Some(&c(&index_key)));
        st.bind_text(4, Some(&c(key)));
        if st.step_result() != Step::Done {
            return Err(Failure::Sql);
        }
    }
    Ok(())
}

fn db_pages(db: &Db) -> i64 {
    match db.prepare(c"PRAGMA page_count") {
        Some(st) if st.step() => st.column_int64(0),
        _ => 0,
    }
}

fn max_origin_pages() -> i64 {
    static CACHED: OnceLock<i64> = OnceLock::new();
    *CACHED.get_or_init(
        || match ffi::env(c"NS_IDB_MAX_ORIGIN_PAGES").filter(|v| !v.is_empty()) {
            Some(v) if ffi::ascii_strtoll(&v) > 0 => ffi::ascii_strtoll(&v),
            _ => MAX_ORIGIN_PAGES,
        },
    )
}

fn sqlite_files(dir: &[u8]) -> Vec<Vec<u8>> {
    ffi::dir_entries(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.ends_with(b".sqlite"))
        .map(|entry| ffi::build_filename(&[dir, &entry]))
        .collect()
}

fn scan_sibling_pages(dir: &[u8], current: &[u8]) -> i64 {
    sqlite_files(dir)
        .into_iter()
        .filter(|path| path != current)
        .filter_map(|path| Db::open_with(&c(&path), SQLITE_OPEN_READONLY).ok())
        .map(|db| db_pages(&db))
        .sum()
}

fn origin_pages(scope: &Scope<'_>, handle: &Handle) -> i64 {
    let total = db_pages(&handle.db);
    let Some(dir) = partition_dir(scope) else {
        return total;
    };
    let now = ffi::monotonic_us();
    let mut guard = cache();
    let fresh = guard
        .siblings
        .get(&dir)
        .filter(|(_, stamp)| now - stamp <= SIBLING_TTL_US)
        .map(|(p, _)| *p);
    let pages = match fresh {
        Some(pages) => pages,
        None => {
            let pages = scan_sibling_pages(&dir, &handle.path);
            guard.siblings.insert(dir, (pages, now));
            pages
        }
    };
    total + pages
}

fn numeric_key(scope: &mut Scope<'_>, value: &Value) -> Option<f64> {
    if value.is_undefined() {
        return None;
    }
    scope.to_number(value).ok()
}

fn put_in(
    scope: &mut Scope<'_>,
    handle: &Handle,
    store: &[u8],
    key: &[u8],
    args: &[Value],
    numeric: Option<f64>,
) -> Result<(), Failure> {
    let db = &handle.db;
    let st = db
        .prepare(c"INSERT INTO records(store,key,value) VALUES(?,?,?) ON CONFLICT(store,key) DO UPDATE SET value=excluded.value")
        .ok_or(Failure::Sql)?;
    st.bind_text(1, Some(&c(store)));
    st.bind_text(2, Some(&c(key)));
    let blob = scope
        .write_object(&args[3])
        .map_err(|e| e.map_or(Failure::Sql, Failure::Thrown))?;
    if !st.bind_blob(3, &blob) || st.step_result() != Step::Done {
        return Err(Failure::Sql);
    }
    drop(st);
    if !run(
        db,
        c"DELETE FROM index_records WHERE store=? AND primary_key=?",
        &[store, key],
    ) {
        return Err(Failure::Sql);
    }
    insert_index_entries(scope, db, store, key, &args[5])?;
    if let Some(numeric) = numeric.filter(|n| *n >= 1.0) {
        if let Some(st) = db.prepare(c"UPDATE stores SET key_gen=max(key_gen, ?) WHERE name=?") {
            let next = numeric + 1.0;
            st.bind_int64(
                1,
                if next >= 9_223_372_036_854_775_807.0 {
                    i64::MAX
                } else {
                    next as i64
                },
            );
            st.bind_text(2, Some(&c(store)));
            if st.step_result() != Step::Done {
                return Err(Failure::Sql);
            }
        }
    }
    if origin_pages(scope, handle) > max_origin_pages() {
        return Err(Failure::Thrown(throw(
            scope,
            "QuotaExceededError",
            "IndexedDB origin storage limit reached",
        )));
    }
    Ok(())
}

fn key_exists(db: &Db, store: &[u8], key: &[u8]) -> Option<bool> {
    let st = db.prepare(c"SELECT 1 FROM records WHERE store=? AND key=?")?;
    st.bind_text(1, Some(&c(store)));
    st.bind_text(2, Some(&c(key)));
    Some(st.step())
}

fn backend_put(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 7 {
        return Ok(Value::boolean(false));
    }
    let strings = c_args::<3>(scope, args);
    let add_only = scope.to_bool(&args[4]);
    let numeric = numeric_key(scope, &args[6]);
    let [db_name, store, key] = strings?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let db = &handle.db;
    if !db.exec_quiet(c"BEGIN IMMEDIATE") {
        return Err(throw_sql(scope, db));
    }
    let outcome = if add_only {
        match key_exists(db, &store, &key) {
            Some(true) => {
                db.exec_quiet(c"ROLLBACK");
                return Err(throw(scope, "ConstraintError", "Key already exists"));
            }
            Some(false) => put_in(scope, &handle, &store, &key, args, numeric),
            None => Err(Failure::Sql),
        }
    } else {
        put_in(scope, &handle, &store, &key, args, numeric)
    };
    db.exec_quiet(if outcome.is_ok() {
        c"COMMIT"
    } else {
        c"ROLLBACK"
    });
    match outcome {
        Ok(()) => Ok(Value::boolean(true)),
        Err(Failure::Thrown(e)) => Err(e),
        Err(Failure::Sql) => Err(throw_sql(scope, db)),
    }
}

fn backend_get(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 3 {
        return Ok(Value::undefined());
    }
    let [db_name, store, key] = c_args::<3>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let Some(st) = handle
        .db
        .prepare(c"SELECT value FROM records WHERE store=? AND key=?")
    else {
        return Ok(Value::undefined());
    };
    st.bind_text(1, Some(&c(&store)));
    st.bind_text(2, Some(&c(&key)));
    if !st.step() {
        return Ok(Value::undefined());
    }
    Ok(read_value(scope, st.column_blob(0)))
}

fn backend_records(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 2 {
        return Ok(scope.new_array());
    }
    let [db_name, store] = c_args::<2>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let arr = scope.new_array();
    if let Some(st) = handle
        .db
        .prepare(c"SELECT key,value FROM records WHERE store=?")
    {
        st.bind_text(1, Some(&c(&store)));
        let mut i = 0;
        while st.step() {
            let record = scope.new_object();
            let v = string(scope, st.column_text(0).as_deref(), b"");
            set(scope, &record, "key", v);
            let v = read_value(scope, st.column_blob(1));
            set(scope, &record, "value", v);
            let _ = scope.set_index(&arr, i, record);
            i += 1;
        }
    }
    Ok(arr)
}

fn backend_index_records(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 3 {
        return Ok(scope.new_array());
    }
    let [db_name, store, index] = c_args::<3>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let arr = scope.new_array();
    if let Some(st) = handle.db.prepare(
        c"SELECT ir.index_key,ir.primary_key,r.value FROM index_records ir JOIN records r ON r.store=ir.store AND r.key=ir.primary_key WHERE ir.store=? AND ir.name=?",
    ) {
        st.bind_text(1, Some(&c(&store)));
        st.bind_text(2, Some(&c(&index)));
        let mut i = 0;
        while st.step() {
            let record = scope.new_object();
            let v = string(scope, st.column_text(0).as_deref(), b"");
            set(scope, &record, "key", v);
            let v = string(scope, st.column_text(1).as_deref(), b"");
            set(scope, &record, "primaryKey", v);
            let v = read_value(scope, st.column_blob(2));
            set(scope, &record, "value", v);
            let _ = scope.set_index(&arr, i, record);
            i += 1;
        }
    }
    Ok(arr)
}

fn backend_delete_record(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 3 {
        return Ok(Value::boolean(false));
    }
    let [db_name, store, key] = c_args::<3>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let ok = run(
        &handle.db,
        c"DELETE FROM records WHERE store=? AND key=?",
        &[&store, &key],
    );
    result(scope, ok, &handle.db)
}

fn backend_clear(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 2 {
        return Ok(Value::boolean(false));
    }
    let [db_name, store] = c_args::<2>(scope, args)?;
    let Some(handle) = open_db(scope, &db_name) else {
        return Err(cannot_open(scope));
    };
    let ok = run(&handle.db, c"DELETE FROM records WHERE store=?", &[&store]);
    result(scope, ok, &handle.db)
}

fn backend_delete_database(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.is_empty() {
        return Ok(Value::boolean(false));
    }
    let name = c_arg(scope, args, 0)?;
    let path = path_for_name(scope, &name);
    let key = cache_key(scope, &name);
    let Some(path) = path else {
        return Err(throw(scope, "SecurityError", "Storage is unavailable"));
    };
    if let Some(key) = key {
        cache().handles.remove(&key);
    }
    if path == MEMORY {
        return Ok(Value::boolean(true));
    }
    ffi::unlink(&path);
    for suffix in [&b"-wal"[..], b"-shm"] {
        let mut side = path.clone();
        side.extend_from_slice(suffix);
        ffi::unlink(&side);
    }
    Ok(Value::boolean(true))
}

fn backend_databases(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let arr = scope.new_array();
    let Some(dir) = partition_dir(scope) else {
        return Ok(arr);
    };
    let mut i = 0;
    for path in sqlite_files(&dir) {
        let Ok(db) = Db::open_with(&c(&path), SQLITE_OPEN_READONLY) else {
            continue;
        };
        db.harden();
        let name = get_meta(&db, c"name");
        let version = get_version(&db);
        drop(db);
        let Some(name) = name else { continue };
        let info = scope.new_object();
        let v = scope.string_from_bytes(&name);
        set(scope, &info, "name", v);
        set(scope, &info, "version", Value::int64(version));
        let _ = scope.set_index(&arr, i, info);
        i += 1;
    }
    Ok(arr)
}

pub fn install(scope: &mut Scope<'_>, global: &Value) {
    let backend = scope.new_object();
    let functions: [(&str, u32, NativeFn); 16] = [
        ("open", 1, backend_open),
        ("info", 1, backend_info),
        ("setVersion", 2, backend_set_version),
        ("createStore", 4, backend_create_store),
        ("deleteStore", 2, backend_delete_store),
        ("createIndex", 6, backend_create_index),
        ("deleteIndex", 3, backend_delete_index),
        ("nextKey", 2, backend_next_key),
        ("put", 7, backend_put),
        ("get", 3, backend_get),
        ("records", 2, backend_records),
        ("indexRecords", 3, backend_index_records),
        ("deleteRecord", 3, backend_delete_record),
        ("clear", 2, backend_clear),
        ("deleteDatabase", 1, backend_delete_database),
        ("databases", 0, backend_databases),
    ];
    for (name, arity, f) in functions {
        let function = scope.function(name, arity, f);
        set(scope, &backend, name, function);
    }
    let _ = scope.define(global, "__nd_idb", backend, Attributes::CONFIGURABLE);
}
