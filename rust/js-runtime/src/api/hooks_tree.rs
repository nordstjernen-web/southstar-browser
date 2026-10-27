//! Southstar — the typed tables through which the page record reaches the binding crates above it that own frames, change records, scripts, listeners, focus, the top layer, attributes, iterators, observers and the document.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::{NsJs, ResourceInfo};

macro_rules! hook_table {
    ($table:ident, $slot:ident, $set:ident, $get:ident) => {
        static $slot: std::sync::OnceLock<$table> = std::sync::OnceLock::new();

        pub fn $set(hooks: $table) {
            let _ = $slot.set(hooks);
        }

        pub fn $get() -> &'static $table {
            $slot.get().unwrap_or(&$table::NONE)
        }
    };
}

pub(crate) use hook_table;

pub struct FramesHooks {
    pub submit_form: fn(*const NsJs, Node<'_>, Option<Node<'_>>) -> bool,
    pub purge_subtree_pending: fn(*const NsJs, Node<'_>),
    pub pending_count: fn(*const NsJs) -> u32,
    pub pending_first: fn(*const NsJs) -> Option<Node<'static>>,
    pub pending_remove_first: fn(*const NsJs),
    pub load_now: fn(*const NsJs, Node<'_>),
    pub clear_initial_blank: fn(*const NsJs),
    pub teardown: fn(*const NsJs),
    pub schedule_load: fn(*const NsJs, Node<'_>, bool),
    pub scrub_globals: fn(*const NsJs, Node<'_>),
    pub promote_deferred: fn(*const NsJs),
    pub frame_context: fn(*const NsJs, Node<'_>) -> ContextHandle,
    pub frame_url: fn(*const NsJs, Option<Node<'_>>) -> Option<CString>,
    pub document_node: fn(Node<'_>) -> Option<Node<'_>>,
    pub frame_of_realm: fn(*const NsJs, ContextHandle) -> Option<Node<'static>>,
    pub node_realm_context: fn(*const NsJs, Node<'_>) -> ContextHandle,
    pub cloners_made: fn(*const NsJs) -> bool,
    pub forget_node: fn(*const NsJs, Node<'_>),
    pub realms_same_origin: fn(*const NsJs, ContextHandle, ContextHandle) -> bool,
    pub schedule_static_iframes: fn(*const NsJs, Node<'_>),
    pub realm_proto_for: fn(*const NsJs, ContextHandle, &Value) -> Value,
    pub frame_realm_window: fn(*const NsJs, Node<'_>) -> Value,
}

impl FramesHooks {
    const NONE: FramesHooks = FramesHooks {
        submit_form: |_, _, _| false,
        purge_subtree_pending: |_, _| {},
        pending_count: |_| 0,
        pending_first: |_| None,
        pending_remove_first: |_| {},
        load_now: |_, _| {},
        clear_initial_blank: |_| {},
        teardown: |_| {},
        schedule_load: |_, _, _| {},
        scrub_globals: |_, _| {},
        promote_deferred: |_| {},
        frame_context: |_, _| ContextHandle::NULL,
        frame_url: |_, _| None,
        document_node: |_| None,
        frame_of_realm: |_, _| None,
        node_realm_context: |_, _| ContextHandle::NULL,
        cloners_made: |_| false,
        forget_node: |_, _| {},
        realms_same_origin: |_, _, _| true,
        schedule_static_iframes: |_, _| {},
        realm_proto_for: |_, _, proto| proto.clone(),
        frame_realm_window: |_, _| Value::null(),
    };
}

hook_table!(FramesHooks, FRAMES, set_frames_hooks, frames_hooks);

pub struct MutationHooks {
    pub init: fn(*const NsJs),
    pub teardown: fn(*const NsJs),
    pub orphan_count: fn(*const NsJs) -> usize,
    pub collect_orphans: fn(*const NsJs) -> Vec<Node<'static>>,
    pub orphans_snapshot: fn(*const NsJs) -> Option<Vec<Node<'static>>>,
    pub orphan_tracked: fn(*const NsJs, Node<'_>) -> bool,
    pub track_orphan: fn(*const NsJs, Node<'_>),
    pub unorphan_node: fn(*const NsJs, Node<'_>),
    pub purge_subtree_node_refs: fn(*const NsJs, Node<'_>),
    pub forget_pending_change: fn(*const NsJs),
    pub forget_pending_change_for: fn(*const NsJs, Node<'_>),
    pub attribute_map_release_owner: fn(*const NsJs, Node<'_>),
    pub attribute_maps_teardown: fn(*const NsJs),
    pub set_attr_recorded: fn(*const NsJs, Node<'_>, &CStr, &[u8]),
    pub set_attr_recorded_paint_only: fn(*const NsJs, Node<'_>, &CStr, &CStr),
    pub index_child_removed: fn(*const NsJs, Option<Node<'_>>, Node<'_>),
    pub record_child_added: fn(*const NsJs, Node<'_>, Node<'_>, Option<Node<'_>>),
}

impl MutationHooks {
    const NONE: MutationHooks = MutationHooks {
        init: |_| {},
        teardown: |_| {},
        orphan_count: |_| 0,
        collect_orphans: |_| Vec::new(),
        orphans_snapshot: |_| None,
        orphan_tracked: |_, _| false,
        track_orphan: |_, _| {},
        unorphan_node: |_, _| {},
        purge_subtree_node_refs: |_, _| {},
        forget_pending_change: |_| {},
        forget_pending_change_for: |_, _| {},
        attribute_map_release_owner: |_, _| {},
        attribute_maps_teardown: |_| {},
        set_attr_recorded: |_, _, _, _| {},
        set_attr_recorded_paint_only: |_, _, _, _| {},
        index_child_removed: |_, _, _| {},
        record_child_added: |_, _, _, _| {},
    };
}

hook_table!(MutationHooks, MUTATION, set_mutation_hooks, mutation_hooks);

pub struct LoaderHooks {
    pub eval: fn(*const NsJs, &[u8], Option<&CStr>),
    pub script_needs_prepare: fn(*const NsJs, Node<'_>),
    pub schedule_pending_script_drain: fn(*const NsJs),
    pub purge_subtree_script_refs: fn(*const NsJs, Node<'_>),
    pub parser_hold_forget: fn(*const NsJs, Node<'_>),
    pub teardown: fn(*const NsJs),
    pub element_perf_info: fn(*const NsJs, Option<Node<'_>>) -> ResourceInfo,
}

impl LoaderHooks {
    const NONE: LoaderHooks = LoaderHooks {
        eval: |_, _, _| {},
        script_needs_prepare: |_, _| {},
        schedule_pending_script_drain: |_| {},
        purge_subtree_script_refs: |_, _| {},
        parser_hold_forget: |_, _| {},
        teardown: |_| {},
        element_perf_info: |_, _| ResourceInfo::default(),
    };
}

hook_table!(LoaderHooks, LOADER, set_loader_hooks, loader_hooks);

pub struct DispatchHooks {
    pub report_pending_rejections: fn(*const NsJs),
    pub drop_pending_rejections: fn(*const NsJs),
    pub forget_node: fn(*const NsJs, Node<'_>),
    pub teardown_listeners: fn(*const NsJs),
    pub teardown: fn(*const NsJs),
}

impl DispatchHooks {
    const NONE: DispatchHooks = DispatchHooks {
        report_pending_rejections: |_| {},
        drop_pending_rejections: |_| {},
        forget_node: |_, _| {},
        teardown_listeners: |_| {},
        teardown: |_| {},
    };
}

hook_table!(DispatchHooks, DISPATCH, set_dispatch_hooks, dispatch_hooks);

pub struct FocusHooks {
    pub focused_node: fn(*const NsJs) -> Option<Node<'static>>,
    pub update_focus_visible: fn(*const NsJs),
    pub forget_node: fn(*const NsJs, Node<'_>),
    pub teardown: fn(*const NsJs),
    pub forget_subtree: fn(*const NsJs, Node<'_>),
}

impl FocusHooks {
    const NONE: FocusHooks = FocusHooks {
        focused_node: |_| None,
        update_focus_visible: |_| {},
        forget_node: |_, _| {},
        teardown: |_| {},
        forget_subtree: |_, _| {},
    };
}

hook_table!(FocusHooks, FOCUS, set_focus_hooks, focus_hooks);

pub struct TopLayerHooks {
    pub popover_attr_changed: fn(*const NsJs, Node<'_>, &CStr, Option<&CStr>, Option<&CStr>),
    pub forget_node: fn(*const NsJs, Node<'_>),
    pub clear: fn(*const NsJs),
    pub popover_removing: fn(*const NsJs, Node<'_>),
    pub dialog_close: fn(*const NsJs, Node<'_>, Option<&CStr>),
    pub activate_summary: fn(*const NsJs, Node<'_>) -> bool,
    pub summary_toggle_target: fn(Node<'_>) -> Option<Node<'static>>,
    pub is_button: fn(Node<'_>) -> bool,
    pub button_activation: fn(*const NsJs, Node<'_>, Option<Node<'_>>),
    pub popover_target_activation: fn(*const NsJs, Node<'_>, Option<Node<'_>>),
}

impl TopLayerHooks {
    const NONE: TopLayerHooks = TopLayerHooks {
        popover_attr_changed: |_, _, _, _, _| {},
        forget_node: |_, _| {},
        clear: |_| {},
        popover_removing: |_, _| {},
        dialog_close: |_, _, _| {},
        activate_summary: |_, _| false,
        summary_toggle_target: |_| None,
        is_button: |_| false,
        button_activation: |_, _, _| {},
        popover_target_activation: |_, _, _| {},
    };
}

hook_table!(
    TopLayerHooks,
    TOP_LAYER,
    set_top_layer_hooks,
    top_layer_hooks
);

pub struct AttrsHooks {
    pub class_id: fn() -> u32,
    pub detach_owner: fn(*const NsJs, Node<'_>),
    pub detach_all: fn(*const NsJs),
    pub detach_matching: fn(*const NsJs, Node<'_>, Option<&CStr>, &CStr),
}

impl AttrsHooks {
    const NONE: AttrsHooks = AttrsHooks {
        class_id: || 0,
        detach_owner: |_, _| {},
        detach_all: |_| {},
        detach_matching: |_, _, _, _| {},
    };
}

hook_table!(AttrsHooks, ATTRS, set_attrs_hooks, attrs_hooks);

pub struct TraversalHooks {
    pub pre_remove: fn(*const NsJs, Node<'_>),
    pub teardown: fn(*const NsJs),
}

impl TraversalHooks {
    const NONE: TraversalHooks = TraversalHooks {
        pre_remove: |_, _| {},
        teardown: |_| {},
    };
}

hook_table!(
    TraversalHooks,
    TRAVERSAL,
    set_traversal_hooks,
    traversal_hooks
);

pub struct ObserversHooks {
    pub intersection_tick: fn(*const NsJs),
    pub resize_tick: fn(*const NsJs),
    pub scrub_node: fn(*const NsJs, Node<'_>),
    pub teardown: fn(*const NsJs),
}

impl ObserversHooks {
    const NONE: ObserversHooks = ObserversHooks {
        intersection_tick: |_| {},
        resize_tick: |_| {},
        scrub_node: |_, _| {},
        teardown: |_| {},
    };
}

hook_table!(
    ObserversHooks,
    OBSERVERS,
    set_observers_hooks,
    observers_hooks
);

pub struct DocumentHooks {
    pub base_url: fn(*const NsJs) -> Option<Vec<u8>>,
    pub flush_write: fn(*const NsJs),
    pub write_teardown: fn(*const NsJs),
    pub node_base_url: fn(*const NsJs, Node<'_>) -> Option<Vec<u8>>,
    pub make_realm_document: MakeRealmDocumentFn,
}

impl DocumentHooks {
    const NONE: DocumentHooks = DocumentHooks {
        base_url: |_| None,
        flush_write: |_| {},
        write_teardown: |_| {},
        node_base_url: |_, _| None,
        make_realm_document: |_, _, _, _, _, _, _| Value::null(),
    };
}

hook_table!(DocumentHooks, DOCUMENT, set_document_hooks, document_hooks);

pub struct InterfacesHooks {
    pub install_pdf_plugins: fn(ContextHandle),
    pub link_interface_ctors: fn(ContextHandle),
    pub lock_global_prototypes: fn(ContextHandle),
}

impl InterfacesHooks {
    const NONE: InterfacesHooks = InterfacesHooks {
        install_pdf_plugins: |_| {},
        link_interface_ctors: |_| {},
        lock_global_prototypes: |_| {},
    };
}

hook_table!(
    InterfacesHooks,
    INTERFACES,
    set_interfaces_hooks,
    interfaces_hooks
);

pub type MakeRealmDocumentFn =
    fn(&mut Scope<'_>, Node<'_>, Option<&[u8]>, Option<&[u8]>, Option<&[u8]>, bool, bool) -> Value;

pub struct DomFactoryHooks {
    pub anchor_resolved_href: fn(*const NsJs, Node<'_>) -> Option<Vec<u8>>,
}

impl DomFactoryHooks {
    const NONE: DomFactoryHooks = DomFactoryHooks {
        anchor_resolved_href: |_, _| None,
    };
}

hook_table!(
    DomFactoryHooks,
    DOM_FACTORY,
    set_dom_factory_hooks,
    dom_factory_hooks
);

pub struct ElementsHooks {
    pub build_labels: fn(&mut Scope<'_>, Node<'_>) -> Result<Value, Value>,
}

impl ElementsHooks {
    const NONE: ElementsHooks = ElementsHooks {
        build_labels: |_, _| Ok(Value::undefined()),
    };
}

hook_table!(ElementsHooks, ELEMENTS, set_elements_hooks, elements_hooks);

pub struct FormsHooks {
    pub reset_form: fn(&mut Scope<'_>, Node<'_>),
    pub listed_controls: fn(Node<'_>, bool) -> Vec<Node<'static>>,
}

impl FormsHooks {
    const NONE: FormsHooks = FormsHooks {
        reset_form: |_, _| {},
        listed_controls: |_, _| Vec::new(),
    };
}

hook_table!(FormsHooks, FORMS, set_forms_hooks, forms_hooks);
