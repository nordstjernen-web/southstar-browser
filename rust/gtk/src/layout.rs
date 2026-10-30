//! Southstar — custom layouts driven by a Rust value, the widget geometry they use, and floating-reference handoff to C code.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use std::rc::Rc;

use southstar_gtk_ffi::{self as sys, GtkWidget};

use crate::{Object, Widget, ffi};

pub trait CustomLayout: 'static {
    fn measure(&self, orientation: sys::GtkOrientation, for_size: c_int) -> (c_int, c_int);
    fn allocate(&self, width: c_int, height: c_int, baseline: c_int);
}

const LAYOUT_QUARK: &CStr = c"southstar-gtk-layout:custom";

fn layout_quark() -> ffi::Quark {
    // SAFETY: the name is NUL-terminated and static.
    unsafe { ffi::g_quark_from_string(LAYOUT_QUARK.as_ptr()) }
}

unsafe extern "C" fn drop_layout(data: *mut c_void) {
    if !data.is_null() {
        // SAFETY: set_custom_layout stored this pointer from Box::into_raw; GLib frees it once.
        drop(unsafe { Box::from_raw(data.cast::<Rc<dyn CustomLayout>>()) });
    }
}

unsafe fn layout_of(widget: *mut GtkWidget) -> Option<Rc<dyn CustomLayout>> {
    // SAFETY: the caller passes the live widget GTK is laying out.
    let data = unsafe { ffi::g_object_get_qdata(widget.cast(), layout_quark()) };
    // SAFETY: the quark is private to this module, so data is null or the boxed Rc set_custom_layout stored.
    unsafe { data.cast::<Rc<dyn CustomLayout>>().as_ref() }.cloned()
}

unsafe extern "C" fn measure(
    widget: *mut GtkWidget,
    orientation: sys::GtkOrientation,
    for_size: c_int,
    minimum: *mut c_int,
    natural: *mut c_int,
    _minimum_baseline: *mut c_int,
    _natural_baseline: *mut c_int,
) {
    // SAFETY: GTK calls the layout function with the live widget it measures.
    let (min, nat) =
        unsafe { layout_of(widget) }.map_or((0, 0), |layout| layout.measure(orientation, for_size));
    // SAFETY: GTK passes null or a writable int for the minimum.
    if let Some(minimum) = unsafe { minimum.as_mut() } {
        *minimum = min;
    }
    // SAFETY: GTK passes null or a writable int for the natural size.
    if let Some(natural) = unsafe { natural.as_mut() } {
        *natural = nat;
    }
}

unsafe extern "C" fn allocate(
    widget: *mut GtkWidget,
    width: c_int,
    height: c_int,
    baseline: c_int,
) {
    // SAFETY: GTK calls the layout function with the live widget it allocates.
    if let Some(layout) = unsafe { layout_of(widget) } {
        layout.allocate(width, height, baseline);
    }
}

pub fn set_custom_layout(widget: &Widget, layout: impl CustomLayout) {
    let layout: Rc<dyn CustomLayout> = Rc::new(layout);
    let data = Box::into_raw(Box::new(layout)).cast::<c_void>();
    // SAFETY: widget is live; drop_layout frees the box when the data is replaced or the widget finalized.
    unsafe {
        ffi::g_object_set_qdata_full(
            widget.as_ptr().cast(),
            layout_quark(),
            data,
            Some(drop_layout),
        )
    };
    // SAFETY: the callbacks match GtkCustomLayout's signatures; the manager is returned with a full reference.
    let manager = unsafe { sys::gtk_custom_layout_new(None, Some(measure), Some(allocate)) };
    // SAFETY: widget is live and takes over the manager's reference.
    unsafe { sys::gtk_widget_set_layout_manager(widget.as_ptr(), manager) };
}

unsafe extern "C" {
    fn gtk_window_controls_get_type() -> sys::GType;
    fn g_object_is_floating(object: *mut c_void) -> sys::GBoolean;
    fn g_object_force_floating(object: *mut c_void);
}

crate::widgets::widget_class!(
    WindowControls => gtk_window_controls_get_type,
    HeaderBar => sys::gtk_header_bar_get_type,
);

impl WindowControls {
    pub fn new(side: sys::GtkPackType) -> WindowControls {
        // SAFETY: GTK checks the pack type and returns a new floating widget.
        let controls = Widget::adopt(unsafe { sys::gtk_window_controls_new(side) });
        WindowControls::cast(&controls).expect("gtk_window_controls_new returns GtkWindowControls")
    }
}

