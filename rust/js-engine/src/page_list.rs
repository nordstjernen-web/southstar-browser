//! Southstar — PageList, the small per-runtime table a binding keeps its page state in, created on first use and looked up by the runtime's key.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

pub struct PageList<K, T> {
    pages: RefCell<Vec<(K, T)>>,
}

impl<K: Copy + PartialEq, T> PageList<K, T> {
    pub const fn new() -> PageList<K, T> {
        PageList {
            pages: RefCell::new(Vec::new()),
        }
    }

    pub fn with<R>(&self, key: K, f: impl FnOnce(&mut T) -> R) -> R
    where
        T: Default,
    {
        let mut pages = self.pages.borrow_mut();
        let index = match pages.iter().position(|(owner, _)| *owner == key) {
            Some(index) => index,
            None => {
                pages.push((key, T::default()));
                pages.len() - 1
            }
        };
        f(&mut pages[index].1)
    }

    pub fn peek<R>(&self, key: K, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.pages
            .borrow()
            .iter()
            .find(|(owner, _)| *owner == key)
            .map(|(_, page)| f(page))
    }

    pub fn peek_mut<R>(&self, key: K, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.pages
            .borrow_mut()
            .iter_mut()
            .find(|(owner, _)| *owner == key)
            .map(|(_, page)| f(page))
    }

    pub fn take(&self, key: K) -> Option<T> {
        let mut pages = self.pages.borrow_mut();
        let index = pages.iter().position(|(owner, _)| *owner == key)?;
        Some(pages.swap_remove(index).1)
    }
}

impl<K: Copy + PartialEq, T> Default for PageList<K, T> {
    fn default() -> PageList<K, T> {
        PageList::new()
    }
}
