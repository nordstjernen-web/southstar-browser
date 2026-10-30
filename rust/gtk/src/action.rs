//! Southstar — GSimpleAction handles, the application window's action map, keyboard accelerators and the GtkSettings object.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};

use southstar_gtk_ffi as sys;

use crate::event::{connect_boxed, gcallback};
use crate::signal::SignalHandlerId;
use crate::{Application, Object, Widget, Window};

#[derive(Clone, PartialEq, Eq)]
pub struct SimpleAction(Object);

unsafe extern "C" fn on_activate<F: Fn() + 'static>(
    _action: *mut c_void,
    _parameter: *mut c_void,
    data: *mut c_void,
) {
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f();
}

impl SimpleAction {
    pub fn new(name: &CStr) -> SimpleAction {
        // SAFETY: name is NUL-terminated and a null parameter type makes a parameterless action.
        let action = unsafe { sys::g_simple_action_new(name.as_ptr(), core::ptr::null()) };
        // SAFETY: a GSimpleAction is not floating and the constructor's reference is handed over.
        SimpleAction(unsafe { Object::from_full(action) })
    }

    pub fn set_enabled(&self, enabled: bool) {
        // SAFETY: self is a live GSimpleAction.
        unsafe { sys::g_simple_action_set_enabled(self.0.as_ptr(), enabled.into()) };
    }

    pub fn connect_activate<F: Fn() + 'static>(&self, f: F) -> SignalHandlerId {
        let callback = gcallback!(on_activate::<F> => fn(*mut c_void, *mut c_void, *mut c_void));
        // SAFETY: GSimpleAction::activate takes (action, parameter, data) and returns nothing, as on_activate does.
        unsafe { connect_boxed(&self.0, c"activate", callback, f) }
    }
}

crate::widgets::widget_type!(ApplicationWindow);

impl ApplicationWindow {
    pub fn new(app: &Application) -> ApplicationWindow {
        // SAFETY: app is a live GtkApplication; the constructor returns a new window registered with it.
        ApplicationWindow(Widget::adopt(unsafe {
            sys::gtk_application_window_new(app.as_ptr())
        }))
    }

    pub fn window(&self) -> Window {
        Window::cast(self).expect("a GtkApplicationWindow is a GtkWindow")
    }

    pub fn add_action(&self, action: &SimpleAction) {
        // SAFETY: self is a live GtkApplicationWindow, a GActionMap, and the map takes its own reference to the action.
        unsafe { sys::g_action_map_add_action(self.as_ptr().cast(), action.0.as_ptr()) };
    }

    pub fn activate_action(&self, name: &CStr) {
        // SAFETY: self is a live GtkApplicationWindow, a GActionGroup; GLib ignores unknown names and a null parameter fits parameterless actions.
        unsafe {
            sys::g_action_group_activate_action(
                self.as_ptr().cast(),
                name.as_ptr(),
                core::ptr::null_mut(),
            )
        };
    }
}

impl Application {
    pub fn set_accels_for_action(&self, detailed_action: &CStr, accels: &[&CStr]) {
        let mut list: Vec<*const c_char> = accels.iter().map(|a| a.as_ptr()).collect();
        list.push(core::ptr::null());
        // SAFETY: self is a live GtkApplication, the name is NUL-terminated and the list is a null-terminated array of strings GTK copies.
        unsafe {
            sys::gtk_application_set_accels_for_action(
                self.as_ptr(),
                detailed_action.as_ptr(),
                list.as_ptr(),
            )
        };
    }
}

pub fn default_settings() -> Option<Object> {
    // SAFETY: gtk_settings_get_default has no preconditions and returns null or the display's settings object.
    let settings = unsafe { sys::gtk_settings_get_default() };
    // SAFETY: settings is null or a live GtkSettings; the handle takes its own reference.
    unsafe { Object::from_ptr(settings) }
}
