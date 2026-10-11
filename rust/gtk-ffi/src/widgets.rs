//! Southstar — the GTK, GIO and GLib calls the browser window makes: widgets, actions, menus, signals and timers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_double, c_int, c_uint, c_ulong, c_void};

use crate::{GBoolean, GType, GdkRgba, GtkWidget};

pub type GCallback = unsafe extern "C" fn();
pub type GSourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;
pub type GThreadFunc = unsafe extern "C" fn(data: *mut c_void) -> *mut c_void;
pub type GClosureNotify = Option<unsafe extern "C" fn(data: *mut c_void, closure: *mut c_void)>;

pub const SOURCE_REMOVE: GBoolean = 0;
pub const SOURCE_CONTINUE: GBoolean = 1;
pub const CONNECT_AFTER: c_uint = 1;

pub const ALIGN_START: c_int = 1;
pub const ALIGN_END: c_int = 2;
pub const ELLIPSIZE_MIDDLE: c_int = 2;
pub const ELLIPSIZE_END: c_int = 3;
pub const SELECTION_NONE: c_int = 0;
pub const SELECTION_SINGLE: c_int = 1;
pub const WRAP_NONE: c_int = 0;
pub const PHASE_CAPTURE: c_int = 1;
pub const POLICY_AUTOMATIC: c_int = 1;
pub const POLICY_NEVER: c_int = 2;
pub const POS_BOTTOM: c_int = 3;
pub const ENTRY_ICON_PRIMARY: c_int = 0;
pub const ENTRY_ICON_SECONDARY: c_int = 1;
pub const ACCESSIBLE_PROPERTY_LABEL: c_int = 4;
pub const STATE_FLAG_FOCUS_WITHIN: c_uint = 1 << 14;
pub const STYLE_PROVIDER_PRIORITY_APPLICATION: c_uint = 600;
pub const USER_DIRECTORY_DOWNLOAD: c_int = 2;
pub const APPLICATION_NON_UNIQUE: c_uint = 1 << 5;
pub const FILE_TEST_IS_REGULAR: c_uint = 1 << 0;
pub const FILE_TEST_IS_SYMLINK: c_uint = 1 << 1;
pub const FILE_TEST_IS_DIR: c_uint = 1 << 2;
pub const FILE_TEST_EXISTS: c_uint = 1 << 4;
pub const SHIFT_MASK: c_uint = 1 << 0;
pub const CONTROL_MASK: c_uint = 1 << 2;
pub const KEY_RETURN: c_uint = 0xff0d;
pub const KEY_KP_ENTER: c_uint = 0xff8d;
pub const KEY_ESCAPE: c_uint = 0xff1b;
pub const KEY_F11: c_uint = 0xffc8;

#[repr(C)]
pub struct GByteArray {
    pub data: *mut u8,
    pub len: c_uint,
}