impl HeaderBar {
    pub fn static_type() -> sys::GType {
        // SAFETY: the type getter registers the class if needed and has no preconditions.
        unsafe { sys::gtk_header_bar_get_type() }
    }
}

impl Object {
    pub fn bind_property(
        &self,
        source_property: &CStr,
        target: &Object,
        target_property: &CStr,
        flags: sys::GBindingFlags,
    ) {
        // SAFETY: both objects are live and the names NUL-terminated; GLib warns on unknown properties, and the binding lives as long as the objects.
        unsafe {
            sys::g_object_bind_property(
                self.as_ptr(),
                source_property.as_ptr(),
                target.as_ptr(),
                target_property.as_ptr(),
                flags,
            )
        };
    }
}

impl Widget {
    /// # Safety
    /// `widget` is null or a live GtkWidget. Unlike [`Widget::from_ptr`], a floating widget stays
    /// floating, so a pointer fresh from a C constructor can be wrapped before it is parented.
    pub unsafe fn from_ptr_keep_floating(widget: *mut GtkWidget) -> Option<Widget> {
        if widget.is_null() {
            return None;
        }
        // SAFETY: the caller passes a live GtkWidget.
        let floating = unsafe { g_object_is_floating(widget.cast()) } != 0;
        // SAFETY: the caller passes a live GtkWidget.
        let handle = unsafe { Widget::from_ptr(widget) }?;
        if floating {
            // SAFETY: widget is live; this reference is the one the handle owns, the sunk one goes back to the floating owner.
            unsafe { sys::g_object_ref(widget.cast()) };
            // SAFETY: widget is live; marking the owner's reference floating again undoes the sink.
            unsafe { g_object_force_floating(widget.cast()) };
        }
        Some(handle)
    }

    pub fn into_floating(self) -> *mut GtkWidget {
        let widget = self.as_ptr();
        // SAFETY: widget is live; the handle's reference becomes the floating one the C caller receives.
        unsafe { g_object_force_floating(widget.cast()) };
        core::mem::forget(self);
        widget
    }

    pub fn measure(&self, orientation: sys::GtkOrientation, for_size: c_int) -> (c_int, c_int) {
        let (mut minimum, mut natural) = (0, 0);
        // SAFETY: self is a live widget, GTK checks the orientation and null baselines are allowed.
        unsafe {
            sys::gtk_widget_measure(
                self.as_ptr(),
                orientation,
                for_size,
                &mut minimum,
                &mut natural,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        (minimum, natural)
    }

    pub fn size_allocate(&self, allocation: sys::GtkAllocation, baseline: c_int) {
        // SAFETY: self is a live widget and allocation a valid rectangle.
        unsafe { sys::gtk_widget_size_allocate(self.as_ptr(), &allocation, baseline) };
    }

    pub fn set_child_visible(&self, visible: bool) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_child_visible(self.as_ptr(), visible.into()) };
    }

    pub fn is_rtl(&self) -> bool {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_get_direction(self.as_ptr()) == sys::TEXT_DIR_RTL }
    }

    pub fn ancestor(&self, type_: sys::GType) -> Option<Widget> {
        // SAFETY: self is a live widget; GTK only compares the type.
        let ancestor = unsafe { sys::gtk_widget_get_ancestor(self.as_ptr(), type_) };
        // SAFETY: GTK returned null or a live widget above self.
        unsafe { Widget::from_ptr(ancestor) }
    }

    pub fn compute_point(
        &self,
        target: &Widget,
        point: sys::GraphenePoint,
    ) -> Option<sys::GraphenePoint> {
        let mut out = sys::GraphenePoint::default();
        // SAFETY: both widgets are live and the points are valid for the call.
        let ok = unsafe {
            sys::gtk_widget_compute_point(self.as_ptr(), target.as_ptr(), &point, &mut out)
        };
        (ok != 0).then_some(out)
    }

    pub fn set_parent(&self, parent: &Widget) {
        // SAFETY: both widgets are live; GTK checks self has no parent yet and the parent takes a reference.
        unsafe { sys::gtk_widget_set_parent(self.as_ptr(), parent.as_ptr()) };
    }

    pub fn set_can_target(&self, can_target: bool) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_can_target(self.as_ptr(), can_target.into()) };
    }
}
