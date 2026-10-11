//! Southstar — the GTK 4, GDK, GObject, cairo and graphene declarations the Rust shell code calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_double, c_float, c_int, c_uint, c_void};

mod widgets;

pub use widgets::*;

pub type GType = usize;
pub type GBoolean = c_int;
pub type GDestroyNotify = Option<unsafe extern "C" fn(data: *mut c_void)>;

pub type GtkOrientation = c_int;
pub const ORIENTATION_HORIZONTAL: GtkOrientation = 0;
pub const ORIENTATION_VERTICAL: GtkOrientation = 1;

pub type GtkTextDirection = c_int;
pub const TEXT_DIR_RTL: GtkTextDirection = 2;

pub type GtkPackType = c_int;
pub const PACK_START: GtkPackType = 0;
pub const PACK_END: GtkPackType = 1;

pub type GtkAlign = c_int;
pub const ALIGN_CENTER: GtkAlign = 3;

pub type GtkEntryIconPosition = c_int;

pub type GBindingFlags = c_uint;
pub const BINDING_SYNC_CREATE: GBindingFlags = 1 << 1;
pub const BINDING_INVERT_BOOLEAN: GBindingFlags = 1 << 2;

pub type GdkPaintableFlags = c_uint;
pub const PAINTABLE_STATIC_SIZE: GdkPaintableFlags = 1 << 0;
pub const PAINTABLE_STATIC_CONTENTS: GdkPaintableFlags = 1 << 1;

