//! Southstar â€” GTK event controllers and input-method contexts whose signals take Rust closures through trampolines of each signal's exact C signature.
//! Copyright 2026 Andreas RÃ¸sdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};

use southstar_gtk_ffi::{self as sys, GBoolean, GValue, GdkModifierType};

use crate::signal::{self, SignalHandlerId};
use crate::{Object, Widget};

macro_rules! gcallback {
    ($f:expr => fn($($arg:ty),*) $(-> $ret:ty)?) => {{
        let f: unsafe extern "C" fn($($arg),*) $(-> $ret)? = $f;
        // SAFETY: only the pointer's type changes; GLib casts it back to the signal's own signature, which the caller matched, before calling it.
        unsafe { core::mem::transmute::<unsafe extern "C" fn($($arg),*) $(-> $ret)?, sys::GCallback>(f) }
    }};
}

pub(crate) use gcallback;

/// # Safety
/// `instance` is a live GObject.
pub(crate) unsafe fn borrow_object(instance: *mut c_void) -> Object {
    // SAFETY: the caller passes a live GObject; the added reference becomes the handle's.
    unsafe { sys::g_object_ref(instance) };
    // SAFETY: the handle takes over the reference just added and releases only that one, so a floating reference stays floating.
    unsafe { Object::from_full(instance) }
}

pub(crate) mod sealed {
    pub trait Wrap: Sized {
        fn wrap(object: crate::Object) -> Self;
    }
}

pub trait ObjectType: sealed::Wrap {}

impl<T: sealed::Wrap> ObjectType for T {}

/// # Safety
/// `object` is null or a live instance of the GObject class `T` stands for. The handle takes its own reference and, unlike `Widget::from_ptr`, leaves a floating reference floating.
pub unsafe fn from_borrowed<T: ObjectType>(object: *mut c_void) -> Option<T> {
    // SAFETY: the caller passes a live instance when it is not null.
    (!object.is_null()).then(|| T::wrap(unsafe { borrow_object(object) }))
}

pub(crate) trait Reply {
    type C;
    fn into_c(self) -> Self::C;
}

impl Reply for () {
    type C = ();
    fn into_c(self) {}
}

impl Reply for bool {
    type C = GBoolean;
    fn into_c(self) -> GBoolean {
        GBoolean::from(self)
    }
}

/// # Safety
/// `callback` has exactly the C signature of `signal`'s handlers on `object` and reads its user data as an `F`.
pub(crate) unsafe fn connect_boxed<F: 'static>(
    object: &Object,
    signal: &CStr,
    callback: sys::GCallback,
    f: F,
) -> SignalHandlerId {
    let data = Box::into_raw(Box::new(f)).cast::<c_void>();
    // SAFETY: object is live, signal is NUL-terminated, the caller vouches for callback, and drop_closure frees data with the handler.
    let id = unsafe {
        sys::g_signal_connect_data(
            object.as_ptr(),
            signal.as_ptr(),
            callback,
            data,
            Some(signal::drop_closure::<F>),
            0,
        )
    };
    SignalHandlerId::from_raw(id)
}

pub(crate) unsafe extern "C" fn on_pair<T, A, R, F>(
    target: *mut c_void,
    a: A,
    b: A,
    data: *mut c_void,
) -> R::C
where
    T: sealed::Wrap,
    R: Reply,
    F: Fn(&T, A, A) -> R + 'static,
{
    // SAFETY: GLib passes the live instance the handler was connected on, which is a T.
    let target = T::wrap(unsafe { borrow_object(target) });
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(&target, a, b).into_c()
}

unsafe extern "C" fn on_press<F: Fn(&GestureClick, c_int, f64, f64) + 'static>(
    gesture: *mut c_void,
    n_press: c_int,
    x: f64,
    y: f64,
    data: *mut c_void,
) {
    // SAFETY: GLib passes the live GtkGestureClick the handler was connected on.
    let gesture = <GestureClick as sealed::Wrap>::wrap(unsafe { borrow_object(gesture) });
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(&gesture, n_press, x, y);
}

unsafe extern "C" fn on_key<R: Reply, F: Fn(c_uint, c_uint, GdkModifierType) -> R + 'static>(
    _controller: *mut c_void,
    keyval: c_uint,
    keycode: c_uint,
    state: GdkModifierType,
    data: *mut c_void,
) -> R::C {
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(keyval, keycode, state).into_c()
}

fn take_path(file: *mut c_void) -> Option<Vec<u8>> {
    // SAFETY: file is a live GFile of the dropped list; the path is a new string or null.
    let path = unsafe { sys::g_file_get_path(file) };
    if path.is_null() {
        return None;
    }
    // SAFETY: GIO returned a NUL-terminated string the caller owns.
    let bytes = unsafe { CStr::from_ptr(path) }.to_bytes().to_vec();
    // SAFETY: the string was allocated by GLib for the caller and is not used again.
    unsafe { sys::g_free(path.cast()) };
    Some(bytes)
}

