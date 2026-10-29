//! Southstar — the form helpers a frame-targeted submission calls: the urlencoded form data set and the page's CSP form-action check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_js_runtime::api;

use super::Js;

pub(crate) fn urlencoded(
    form: Node<'_>,
    doc: Node<'_>,
    submitter: Option<Node<'_>>,
    charset: Option<&CStr>,
) -> Vec<u8> {
    southstar_net::set_submission_charset(charset.map(CStr::to_bytes));
    let bytes = southstar_forms::urlencoded_inputs(form, doc, submitter);
    southstar_net::set_submission_charset(None);
    bytes
}

pub(crate) fn csp_form_action_allowed(js: Js, form: Node<'_>, url: &CStr) -> bool {
    api::csp_form_action_allowed(js.ptr(), Some(form), Some(url))
}