unsafe extern "C" {
    pub fn g_signal_connect_data(
        instance: *mut c_void,
        detailed_signal: *const c_char,
        c_handler: GCallback,
        data: *mut c_void,
        destroy_data: GClosureNotify,
        connect_flags: c_uint,
    ) -> c_ulong;
    pub fn g_object_set_data(object: *mut c_void, key: *const c_char, data: *mut c_void);
    pub fn g_signal_handler_disconnect(instance: *mut c_void, handler_id: c_ulong);
    pub fn g_timeout_add(interval: c_uint, function: GSourceFunc, data: *mut c_void) -> c_uint;
    pub fn g_timeout_add_seconds(
        interval: c_uint,
        function: GSourceFunc,
        data: *mut c_void,
    ) -> c_uint;
    pub fn g_idle_add(function: GSourceFunc, data: *mut c_void) -> c_uint;
    pub fn g_source_remove(tag: c_uint) -> GBoolean;
    pub fn g_thread_new(name: *const c_char, func: GThreadFunc, data: *mut c_void) -> *mut c_void;
    pub fn g_thread_unref(thread: *mut c_void);

    pub fn g_get_user_special_dir(directory: c_int) -> *const c_char;
    pub fn g_get_home_dir() -> *const c_char;
    pub fn g_get_user_cache_dir() -> *const c_char;
    pub fn g_get_monotonic_time() -> i64;
    pub fn g_get_real_time() -> i64;
    pub fn g_get_num_processors() -> c_uint;
    pub fn g_filename_to_uri(
        filename: *const c_char,
        hostname: *const c_char,
        error: *mut *mut c_void,
    ) -> *mut c_char;
    pub fn g_format_size(size: u64) -> *mut c_char;
    pub fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut c_void,
    ) -> GBoolean;
    pub fn g_file_get_contents(
        filename: *const c_char,
        contents: *mut *mut c_char,
        length: *mut usize,
        error: *mut *mut c_void,
    ) -> GBoolean;
    pub fn g_file_test(filename: *const c_char, test: c_uint) -> GBoolean;
    pub fn g_dir_open(path: *const c_char, flags: c_uint, error: *mut *mut c_void) -> *mut c_void;
    pub fn g_dir_read_name(dir: *mut c_void) -> *const c_char;
    pub fn g_dir_close(dir: *mut c_void);
    pub fn g_path_get_basename(file_name: *const c_char) -> *mut c_char;
    pub fn g_path_get_dirname(file_name: *const c_char) -> *mut c_char;
    pub fn g_build_filenamev(args: *mut *mut c_char) -> *mut c_char;
    pub fn g_unlink(filename: *const c_char) -> c_int;
    pub fn g_rmdir(filename: *const c_char) -> c_int;
    pub fn g_uri_unescape_string(
        escaped_string: *const c_char,
        illegal_characters: *const c_char,
    ) -> *mut c_char;
    pub fn g_markup_escape_text(text: *const c_char, length: isize) -> *mut c_char;
    pub fn g_utf8_make_valid(str: *const c_char, len: isize) -> *mut c_char;
    pub fn g_strdup(str: *const c_char) -> *mut c_char;
    pub fn g_getenv(variable: *const c_char) -> *const c_char;
    pub fn g_setenv(variable: *const c_char, value: *const c_char, overwrite: GBoolean)
    -> GBoolean;
    pub fn g_app_info_launch_default_for_uri(
        uri: *const c_char,
        context: *mut c_void,
        error: *mut *mut c_void,
    ) -> GBoolean;
    pub fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;

    pub fn g_simple_action_new(name: *const c_char, parameter_type: *const c_void) -> *mut c_void;
    pub fn g_simple_action_set_enabled(action: *mut c_void, enabled: GBoolean);
    pub fn g_action_map_add_action(map: *mut c_void, action: *mut c_void);
    pub fn g_action_map_lookup_action(map: *mut c_void, name: *const c_char) -> *mut c_void;
    pub fn g_action_group_activate_action(
        group: *mut c_void,
        action_name: *const c_char,
        parameter: *mut c_void,
    );
    pub fn g_menu_new() -> *mut c_void;
    pub fn g_menu_item_new(label: *const c_char, detailed_action: *const c_char) -> *mut c_void;
    pub fn g_menu_item_set_attribute_value(
        item: *mut c_void,
        attribute: *const c_char,
        value: *mut c_void,
    );
    pub fn g_variant_new_string(string: *const c_char) -> *mut c_void;
    pub fn g_menu_append_item(menu: *mut c_void, item: *mut c_void);
    pub fn g_menu_append(menu: *mut c_void, label: *const c_char, detailed_action: *const c_char);
    pub fn g_menu_append_section(menu: *mut c_void, label: *const c_char, section: *mut c_void);
    pub fn g_application_run(
        application: *mut c_void,
        argc: c_int,
        argv: *mut *mut c_char,
    ) -> c_int;

    pub fn gdk_display_get_default() -> *mut c_void;
    pub fn gtk_icon_theme_get_for_display(display: *mut c_void) -> *mut c_void;
    pub fn gtk_icon_theme_add_resource_path(theme: *mut c_void, path: *const c_char);
    pub fn gtk_icon_theme_add_search_path(theme: *mut c_void, path: *const c_char);
    pub fn gtk_css_provider_new() -> *mut c_void;
    pub fn gtk_css_provider_load_from_string(provider: *mut c_void, string: *const c_char);
    pub fn gtk_style_context_add_provider_for_display(
        display: *mut c_void,
        provider: *mut c_void,
        priority: c_uint,
    );
    pub fn gtk_settings_get_default() -> *mut c_void;

    pub fn gtk_application_new(application_id: *const c_char, flags: c_uint) -> *mut c_void;
    pub fn gtk_application_set_accels_for_action(
        application: *mut c_void,
        detailed_action_name: *const c_char,
        accels: *const *const c_char,
    );
    pub fn gtk_application_window_new(application: *mut c_void) -> *mut GtkWidget;
    pub fn gtk_window_new() -> *mut GtkWidget;
    pub fn gtk_window_set_title(window: *mut GtkWidget, title: *const c_char);
    pub fn gtk_window_set_default_size(window: *mut GtkWidget, width: c_int, height: c_int);
    pub fn gtk_window_set_transient_for(window: *mut GtkWidget, parent: *mut GtkWidget);
    pub fn gtk_window_set_child(window: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_window_set_titlebar(window: *mut GtkWidget, titlebar: *mut GtkWidget);
    pub fn gtk_window_present(window: *mut GtkWidget);
    pub fn gtk_window_close(window: *mut GtkWidget);
    pub fn gtk_window_destroy(window: *mut GtkWidget);
    pub fn gtk_window_maximize(window: *mut GtkWidget);
    pub fn gtk_window_fullscreen(window: *mut GtkWidget);
    pub fn gtk_window_unfullscreen(window: *mut GtkWidget);
    pub fn gtk_window_is_fullscreen(window: *mut GtkWidget) -> GBoolean;
    pub fn gtk_window_get_focus(window: *mut GtkWidget) -> *mut GtkWidget;

    pub fn gtk_widget_set_visible(widget: *mut GtkWidget, visible: GBoolean);
    pub fn gtk_widget_set_sensitive(widget: *mut GtkWidget, sensitive: GBoolean);
    pub fn gtk_widget_set_tooltip_text(widget: *mut GtkWidget, text: *const c_char);
    pub fn gtk_widget_remove_css_class(widget: *mut GtkWidget, css_class: *const c_char);
    pub fn gtk_widget_set_vexpand(widget: *mut GtkWidget, expand: GBoolean);
    pub fn gtk_widget_set_margin_start(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_margin_end(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_margin_top(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_margin_bottom(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_size_request(widget: *mut GtkWidget, width: c_int, height: c_int);
    pub fn gtk_widget_get_first_child(widget: *mut GtkWidget) -> *mut GtkWidget;
    pub fn gtk_widget_get_next_sibling(widget: *mut GtkWidget) -> *mut GtkWidget;
    pub fn gtk_widget_get_parent(widget: *mut GtkWidget) -> *mut GtkWidget;
    pub fn gtk_widget_unparent(widget: *mut GtkWidget);
    pub fn gtk_widget_grab_focus(widget: *mut GtkWidget) -> GBoolean;
    pub fn gtk_widget_is_ancestor(widget: *mut GtkWidget, ancestor: *mut GtkWidget) -> GBoolean;
    pub fn gtk_widget_get_state_flags(widget: *mut GtkWidget) -> c_uint;
    pub fn gtk_widget_get_color(widget: *mut GtkWidget, color: *mut GdkRgba);
    pub fn gtk_widget_add_controller(widget: *mut GtkWidget, controller: *mut c_void);
    pub fn gtk_accessible_update_property(accessible: *mut GtkWidget, first_property: c_int, ...);

    pub fn gtk_event_controller_key_new() -> *mut c_void;
    pub fn gtk_event_controller_focus_new() -> *mut c_void;
    pub fn gtk_gesture_click_new() -> *mut c_void;
    pub fn gtk_event_controller_set_propagation_phase(controller: *mut c_void, phase: c_int);

    pub fn gtk_label_set_text(label: *mut GtkWidget, str: *const c_char);
    pub fn gtk_label_set_markup(label: *mut GtkWidget, str: *const c_char);
    pub fn gtk_label_set_xalign(label: *mut GtkWidget, xalign: f32);
    pub fn gtk_label_set_ellipsize(label: *mut GtkWidget, mode: c_int);
    pub fn gtk_label_set_width_chars(label: *mut GtkWidget, n_chars: c_int);
    pub fn gtk_label_set_max_width_chars(label: *mut GtkWidget, n_chars: c_int);
    pub fn gtk_button_new_with_label(label: *const c_char) -> *mut GtkWidget;
    pub fn gtk_button_new_from_icon_name(icon_name: *const c_char) -> *mut GtkWidget;
    pub fn gtk_button_set_label(button: *mut GtkWidget, label: *const c_char);
    pub fn gtk_button_set_has_frame(button: *mut GtkWidget, has_frame: GBoolean);
    pub fn gtk_box_append(box_: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_box_remove(box_: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_progress_bar_new() -> *mut GtkWidget;
    pub fn gtk_progress_bar_pulse(pbar: *mut GtkWidget);
    pub fn gtk_progress_bar_set_fraction(pbar: *mut GtkWidget, fraction: c_double);
    pub fn gtk_scrolled_window_new() -> *mut GtkWidget;
    pub fn gtk_scrolled_window_set_child(scrolled: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_scrolled_window_set_policy(scrolled: *mut GtkWidget, h: c_int, v: c_int);
    pub fn gtk_scrolled_window_set_propagate_natural_height(
        scrolled: *mut GtkWidget,
        propagate: GBoolean,
    );
    pub fn gtk_scrolled_window_set_max_content_height(scrolled: *mut GtkWidget, height: c_int);
    pub fn gtk_list_box_new() -> *mut GtkWidget;
    pub fn gtk_list_box_set_selection_mode(list: *mut GtkWidget, mode: c_int);
    pub fn gtk_list_box_append(list: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_list_box_prepend(list: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_list_box_remove(list: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_list_box_get_selected_row(list: *mut GtkWidget) -> *mut GtkWidget;
    pub fn gtk_list_box_select_row(list: *mut GtkWidget, row: *mut GtkWidget);
    pub fn gtk_list_box_row_new() -> *mut GtkWidget;
    pub fn gtk_list_box_row_set_child(row: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_text_view_new() -> *mut GtkWidget;
    pub fn gtk_text_view_set_editable(view: *mut GtkWidget, setting: GBoolean);
    pub fn gtk_text_view_set_cursor_visible(view: *mut GtkWidget, setting: GBoolean);
    pub fn gtk_text_view_set_monospace(view: *mut GtkWidget, monospace: GBoolean);
    pub fn gtk_text_view_set_wrap_mode(view: *mut GtkWidget, wrap_mode: c_int);
    pub fn gtk_text_view_get_buffer(view: *mut GtkWidget) -> *mut c_void;
    pub fn gtk_text_buffer_set_text(buffer: *mut c_void, text: *const c_char, len: c_int);
    pub fn gtk_separator_new(orientation: c_int) -> *mut GtkWidget;
    pub fn gtk_spinner_new() -> *mut GtkWidget;
    pub fn gtk_spinner_set_spinning(spinner: *mut GtkWidget, spinning: GBoolean);
    pub fn gtk_image_set_pixel_size(image: *mut GtkWidget, pixel_size: c_int);
    pub fn gtk_popover_new() -> *mut GtkWidget;
    pub fn gtk_popover_set_child(popover: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_popover_set_position(popover: *mut GtkWidget, position: c_int);
    pub fn gtk_popover_popup(popover: *mut GtkWidget);
    pub fn gtk_popover_popdown(popover: *mut GtkWidget);
    pub fn gtk_popover_get_type() -> GType;
    pub fn gtk_menu_button_new() -> *mut GtkWidget;
    pub fn gtk_menu_button_set_menu_model(button: *mut GtkWidget, model: *mut c_void);
    pub fn gtk_menu_button_get_popover(button: *mut GtkWidget) -> *mut GtkWidget;
    pub fn gtk_header_bar_new() -> *mut GtkWidget;
    pub fn gtk_header_bar_set_show_title_buttons(bar: *mut GtkWidget, setting: GBoolean);
    pub fn gtk_header_bar_set_title_widget(bar: *mut GtkWidget, title_widget: *mut GtkWidget);
    pub fn gtk_entry_new() -> *mut GtkWidget;
    pub fn gtk_entry_set_icon_tooltip_text(
        entry: *mut GtkWidget,
        pos: c_int,
        tooltip: *const c_char,
    );
    pub fn gtk_entry_set_icon_activatable(entry: *mut GtkWidget, pos: c_int, activatable: GBoolean);
    pub fn gtk_entry_set_placeholder_text(entry: *mut GtkWidget, text: *const c_char);
    pub fn gtk_editable_get_text(editable: *mut GtkWidget) -> *const c_char;
    pub fn gtk_editable_set_text(editable: *mut GtkWidget, text: *const c_char);
    pub fn gtk_editable_select_region(editable: *mut GtkWidget, start_pos: c_int, end_pos: c_int);
    pub fn gtk_editable_get_selection_bounds(
        editable: *mut GtkWidget,
        start_pos: *mut c_int,
        end_pos: *mut c_int,
    ) -> GBoolean;
    pub fn gtk_editable_get_type() -> GType;
    pub fn gtk_notebook_new() -> *mut GtkWidget;
    pub fn gtk_notebook_set_show_tabs(notebook: *mut GtkWidget, show_tabs: GBoolean);
    pub fn gtk_notebook_set_show_border(notebook: *mut GtkWidget, show_border: GBoolean);
    pub fn gtk_notebook_set_scrollable(notebook: *mut GtkWidget, scrollable: GBoolean);
    pub fn gtk_notebook_get_current_page(notebook: *mut GtkWidget) -> c_int;
    pub fn gtk_notebook_set_current_page(notebook: *mut GtkWidget, page_num: c_int);
    pub fn gtk_notebook_get_nth_page(notebook: *mut GtkWidget, page_num: c_int) -> *mut GtkWidget;
    pub fn gtk_notebook_get_n_pages(notebook: *mut GtkWidget) -> c_int;
    pub fn gtk_notebook_page_num(notebook: *mut GtkWidget, child: *mut GtkWidget) -> c_int;
    pub fn gtk_notebook_append_page(
        notebook: *mut GtkWidget,
        child: *mut GtkWidget,
        tab_label: *mut GtkWidget,
    ) -> c_int;
    pub fn gtk_notebook_remove_page(notebook: *mut GtkWidget, page_num: c_int);
    pub fn gtk_overlay_new() -> *mut GtkWidget;
    pub fn gtk_overlay_set_child(overlay: *mut GtkWidget, child: *mut GtkWidget);
    pub fn gtk_overlay_add_overlay(overlay: *mut GtkWidget, widget: *mut GtkWidget);
}