pub type GdkMemoryFormat = c_int;
pub const MEMORY_DEFAULT: GdkMemoryFormat = 0;

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(
    GtkWidget,
    GtkSnapshot,
    GtkImage,
    GtkButton,
    GtkMenuButton,
    GtkEntry,
    GtkLayoutManager,
    GdkPaintable,
    GdkTexture,
    GBytes,
    GBinding,
    GdkPixbufFormat,
    Cairo,
    CairoPattern,
);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GdkRgba {
    pub red: c_float,
    pub green: c_float,
    pub blue: c_float,
    pub alpha: c_float,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GraphenePoint {
    pub x: c_float,
    pub y: c_float,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GrapheneSize {
    pub width: c_float,
    pub height: c_float,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GrapheneRect {
    pub origin: GraphenePoint,
    pub size: GrapheneSize,
}

impl GrapheneRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> GrapheneRect {
        GrapheneRect {
            origin: GraphenePoint { x, y },
            size: GrapheneSize { width, height },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GtkAllocation {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

#[repr(C)]
pub struct GSList {
    pub data: *mut c_void,
    pub next: *mut GSList,
}

#[repr(C)]
pub struct GTypeInstance {
    pub g_class: *mut c_void,
}

#[repr(C)]
pub struct GObject {
    pub g_type_instance: GTypeInstance,
    pub ref_count: c_uint,
    pub qdata: *mut c_void,
}

#[repr(C)]
pub struct GObjectClass {
    pub g_type: GType,
    pub construct_properties: *mut GSList,
    pub constructor: *mut c_void,
    pub set_property: *mut c_void,
    pub get_property: *mut c_void,
    pub dispose: Option<unsafe extern "C" fn(object: *mut GObject)>,
    pub finalize: Option<unsafe extern "C" fn(object: *mut GObject)>,
    pub dispatch_properties_changed: *mut c_void,
    pub notify: *mut c_void,
    pub constructed: *mut c_void,
    pub flags: usize,
    pub n_construct_properties: usize,
    pub pspecs: *mut c_void,
    pub n_pspecs: usize,
    pub pdummy: [*mut c_void; 3],
}

#[repr(C)]
pub struct GTypeInterface {
    pub g_type: GType,
    pub g_instance_type: GType,
}

#[repr(C)]
pub struct GdkPaintableInterface {
    pub g_iface: GTypeInterface,
    pub snapshot: Option<
        unsafe extern "C" fn(
            paintable: *mut GdkPaintable,
            snapshot: *mut GtkSnapshot,
            width: c_double,
            height: c_double,
        ),
    >,
    pub get_current_image:
        Option<unsafe extern "C" fn(paintable: *mut GdkPaintable) -> *mut GdkPaintable>,
    pub get_flags: Option<unsafe extern "C" fn(paintable: *mut GdkPaintable) -> GdkPaintableFlags>,
    pub get_intrinsic_width: Option<unsafe extern "C" fn(paintable: *mut GdkPaintable) -> c_int>,
    pub get_intrinsic_height: Option<unsafe extern "C" fn(paintable: *mut GdkPaintable) -> c_int>,
    pub get_intrinsic_aspect_ratio:
        Option<unsafe extern "C" fn(paintable: *mut GdkPaintable) -> c_double>,
}

#[repr(C)]
pub struct GtkSymbolicPaintableInterface {
    pub g_iface: GTypeInterface,
    pub snapshot_symbolic: Option<
        unsafe extern "C" fn(
            paintable: *mut GdkPaintable,
            snapshot: *mut GtkSnapshot,
            width: c_double,
            height: c_double,
            colors: *const GdkRgba,
            n_colors: usize,
        ),
    >,
}

pub type GClassInitFunc =
    Option<unsafe extern "C" fn(g_class: *mut c_void, class_data: *mut c_void)>;
pub type GInstanceInitFunc =
    Option<unsafe extern "C" fn(instance: *mut GTypeInstance, g_class: *mut c_void)>;
pub type GInterfaceInitFunc =
    Option<unsafe extern "C" fn(g_iface: *mut c_void, iface_data: *mut c_void)>;

#[repr(C)]
pub struct GInterfaceInfo {
    pub interface_init: GInterfaceInitFunc,
    pub interface_finalize: GInterfaceInitFunc,
    pub interface_data: *mut c_void,
}

pub type GtkCustomMeasureFunc = Option<
    unsafe extern "C" fn(
        widget: *mut GtkWidget,
        orientation: GtkOrientation,
        for_size: c_int,
        minimum: *mut c_int,
        natural: *mut c_int,
        minimum_baseline: *mut c_int,
        natural_baseline: *mut c_int,
    ),
>;
pub type GtkCustomAllocateFunc = Option<
    unsafe extern "C" fn(widget: *mut GtkWidget, width: c_int, height: c_int, baseline: c_int),
>;
pub type GtkCustomRequestModeFunc = Option<unsafe extern "C" fn(widget: *mut GtkWidget) -> c_int>;

unsafe extern "C" {
    pub fn g_object_get_type() -> GType;
    pub fn g_object_new(object_type: GType, first_property_name: *const c_char, ...)
    -> *mut c_void;
    pub fn g_object_ref(object: *mut c_void) -> *mut c_void;
    pub fn g_object_unref(object: *mut c_void);
    pub fn g_object_set_data_full(
        object: *mut c_void,
        key: *const c_char,
        data: *mut c_void,
        destroy: GDestroyNotify,
    );
    pub fn g_object_get_data(object: *mut c_void, key: *const c_char) -> *mut c_void;
    pub fn g_object_bind_property(
        source: *mut c_void,
        source_property: *const c_char,
        target: *mut c_void,
        target_property: *const c_char,
        flags: GBindingFlags,
    ) -> *mut GBinding;
    pub fn g_type_register_static_simple(
        parent_type: GType,
        type_name: *const c_char,
        class_size: c_uint,
        class_init: GClassInitFunc,
        instance_size: c_uint,
        instance_init: GInstanceInitFunc,
        flags: c_uint,
    ) -> GType;
    pub fn g_type_add_interface_static(
        instance_type: GType,
        interface_type: GType,
        info: *const GInterfaceInfo,
    );
    pub fn g_type_class_peek(type_: GType) -> *mut c_void;
    pub fn g_type_check_instance_is_a(instance: *mut c_void, iface_type: GType) -> GBoolean;

    pub fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
    pub fn g_bytes_unref(bytes: *mut GBytes);
    pub fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const c_void;
    pub fn g_resources_lookup_data(
        path: *const c_char,
        lookup_flags: c_uint,
        error: *mut *mut c_void,
    ) -> *mut GBytes;
    pub fn g_slist_free(list: *mut GSList);
    pub fn g_free(mem: *mut c_void);

    pub fn gdk_paintable_get_type() -> GType;
    pub fn gdk_memory_texture_new(
        width: c_int,
        height: c_int,
        format: GdkMemoryFormat,
        bytes: *mut GBytes,
        stride: usize,
    ) -> *mut GdkTexture;
    pub fn gdk_texture_get_width(texture: *mut GdkTexture) -> c_int;
    pub fn gdk_texture_get_height(texture: *mut GdkTexture) -> c_int;
    pub fn gdk_pixbuf_get_formats() -> *mut GSList;
    pub fn gdk_pixbuf_format_get_name(format: *mut GdkPixbufFormat) -> *mut c_char;

    pub fn gtk_symbolic_paintable_get_type() -> GType;
    pub fn gtk_header_bar_get_type() -> GType;
    pub fn gtk_image_get_type() -> GType;

    pub fn gtk_widget_measure(
        widget: *mut GtkWidget,
        orientation: GtkOrientation,
        for_size: c_int,
        minimum: *mut c_int,
        natural: *mut c_int,
        minimum_baseline: *mut c_int,
        natural_baseline: *mut c_int,
    );
    pub fn gtk_widget_get_visible(widget: *mut GtkWidget) -> GBoolean;
    pub fn gtk_widget_get_direction(widget: *mut GtkWidget) -> GtkTextDirection;
    pub fn gtk_widget_get_width(widget: *mut GtkWidget) -> c_int;
    pub fn gtk_widget_size_allocate(
        widget: *mut GtkWidget,
        allocation: *const GtkAllocation,
        baseline: c_int,
    );
    pub fn gtk_widget_get_ancestor(widget: *mut GtkWidget, widget_type: GType) -> *mut GtkWidget;
    pub fn gtk_widget_compute_point(
        widget: *mut GtkWidget,
        target: *mut GtkWidget,
        point: *const GraphenePoint,
        out_point: *mut GraphenePoint,
    ) -> GBoolean;
    pub fn gtk_widget_set_child_visible(widget: *mut GtkWidget, child_visible: GBoolean);
    pub fn gtk_widget_set_parent(widget: *mut GtkWidget, parent: *mut GtkWidget);
    pub fn gtk_widget_add_css_class(widget: *mut GtkWidget, css_class: *const c_char);
    pub fn gtk_widget_set_can_target(widget: *mut GtkWidget, can_target: GBoolean);
    pub fn gtk_widget_set_layout_manager(
        widget: *mut GtkWidget,
        layout_manager: *mut GtkLayoutManager,
    );
    pub fn gtk_widget_set_hexpand(widget: *mut GtkWidget, expand: GBoolean);
    pub fn gtk_widget_set_halign(widget: *mut GtkWidget, align: GtkAlign);
    pub fn gtk_widget_set_valign(widget: *mut GtkWidget, align: GtkAlign);
    pub fn gtk_window_controls_new(side: GtkPackType) -> *mut GtkWidget;
    pub fn gtk_label_new(text: *const c_char) -> *mut GtkWidget;
    pub fn gtk_box_new(orientation: GtkOrientation, spacing: c_int) -> *mut GtkWidget;
    pub fn gtk_custom_layout_new(
        request_mode: GtkCustomRequestModeFunc,
        measure: GtkCustomMeasureFunc,
        allocate: GtkCustomAllocateFunc,
    ) -> *mut GtkLayoutManager;

    pub fn gtk_image_new_from_paintable(paintable: *mut GdkPaintable) -> *mut GtkWidget;
    pub fn gtk_image_new_from_icon_name(icon_name: *const c_char) -> *mut GtkWidget;
    pub fn gtk_image_set_from_paintable(image: *mut GtkImage, paintable: *mut GdkPaintable);
    pub fn gtk_image_set_from_icon_name(image: *mut GtkImage, icon_name: *const c_char);
    pub fn gtk_button_new() -> *mut GtkWidget;
    pub fn gtk_button_set_icon_name(button: *mut GtkButton, icon_name: *const c_char);
    pub fn gtk_button_get_child(button: *mut GtkButton) -> *mut GtkWidget;
    pub fn gtk_button_set_child(button: *mut GtkButton, child: *mut GtkWidget);
    pub fn gtk_menu_button_set_child(button: *mut GtkMenuButton, child: *mut GtkWidget);
    pub fn gtk_menu_button_set_icon_name(button: *mut GtkMenuButton, icon_name: *const c_char);
    pub fn gtk_entry_set_icon_from_paintable(
        entry: *mut GtkEntry,
        icon_pos: GtkEntryIconPosition,
        paintable: *mut GdkPaintable,
    );
    pub fn gtk_entry_set_icon_from_icon_name(
        entry: *mut GtkEntry,
        icon_pos: GtkEntryIconPosition,
        icon_name: *const c_char,
    );
    pub fn gtk_window_set_default_icon_name(name: *const c_char);

    pub fn gtk_snapshot_append_cairo(
        snapshot: *mut GtkSnapshot,
        bounds: *const GrapheneRect,
    ) -> *mut Cairo;
    pub fn gtk_snapshot_append_texture(
        snapshot: *mut GtkSnapshot,
        texture: *mut GdkTexture,
        bounds: *const GrapheneRect,
    );
    pub fn gtk_snapshot_append_color(
        snapshot: *mut GtkSnapshot,
        color: *const GdkRgba,
        bounds: *const GrapheneRect,
    );
    pub fn gtk_snapshot_push_clip(snapshot: *mut GtkSnapshot, bounds: *const GrapheneRect);
    pub fn gtk_snapshot_pop(snapshot: *mut GtkSnapshot);

    pub fn cairo_push_group(cr: *mut Cairo);
    pub fn cairo_pop_group(cr: *mut Cairo) -> *mut CairoPattern;
    pub fn cairo_set_source_rgba(
        cr: *mut Cairo,
        red: c_double,
        green: c_double,
        blue: c_double,
        alpha: c_double,
    );
    pub fn cairo_mask(cr: *mut Cairo, pattern: *mut CairoPattern);
    pub fn cairo_pattern_destroy(pattern: *mut CairoPattern);
    pub fn cairo_destroy(cr: *mut Cairo);
}

opaque!(GdkFrameClock, CairoSurface);

pub type GtkTickCallback = Option<
    unsafe extern "C" fn(
        widget: *mut GtkWidget,
        frame_clock: *mut GdkFrameClock,
        user_data: *mut c_void,
    ) -> GBoolean,
>;

pub type CairoFormat = c_int;
pub const CAIRO_FORMAT_RGB24: CairoFormat = 1;
pub const CAIRO_STATUS_SUCCESS: c_int = 0;

unsafe extern "C" {
    pub fn gtk_widget_queue_draw(widget: *mut GtkWidget);
    pub fn gtk_widget_add_tick_callback(
        widget: *mut GtkWidget,
        callback: GtkTickCallback,
        user_data: *mut c_void,
        notify: GDestroyNotify,
    ) -> c_uint;

    pub fn cairo_save(cr: *mut Cairo);
    pub fn cairo_restore(cr: *mut Cairo);
    pub fn cairo_scale(cr: *mut Cairo, sx: c_double, sy: c_double);
    pub fn cairo_translate(cr: *mut Cairo, tx: c_double, ty: c_double);
    pub fn cairo_rectangle(
        cr: *mut Cairo,
        x: c_double,
        y: c_double,
        width: c_double,
        height: c_double,
    );
    pub fn cairo_clip(cr: *mut Cairo);
    pub fn cairo_fill(cr: *mut Cairo);
    pub fn cairo_paint(cr: *mut Cairo);
    pub fn cairo_set_source_rgb(cr: *mut Cairo, red: c_double, green: c_double, blue: c_double);
    pub fn cairo_set_source_surface(
        cr: *mut Cairo,
        surface: *mut CairoSurface,
        x: c_double,
        y: c_double,
    );
    pub fn cairo_image_surface_create(
        format: CairoFormat,
        width: c_int,
        height: c_int,
    ) -> *mut CairoSurface;
    pub fn cairo_image_surface_create_for_data(
        data: *mut u8,
        format: CairoFormat,
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> *mut CairoSurface;
    pub fn cairo_image_surface_get_data(surface: *mut CairoSurface) -> *mut u8;
    pub fn cairo_image_surface_get_stride(surface: *mut CairoSurface) -> c_int;
    pub fn cairo_image_surface_get_width(surface: *mut CairoSurface) -> c_int;
    pub fn cairo_image_surface_get_height(surface: *mut CairoSurface) -> c_int;
    pub fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
    pub fn cairo_surface_mark_dirty(surface: *mut CairoSurface);
    pub fn cairo_surface_destroy(surface: *mut CairoSurface);
}

opaque!(
    GtkGesture,
    GtkEventController,
    GtkDropTarget,
    GtkIMContext,
    GtkAdjustment,
    GtkNative,
    GdkSurface,
    GdkClipboard,
    GdkFileList,
    GValue,
);

pub type GAsyncReadyCallback =
    Option<unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, data: *mut c_void)>;
pub type GdkModifierType = c_uint;
pub const ALT_MASK: GdkModifierType = 1 << 3;
pub const META_MASK: GdkModifierType = 1 << 28;
pub const BUTTON_PRIMARY: c_uint = 1;
pub const BUTTON_MIDDLE: c_uint = 2;
pub const BUTTON_SECONDARY: c_uint = 3;
pub const EVENT_SEQUENCE_CLAIMED: c_int = 1;
pub const ACTION_COPY: c_uint = 1 << 0;

unsafe extern "C" {
    pub fn g_type_check_value_holds(value: *const GValue, type_: GType) -> GBoolean;
    pub fn g_value_get_boxed(value: *const GValue) -> *mut c_void;
    pub fn g_file_get_path(file: *mut c_void) -> *mut c_char;

    pub fn gdk_keyval_to_unicode(keyval: c_uint) -> u32;
    pub fn gdk_keyval_name(keyval: c_uint) -> *const c_char;
    pub fn gdk_keyval_to_lower(keyval: c_uint) -> c_uint;
    pub fn gdk_surface_get_scale(surface: *mut GdkSurface) -> c_double;
    pub fn gdk_clipboard_read_text_async(
        clipboard: *mut GdkClipboard,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: *mut c_void,
    );
    pub fn gdk_clipboard_read_text_finish(
        clipboard: *mut GdkClipboard,
        result: *mut c_void,
        error: *mut *mut c_void,
    ) -> *mut c_char;
    pub fn gdk_file_list_get_type() -> GType;
    pub fn gdk_file_list_get_files(file_list: *mut GdkFileList) -> *mut GSList;

    pub fn gtk_widget_get_height(widget: *mut GtkWidget) -> c_int;
    pub fn gtk_widget_get_clipboard(widget: *mut GtkWidget) -> *mut GdkClipboard;
    pub fn gtk_widget_get_native(widget: *mut GtkWidget) -> *mut GtkNative;
    pub fn gtk_widget_get_scale_factor(widget: *mut GtkWidget) -> c_int;
    pub fn gtk_native_get_surface(native: *mut GtkNative) -> *mut GdkSurface;
    pub fn gtk_adjustment_get_upper(adjustment: *mut GtkAdjustment) -> c_double;
    pub fn gtk_gesture_drag_new() -> *mut GtkGesture;
    pub fn gtk_gesture_set_state(gesture: *mut GtkGesture, state: c_int) -> GBoolean;
    pub fn gtk_gesture_single_set_button(gesture: *mut GtkGesture, button: c_uint);
    pub fn gtk_gesture_single_get_current_button(gesture: *mut GtkGesture) -> c_uint;
    pub fn gtk_event_controller_get_current_event_state(
        controller: *mut GtkEventController,
    ) -> GdkModifierType;
    pub fn gtk_event_controller_motion_new() -> *mut GtkEventController;
    pub fn gtk_event_controller_key_set_im_context(
        controller: *mut GtkEventController,
        im_context: *mut GtkIMContext,
    );
    pub fn gtk_drop_target_new(type_: GType, actions: c_uint) -> *mut GtkDropTarget;
    pub fn gtk_im_multicontext_new() -> *mut GtkIMContext;
    pub fn gtk_im_context_set_client_widget(context: *mut GtkIMContext, widget: *mut GtkWidget);
}

opaque!(GdkCursor, GdkEvent);

#[repr(C)]
pub struct GtkWidgetClass {
    pub parent_class: GObjectClass,
    pub vfuncs: [*mut c_void; 23],
    pub snapshot: Option<unsafe extern "C" fn(widget: *mut GtkWidget, snapshot: *mut GtkSnapshot)>,
    pub contains: *mut c_void,
    pub priv_: *mut c_void,
    pub padding: [*mut c_void; 8],
}

#[repr(C)]
pub struct GTypeQuery {
    pub type_: GType,
    pub type_name: *const c_char,
    pub class_size: c_uint,
    pub instance_size: c_uint,
}

pub const CAIRO_FORMAT_ARGB32: CairoFormat = 0;
pub const CAIRO_ANTIALIAS_BEST: c_int = 6;
pub const CAIRO_LINE_CAP_ROUND: c_int = 1;
pub const CAIRO_LINE_JOIN_ROUND: c_int = 1;
pub const SCROLL_UNIT_SURFACE: c_int = 1;
pub const EVENT_CONTROLLER_SCROLL_BOTH_AXES: c_uint = 1 | 2;
pub const EVENT_CONTROLLER_SCROLL_KINETIC: c_uint = 1 << 3;

unsafe extern "C" {
    pub fn g_type_query(type_: GType, query: *mut GTypeQuery);
    pub fn g_bytes_new_take(data: *mut c_void, size: usize) -> *mut GBytes;

    pub fn gdk_cursor_new_from_name(
        name: *const c_char,
        fallback: *mut GdkCursor,
    ) -> *mut GdkCursor;
    pub fn gdk_cursor_new_from_texture(
        texture: *mut GdkTexture,
        hotspot_x: c_int,
        hotspot_y: c_int,
        fallback: *mut GdkCursor,
    ) -> *mut GdkCursor;
    pub fn gdk_frame_clock_get_frame_time(frame_clock: *mut GdkFrameClock) -> i64;
    pub fn gdk_event_get_modifier_state(event: *mut GdkEvent) -> GdkModifierType;

    pub fn gtk_drawing_area_get_type() -> GType;
    pub fn gtk_widget_set_cursor(widget: *mut GtkWidget, cursor: *mut GdkCursor);
    pub fn gtk_widget_set_cursor_from_name(widget: *mut GtkWidget, name: *const c_char);
    pub fn gtk_widget_has_focus(widget: *mut GtkWidget) -> GBoolean;
    pub fn gtk_widget_contains(widget: *mut GtkWidget, x: c_double, y: c_double) -> GBoolean;
    pub fn gtk_widget_remove_tick_callback(widget: *mut GtkWidget, id: c_uint);
    pub fn gtk_adjustment_configure(
        adjustment: *mut GtkAdjustment,
        value: c_double,
        lower: c_double,
        upper: c_double,
        step_increment: c_double,
        page_increment: c_double,
        page_size: c_double,
    );
    pub fn gtk_adjustment_get_value(adjustment: *mut GtkAdjustment) -> c_double;
    pub fn gtk_adjustment_set_value(adjustment: *mut GtkAdjustment, value: c_double);
    pub fn gtk_adjustment_get_page_size(adjustment: *mut GtkAdjustment) -> c_double;
    pub fn gtk_event_controller_scroll_new(flags: c_uint) -> *mut GtkEventController;
    pub fn gtk_event_controller_scroll_get_unit(controller: *mut GtkEventController) -> c_int;
    pub fn gtk_event_controller_get_current_event(
        controller: *mut GtkEventController,
    ) -> *mut GdkEvent;
    pub fn gtk_snapshot_save(snapshot: *mut GtkSnapshot);
    pub fn gtk_snapshot_restore(snapshot: *mut GtkSnapshot);
    pub fn gtk_snapshot_translate(snapshot: *mut GtkSnapshot, point: *const GraphenePoint);

    pub fn cairo_create(target: *mut CairoSurface) -> *mut Cairo;
    pub fn cairo_set_antialias(cr: *mut Cairo, antialias: c_int);
    pub fn cairo_set_line_cap(cr: *mut Cairo, line_cap: c_int);
    pub fn cairo_set_line_join(cr: *mut Cairo, line_join: c_int);
    pub fn cairo_set_line_width(cr: *mut Cairo, width: c_double);
    pub fn cairo_move_to(cr: *mut Cairo, x: c_double, y: c_double);
    pub fn cairo_line_to(cr: *mut Cairo, x: c_double, y: c_double);
    pub fn cairo_curve_to(
        cr: *mut Cairo,
        x1: c_double,
        y1: c_double,
        x2: c_double,
        y2: c_double,
        x3: c_double,
        y3: c_double,
    );
    pub fn cairo_close_path(cr: *mut Cairo);
    pub fn cairo_stroke(cr: *mut Cairo);
    pub fn cairo_set_source(cr: *mut Cairo, source: *mut CairoPattern);
    pub fn cairo_pattern_create_linear(
        x0: c_double,
        y0: c_double,
        x1: c_double,
        y1: c_double,
    ) -> *mut CairoPattern;
    pub fn cairo_pattern_add_color_stop_rgba(
        pattern: *mut CairoPattern,
        offset: c_double,
        red: c_double,
        green: c_double,
        blue: c_double,
        alpha: c_double,
    );
    pub fn cairo_surface_flush(surface: *mut CairoSurface);
}
