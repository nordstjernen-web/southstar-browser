//! Southstar — the two storage areas of a page: their items, the 5 MiB quota, and the session and local areas shared by every page of the renderer, per partition and per origin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::ffi;

pub(crate) const QUOTA_BYTES: usize = 5 * 1024 * 1024;

pub(crate) type Items = BTreeMap<String, String>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Area {
    Local,
    Session,
}

impl Area {
    pub const LOCAL_TAG: i32 = 1;
    pub const SESSION_TAG: i32 = 2;

    pub fn from_tag(tag: i32) -> Option<Area> {
        match tag {
            Area::LOCAL_TAG => Some(Area::Local),
            Area::SESSION_TAG => Some(Area::Session),
            _ => None,
        }
    }
}

pub(crate) struct QuotaExceeded;

pub(crate) enum Write {
    Unchanged,
    Changed(Option<String>),
}

struct LocalArea {
    items: Items,
    dirty: bool,
}

type SharedLocal = Rc<RefCell<LocalArea>>;
type SharedSession = Rc<RefCell<Items>>;

thread_local! {
    static SESSIONS: RefCell<HashMap<String, SharedSession>> = RefCell::new(HashMap::new());
    static LOCALS: RefCell<HashMap<String, SharedLocal>> = RefCell::new(HashMap::new());
}

fn session_for(partition: &str) -> SharedSession {
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .entry(partition.to_owned())
            .or_default()
            .clone()
    })
}

fn detached_local() -> SharedLocal {
    Rc::new(RefCell::new(LocalArea {
        items: Items::new(),
        dirty: false,
    }))
}

fn local_for(origin: &str, path: Option<&str>) -> SharedLocal {
    let shared = LOCALS.with(|locals| {
        locals
            .borrow_mut()
            .entry(origin.to_owned())
            .or_insert_with(detached_local)
            .clone()
    });
    if let Some(path) = path {
        let mut area = shared.borrow_mut();
        if !area.dirty {
            area.items = ffi::read_items(path);
        }
    }
    shared
}

pub(crate) struct Storage {
    local: SharedLocal,
    session: SharedSession,
    local_origin: Option<String>,
    local_path: Option<String>,
    local_disabled: bool,
}

pub(crate) struct Snapshot {
    pub path: String,
    pub origin: Option<String>,
    pub items: Items,
}

fn bytes_used(items: &Items) -> usize {
    items.iter().map(|(k, v)| k.len() + v.len()).sum()
}

fn fits(items: &Items, key: &str, value: &str) -> bool {
    let old = items
        .get(key)
        .map_or(0, |existing| key.len() + existing.len());
    let new = key.len() + value.len();
    new <= old || bytes_used(items) + (new - old) <= QUOTA_BYTES
}

impl Storage {
    pub fn new(local_disabled: bool) -> Storage {
        Storage {
            local: detached_local(),
            session: SharedSession::default(),
            local_origin: None,
            local_path: None,
            local_disabled,
        }
    }

    fn with<R>(&self, area: Area, f: impl FnOnce(&Items) -> R) -> R {
        match area {
            Area::Local => f(&self.local.borrow().items),
            Area::Session => f(&self.session.borrow()),
        }
    }

    fn with_mut<R>(&mut self, area: Area, f: impl FnOnce(&mut Items) -> R) -> R {
        match area {
            Area::Local => {
                let mut local = self.local.borrow_mut();
                local.dirty = true;
                f(&mut local.items)
            }
            Area::Session => f(&mut self.session.borrow_mut()),
        }
    }

    pub fn get(&self, area: Area, key: &str) -> Option<String> {
        self.with(area, |items| items.get(key).cloned())
    }

    pub fn len(&self, area: Area) -> usize {
        self.with(area, Items::len)
    }

    pub fn keys(&self, area: Area) -> Vec<String> {
        self.with(area, |items| items.keys().cloned().collect())
    }

    pub fn key(&self, area: Area, index: usize) -> Option<String> {
        self.with(area, |items| items.keys().nth(index).cloned())
    }

    pub fn set(&mut self, area: Area, key: &str, value: &str) -> Result<Write, QuotaExceeded> {
        let unchanged = self.with(area, |items| {
            if !fits(items, key, value) {
                return Err(QuotaExceeded);
            }
            Ok(items.get(key).is_some_and(|old| old == value))
        })?;
        if unchanged {
            return Ok(Write::Unchanged);
        }
        let old = self.with_mut(area, |items| items.insert(key.to_owned(), value.to_owned()));
        Ok(Write::Changed(old))
    }

    pub fn remove(&mut self, area: Area, key: &str) -> Option<String> {
        if !self.with(area, |items| items.contains_key(key)) {
            return None;
        }
        self.with_mut(area, |items| items.remove(key))
    }

    pub fn clear(&mut self, area: Area) -> bool {
        if self.with(area, Items::is_empty) {
            return false;
        }
        self.with_mut(area, Items::clear);
        true
    }

    pub fn switch_session(&mut self, new_partition: &str) {
        self.session = session_for(new_partition);
    }

    pub fn wants_flush(&self) -> bool {
        self.local.borrow().dirty && !self.local_disabled
    }

    pub fn take_snapshot(&mut self) -> Option<Snapshot> {
        let mut local = self.local.borrow_mut();
        if !local.dirty {
            return None;
        }
        let path = self.local_path.clone()?;
        local.dirty = false;
        if self.local_disabled {
            return None;
        }
        Some(Snapshot {
            path,
            origin: self.local_origin.clone(),
            items: local.items.clone(),
        })
    }

    pub fn local_disabled(&self) -> bool {
        self.local_disabled
    }

    pub fn clear_local(&mut self) {
        self.local = detached_local();
    }

    pub fn same_origin(&self, origin: Option<&str>) -> bool {
        matches!((self.local_origin.as_deref(), origin), (Some(a), Some(b)) if a == b)
    }

    pub fn adopt_origin(&mut self, origin: Option<String>) {
        self.local_path = origin.as_deref().and_then(ffi::storage_path_for_origin);
        self.local = match origin.as_deref() {
            Some(origin) => local_for(origin, self.local_path.as_deref()),
            None => detached_local(),
        };
        self.local_origin = origin;
    }
}
