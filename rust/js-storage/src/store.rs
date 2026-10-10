//! Southstar — the two storage areas of a page: their items, the 5 MiB quota, the per-partition session buckets and the persisted local area.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{BTreeMap, HashMap};

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

pub(crate) struct Storage {
    local: Items,
    session: Items,
    session_buckets: HashMap<String, Items>,
    local_origin: Option<String>,
    local_path: Option<String>,
    local_dirty: bool,
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
            local: Items::new(),
            session: Items::new(),
            session_buckets: HashMap::new(),
            local_origin: None,
            local_path: None,
            local_dirty: false,
            local_disabled,
        }
    }

    fn items(&self, area: Area) -> &Items {
        match area {
            Area::Local => &self.local,
            Area::Session => &self.session,
        }
    }

    fn items_mut(&mut self, area: Area) -> &mut Items {
        if area == Area::Local {
            self.local_dirty = true;
        }
        match area {
            Area::Local => &mut self.local,
            Area::Session => &mut self.session,
        }
    }

    pub fn get(&self, area: Area, key: &str) -> Option<String> {
        self.items(area).get(key).cloned()
    }

    pub fn len(&self, area: Area) -> usize {
        self.items(area).len()
    }

    pub fn keys(&self, area: Area) -> Vec<String> {
        self.items(area).keys().cloned().collect()
    }

    pub fn key(&self, area: Area, index: usize) -> Option<String> {
        self.items(area).keys().nth(index).cloned()
    }

    pub fn set(&mut self, area: Area, key: &str, value: &str) -> Result<Write, QuotaExceeded> {
        let items = self.items(area);
        if !fits(items, key, value) {
            return Err(QuotaExceeded);
        }
        if items.get(key).is_some_and(|old| old == value) {
            return Ok(Write::Unchanged);
        }
        let old = self
            .items_mut(area)
            .insert(key.to_owned(), value.to_owned());
        Ok(Write::Changed(old))
    }

    pub fn remove(&mut self, area: Area, key: &str) -> Option<String> {
        if !self.items(area).contains_key(key) {
            return None;
        }
        self.items_mut(area).remove(key)
    }

    pub fn clear(&mut self, area: Area) -> bool {
        if self.items(area).is_empty() {
            return false;
        }
        self.items_mut(area).clear();
        true
    }

    pub fn switch_session(&mut self, old_partition: Option<&str>, new_partition: &str) {
        if let Some(old) = old_partition {
            self.session_buckets
                .insert(old.to_owned(), self.session.clone());
        }
        self.session = self
            .session_buckets
            .get(new_partition)
            .cloned()
            .unwrap_or_default();
    }

    pub fn wants_flush(&self) -> bool {
        self.local_dirty && !self.local_disabled
    }

    pub fn take_snapshot(&mut self) -> Option<Snapshot> {
        if !self.local_dirty {
            return None;
        }
        let path = self.local_path.clone()?;
        self.local_dirty = false;
        if self.local_disabled {
            return None;
        }
        Some(Snapshot {
            path,
            origin: self.local_origin.clone(),
            items: self.local.clone(),
        })
    }

    pub fn local_disabled(&self) -> bool {
        self.local_disabled
    }

    pub fn clear_local(&mut self) {
        self.local.clear();
    }

    pub fn same_origin(&self, origin: Option<&str>) -> bool {
        matches!((self.local_origin.as_deref(), origin), (Some(a), Some(b)) if a == b)
    }

    pub fn adopt_origin(&mut self, origin: Option<String>) {
        self.local.clear();
        self.local_path = origin.as_deref().and_then(ffi::storage_path_for_origin);
        self.local_origin = origin;
        if let Some(path) = &self.local_path {
            self.local = ffi::read_items(path);
        }
    }
}