fn file_list_paths(value: *const GValue) -> Option<Vec<Vec<u8>>> {
    // SAFETY: the GDK type getter has no preconditions.
    let file_list = unsafe { sys::gdk_file_list_get_type() };
    // SAFETY: GTK passes the dropped value, valid for the call.
    if unsafe { sys::g_type_check_value_holds(value, file_list) } == 0 {
        return None;
    }
    // SAFETY: value holds a GdkFileList boxed value owned by the value.
    let list = unsafe { sys::g_value_get_boxed(value) };
    if list.is_null() {
        return None;
    }
    // SAFETY: list is the live GdkFileList; the returned list is the caller's, its files the list's.
    let files = unsafe { sys::gdk_file_list_get_files(list.cast()) };
    let mut paths = Vec::new();
    let mut node = files;
    // SAFETY: node is null or a live element of the list just returned.
    while let Some(item) = unsafe { node.as_ref() } {
        paths.extend(take_path(item.data));
        node = item.next;
    }
    // SAFETY: frees the list cells only; the files belong to the GdkFileList.
    unsafe { sys::g_slist_free(files) };
    Some(paths)
}

unsafe extern "C" fn on_drop_files<F: Fn(&[Vec<u8>], f64, f64) -> bool + 'static>(
    _target: *mut c_void,
    value: *const GValue,
    x: f64,
    y: f64,
    data: *mut c_void,
) -> GBoolean {
    let Some(paths) = file_list_paths(value) else {
        return 0;
    };
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    GBoolean::from(f(&paths, x, y))
}

unsafe extern "C" fn on_commit<F: Fn(&[u8]) + 'static>(
    _context: *mut c_void,
    text: *const c_char,
    data: *mut c_void,
) {
    if text.is_null() {
        return;
    }
    // SAFETY: GTK passes the committed NUL-terminated string, valid for the call.
    let text = unsafe { CStr::from_ptr(text) }.to_bytes();
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(text);
}

macro_rules! controller_class {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Clone, PartialEq, Eq)]
            pub struct $name(EventController);

            impl core::ops::Deref for $name {
                type Target = EventController;

                fn deref(&self) -> &EventController {
                    &self.0
                }
            }

            impl AsRef<EventController> for $name {
                fn as_ref(&self) -> &EventController {
                    &self.0
                }
            }

            impl From<$name> for EventController {
                fn from(controller: $name) -> EventController {
                    controller.0
                }
            }

            impl sealed::Wrap for $name {
                fn wrap(object: Object) -> $name {
                    $name(EventController(object))
                }
            }
        )*
    };
}

#[derive(Clone, PartialEq, Eq)]
pub struct EventController(Object);

impl AsRef<EventController> for EventController {
    fn as_ref(&self) -> &EventController {
        self
    }
}

impl EventController {
    fn adopt(controller: *mut c_void) -> EventController {
        // SAFETY: GTK controller constructors return a new, non-floating reference that the handle takes over.
        EventController(unsafe { Object::from_full(controller) })
    }

    pub fn as_ptr(&self) -> *mut sys::GtkEventController {
        self.0.as_ptr().cast()
    }

    pub fn current_event_state(&self) -> GdkModifierType {
        // SAFETY: self is a live event controller.
        unsafe { sys::gtk_event_controller_get_current_event_state(self.as_ptr()) }
    }

    pub fn current_event_modifier_state(&self) -> Option<GdkModifierType> {
        // SAFETY: self is a live event controller; the event is borrowed for this dispatch.
        let event = unsafe { sys::gtk_event_controller_get_current_event(self.as_ptr()) };
        // SAFETY: event is checked non-null and stays alive while it is being handled.
        (!event.is_null()).then(|| unsafe { sys::gdk_event_get_modifier_state(event) })
    }
}

controller_class!(
    GestureClick,
    GestureDrag,
    EventControllerMotion,
    EventControllerKey,
    EventControllerScroll,
    DropTarget,
);

fn set_button(gesture: &EventController, button: c_uint) {
    // SAFETY: gesture is a live GtkGestureSingle.
    unsafe { sys::gtk_gesture_single_set_button(gesture.as_ptr().cast(), button) };
}

impl GestureClick {
    pub fn new(button: c_uint) -> GestureClick {
        // SAFETY: gtk_gesture_click_new has no preconditions.
        let gesture = GestureClick(EventController::adopt(unsafe {
            sys::gtk_gesture_click_new()
        }));
        set_button(&gesture, button);
        gesture
    }

