//! Southstar — image fetches for a laid-out page: the wanted-URL set, the blocking fetch on a nested loop and the incremental sessions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::CStr;
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;

use super::net::{self, Dest, NestedLoop, Outcome, Response};

pub use southstar_layout_query::collect_images;

#[derive(Clone, Copy)]
pub struct ImageCache(southstar_image::ImageCache);

impl ImageCache {
    pub fn of(cache: southstar_image::ImageCache) -> ImageCache {
        ImageCache(cache)
    }

    pub fn peek(self, url: &CStr) -> bool {
        self.0.peek(url).is_some()
    }

    fn store(self, url: &CStr, resp: &Response) {
        let Some(body) = resp.body().filter(|b| !b.is_empty()) else {
            return;
        };
        if resp.error().is_some() {
            return;
        }
        if let Some(image) = self.0.insert_encoded(url, body) {
            image.adopt_origin(resp.final_url(), resp.cors_allow_origin());
        }
    }
}

#[derive(Default)]
pub struct Wanted {
    order: RefCell<Vec<CString>>,
    set: RefCell<HashSet<CString>>,
}

impl Wanted {
    pub fn new() -> Wanted {
        Wanted::default()
    }

    pub fn contains(&self, url: &CStr) -> bool {
        self.set.borrow().contains(url)
    }

    pub fn add(&self, url: &CStr) {
        if self.set.borrow_mut().insert(url.to_owned()) {
            self.order.borrow_mut().push(url.to_owned());
        }
    }

    fn keys(&self) -> Vec<CString> {
        self.order.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.order.borrow().len()
    }

    pub fn exclude_and_record(&self, requested: &mut HashSet<CString>) {
        self.order
            .borrow_mut()
            .retain(|key| !requested.contains(key));
        self.set.borrow_mut().retain(|key| !requested.contains(key));
        requested.extend(self.order.borrow().iter().cloned());
    }
}

struct BlockingFetch {
    main_loop: NestedLoop,
    pending: Cell<usize>,
    cache: ImageCache,
}

pub fn fetch_images_blocking(wanted: &Wanted, base: &CStr, cache: ImageCache) {
    let state = Rc::new(BlockingFetch {
        main_loop: NestedLoop::new(),
        pending: Cell::new(wanted.len()),
        cache,
    });
    for key in wanted.keys() {
        let st = Rc::clone(&state);
        net::request_async(&key.clone(), base, Dest::Image, move |outcome| {
            if let Ok(resp) = outcome {
                st.cache.store(&key, &resp);
            }
            st.pending.set(st.pending.get() - 1);
            if st.pending.get() == 0 {
                st.main_loop.quit();
            }
        });
    }
    state.main_loop.run_blocking();
}

pub type Arrived = Rc<dyn Fn()>;

pub struct Session {
    dead: Cell<bool>,
    outstanding: Cell<i32>,
    cache: ImageCache,
    arrived: RefCell<Option<Arrived>>,
}

impl Session {
    fn image_fetched(&self, abs: &CStr, outcome: Outcome) {
        if let Ok(resp) = outcome
            && !self.dead.get()
        {
            self.cache.store(abs, &resp);
        }
        if self.outstanding.get() > 0 {
            self.outstanding.set(self.outstanding.get() - 1);
        }
        if !self.dead.get() {
            let arrived = self.arrived.borrow().clone();
            if let Some(arrived) = arrived {
                arrived();
            }
        }
    }
}

pub fn start_session(
    wanted: &Wanted,
    base: &CStr,
    cache: ImageCache,
    arrived: Arrived,
) -> ImageSession {
    let session = Rc::new(Session {
        dead: Cell::new(false),
        outstanding: Cell::new(wanted.len() as i32),
        cache,
        arrived: RefCell::new(Some(arrived)),
    });
    for key in wanted.keys() {
        let s = Rc::clone(&session);
        net::request_async(&key.clone(), base, Dest::Image, move |outcome| {
            s.image_fetched(&key, outcome);
        });
    }
    ImageSession(session)
}

pub struct ImageSession(Rc<Session>);

impl ImageSession {
    pub fn outstanding(&self) -> i32 {
        self.0.outstanding.get()
    }

    pub fn close(self) {
        drop(self);
    }
}

impl Drop for ImageSession {
    fn drop(&mut self) {
        self.0.dead.set(true);
        drop(self.0.arrived.replace(None));
    }
}
