//! Southstar — GMenu models, their items and the menu button that shows one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_gtk_ffi as sys;

use crate::{MenuButton, Object, Widget};

#[derive(Clone, PartialEq, Eq)]
pub struct Menu(Object);

#[derive(Clone, PartialEq, Eq)]
pub struct MenuItem(Object);

impl Menu {
    pub fn new() -> Menu {
        // SAFETY: g_menu_new has no preconditions.
        let menu = unsafe { sys::g_menu_new() };
        // SAFETY: a GMenu is not floating and the constructor's reference is handed over.
        Menu(unsafe { Object::from_full(menu) })
    }

    pub fn append(&self, label: &CStr, detailed_action: &CStr) {
        // SAFETY: self is a live GMenu and both strings are NUL-terminated; GLib copies them.
        unsafe { sys::g_menu_append(self.0.as_ptr(), label.as_ptr(), detailed_action.as_ptr()) };
    }

    pub fn append_item(&self, item: &MenuItem) {
        // SAFETY: self is a live GMenu and item a live GMenuItem whose attributes the menu copies.
        unsafe { sys::g_menu_append_item(self.0.as_ptr(), item.0.as_ptr()) };
    }

    pub fn append_section(&self, section: &Menu) {
        // SAFETY: self and section are live GMenus; a null label makes an unlabelled section and the menu takes its own reference.
        unsafe {
            sys::g_menu_append_section(self.0.as_ptr(), core::ptr::null(), section.0.as_ptr())
        };
    }
}

impl Default for Menu {
    fn default() -> Menu {
        Menu::new()
    }
}

impl MenuItem {
    pub fn new(label: &CStr, detailed_action: &CStr) -> MenuItem {
        // SAFETY: both strings are NUL-terminated; GLib copies them.
        let item = unsafe { sys::g_menu_item_new(label.as_ptr(), detailed_action.as_ptr()) };
        // SAFETY: a GMenuItem is not floating and the constructor's reference is handed over.
        MenuItem(unsafe { Object::from_full(item) })
    }

    pub fn set_attribute_string(&self, attribute: &CStr, value: &CStr) {
        // SAFETY: value is NUL-terminated; the floating variant returned is sunk by the item below.
        let variant = unsafe { sys::g_variant_new_string(value.as_ptr()) };
        // SAFETY: self is a live GMenuItem, attribute is NUL-terminated and variant a fresh floating GVariant the item takes over.
        unsafe {
            sys::g_menu_item_set_attribute_value(self.0.as_ptr(), attribute.as_ptr(), variant)
        };
    }
}

impl MenuButton {
    pub fn new() -> MenuButton {
        // SAFETY: gtk_menu_button_new returns a new floating widget.
        MenuButton::cast(&Widget::adopt(unsafe { sys::gtk_menu_button_new() }))
            .expect("gtk_menu_button_new returns a GtkMenuButton")
    }

    pub fn set_menu_model(&self, menu: &Menu) {
        // SAFETY: self is a live GtkMenuButton and menu a live GMenuModel the button references.
        unsafe { sys::gtk_menu_button_set_menu_model(self.as_ptr(), menu.0.as_ptr()) };
    }

    pub fn popover(&self) -> Option<Widget> {
        // SAFETY: self is a live GtkMenuButton.
        let popover = unsafe { sys::gtk_menu_button_get_popover(self.as_ptr()) };
        // SAFETY: GTK returned null or the live popover the button owns.
        unsafe { Widget::from_ptr(popover) }
    }
}

impl Default for MenuButton {
    fn default() -> MenuButton {
        MenuButton::new()
    }
}