    pub fn current_button(&self) -> c_uint {
        // SAFETY: self is a live GtkGestureSingle.
        unsafe { sys::gtk_gesture_single_get_current_button(self.as_ptr().cast()) }
    }

    pub fn claim(&self) {
        // SAFETY: self is a live GtkGesture and the state is a valid GtkEventSequenceState.
        unsafe { sys::gtk_gesture_set_state(self.as_ptr().cast(), sys::EVENT_SEQUENCE_CLAIMED) };
    }

    pub fn connect_pressed(
        &self,
        f: impl Fn(&GestureClick, c_int, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_press(self, c"pressed", f)
    }

    pub fn connect_released(
        &self,
        f: impl Fn(&GestureClick, c_int, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_press(self, c"released", f)
    }
}

fn connect_press<F: Fn(&GestureClick, c_int, f64, f64) + 'static>(
    gesture: &GestureClick,
    signal: &CStr,
    f: F,
) -> SignalHandlerId {
    let callback = gcallback!(on_press::<F> => fn(*mut c_void, c_int, f64, f64, *mut c_void));
    // SAFETY: GtkGestureClick::pressed and ::released take (gesture, n_press, x, y, data), as on_press does.
    unsafe { connect_boxed(&gesture.0.0, signal, callback, f) }
}

fn connect_xy<T: sealed::Wrap + AsRef<EventController>, F: Fn(&T, f64, f64) + 'static>(
    target: &T,
    signal: &CStr,
    f: F,
) -> SignalHandlerId {
    let callback = gcallback!(on_pair::<T, f64, (), F> => fn(*mut c_void, f64, f64, *mut c_void));
    // SAFETY: every signal passed here takes (instance, double, double, data) and returns nothing, as on_pair does for ().
    unsafe { connect_boxed(&target.as_ref().0, signal, callback, f) }
}

impl GestureDrag {
    pub fn new(button: c_uint) -> GestureDrag {
        // SAFETY: gtk_gesture_drag_new has no preconditions.
        let gesture = GestureDrag(EventController::adopt(
            unsafe { sys::gtk_gesture_drag_new() }.cast(),
        ));
        set_button(&gesture, button);
        gesture
    }

    pub fn connect_drag_begin(
        &self,
        f: impl Fn(&GestureDrag, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_xy(self, c"drag-begin", f)
    }

    pub fn connect_drag_update(
        &self,
        f: impl Fn(&GestureDrag, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_xy(self, c"drag-update", f)
    }

    pub fn connect_drag_end(
        &self,
        f: impl Fn(&GestureDrag, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_xy(self, c"drag-end", f)
    }
}

impl EventControllerMotion {
    pub fn new() -> EventControllerMotion {
        // SAFETY: gtk_event_controller_motion_new has no preconditions.
        EventControllerMotion(EventController::adopt(
            unsafe { sys::gtk_event_controller_motion_new() }.cast(),
        ))
    }

    pub fn connect_motion(
        &self,
        f: impl Fn(&EventControllerMotion, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_xy(self, c"motion", f)
    }
}

impl Default for EventControllerMotion {
    fn default() -> EventControllerMotion {
        EventControllerMotion::new()
    }
}

impl EventControllerKey {
    pub fn new() -> EventControllerKey {
        // SAFETY: gtk_event_controller_key_new has no preconditions.
        EventControllerKey(EventController::adopt(unsafe {
            sys::gtk_event_controller_key_new()
        }))
    }

    pub fn set_im_context(&self, context: &IMContext) {
        // SAFETY: self is a live key controller and context a live IM context it takes a reference to.
        unsafe { sys::gtk_event_controller_key_set_im_context(self.as_ptr(), context.as_ptr()) };
    }

    pub fn connect_key_pressed<F: Fn(c_uint, c_uint, GdkModifierType) -> bool + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(on_key::<bool, F> =>
            fn(*mut c_void, c_uint, c_uint, GdkModifierType, *mut c_void) -> GBoolean);
        // SAFETY: GtkEventControllerKey::key-pressed takes (controller, keyval, keycode, state, data) and returns gboolean, as on_key does for bool.
        unsafe { connect_boxed(&self.0.0, c"key-pressed", callback, f) }
    }

    pub fn connect_key_released<F: Fn(c_uint, c_uint, GdkModifierType) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(on_key::<(), F> =>
            fn(*mut c_void, c_uint, c_uint, GdkModifierType, *mut c_void));
        // SAFETY: GtkEventControllerKey::key-released takes (controller, keyval, keycode, state, data) and returns nothing, as on_key does for ().
        unsafe { connect_boxed(&self.0.0, c"key-released", callback, f) }
    }
}

impl Default for EventControllerKey {
    fn default() -> EventControllerKey {
        EventControllerKey::new()
    }
}

impl EventControllerScroll {
    pub fn new(flags: c_uint) -> EventControllerScroll {
        // SAFETY: GTK checks the flags; the constructor has no other preconditions.
        EventControllerScroll(EventController::adopt(
            unsafe { sys::gtk_event_controller_scroll_new(flags) }.cast(),
        ))
    }

    pub fn unit(&self) -> c_int {
        // SAFETY: self is a live scroll controller.
        unsafe { sys::gtk_event_controller_scroll_get_unit(self.as_ptr()) }
    }

    pub fn connect_scroll<F: Fn(&EventControllerScroll, f64, f64) -> bool + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(on_pair::<EventControllerScroll, f64, bool, F> =>
            fn(*mut c_void, f64, f64, *mut c_void) -> GBoolean);
        // SAFETY: GtkEventControllerScroll::scroll takes (controller, dx, dy, data) and returns gboolean, as on_pair does for bool.
        unsafe { connect_boxed(&self.0.0, c"scroll", callback, f) }
    }

    pub fn connect_decelerate(
        &self,
        f: impl Fn(&EventControllerScroll, f64, f64) + 'static,
    ) -> SignalHandlerId {
        connect_xy(self, c"decelerate", f)
    }
}

impl DropTarget {
    pub fn for_files(actions: c_uint) -> DropTarget {
        // SAFETY: the GDK type getter has no preconditions.
        let file_list = unsafe { sys::gdk_file_list_get_type() };
        // SAFETY: GTK checks the actions; the constructor returns a new reference.
        let target = unsafe { sys::gtk_drop_target_new(file_list, actions) };
        DropTarget(EventController::adopt(target.cast()))
    }

    pub fn connect_drop_files<F: Fn(&[Vec<u8>], f64, f64) -> bool + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(on_drop_files::<F> =>
            fn(*mut c_void, *const GValue, f64, f64, *mut c_void) -> GBoolean);
        // SAFETY: GtkDropTarget::drop takes (target, value, x, y, data) and returns gboolean, as on_drop_files does.
        unsafe { connect_boxed(&self.0.0, c"drop", callback, f) }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct IMContext(Object);

impl sealed::Wrap for IMContext {
    fn wrap(object: Object) -> IMContext {
        IMContext(object)
    }
}

impl IMContext {
    pub fn new_multicontext() -> IMContext {
        // SAFETY: gtk_im_multicontext_new has no preconditions.
        let context = unsafe { sys::gtk_im_multicontext_new() };
        // SAFETY: the constructor returned a new, non-floating reference the handle takes over.
        IMContext(unsafe { Object::from_full(context.cast()) })
    }

    pub fn as_ptr(&self) -> *mut sys::GtkIMContext {
        self.0.as_ptr().cast()
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0.as_ptr();
        core::mem::forget(self);
        raw
    }

    pub fn set_client_widget(&self, widget: Option<&Widget>) {
        let widget = widget.map_or(core::ptr::null_mut(), Widget::as_ptr);
        // SAFETY: self is a live IM context and widget null or a live widget.
        unsafe { sys::gtk_im_context_set_client_widget(self.as_ptr(), widget) };
    }

    pub fn connect_commit<F: Fn(&[u8]) + 'static>(&self, f: F) -> SignalHandlerId {
        let callback = gcallback!(on_commit::<F> => fn(*mut c_void, *const c_char, *mut c_void));
        // SAFETY: GtkIMContext::commit takes (context, text, data) and returns nothing, as on_commit does.
        unsafe { connect_boxed(&self.0, c"commit", callback, f) }
    }
}

impl Widget {
    pub fn add_controller(&self, controller: impl Into<EventController>) {
        let controller = controller.into();
        let raw = controller.as_ptr();
        core::mem::forget(controller);
        // SAFETY: self is a live widget and raw a live controller whose reference, given up by the handle above, the widget takes over.
        unsafe { sys::gtk_widget_add_controller(self.as_ptr(), raw.cast()) };
    }
}

pub fn keyval_to_unicode(keyval: c_uint) -> u32 {
    // SAFETY: a pure table lookup over any keyval.
    unsafe { sys::gdk_keyval_to_unicode(keyval) }
}

pub fn keyval_to_lower(keyval: c_uint) -> c_uint {
    // SAFETY: a pure table lookup over any keyval.
    unsafe { sys::gdk_keyval_to_lower(keyval) }
}

pub fn keyval_name(keyval: c_uint) -> Option<Vec<u8>> {
    // SAFETY: any keyval is accepted; the name is null or a string GDK owns until the next call.
    let name = unsafe { sys::gdk_keyval_name(keyval) };
    // SAFETY: name is checked non-null and copied before GDK can reuse it.
    (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_bytes().to_vec())
}
