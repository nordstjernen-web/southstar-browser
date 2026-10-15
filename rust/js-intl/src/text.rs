//! Southstar — Intl.ListFormat, RelativeTimeFormat, DisplayNames, DurationFormat and Segmenter.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod display;
mod duration;
mod list;
mod relative;
mod segmenter;
mod service;

use southstar_js_engine::{Scope, Value};

pub(crate) use display::{METHODS as DISPLAY_METHODS, constructor as display_constructor};
pub(crate) use duration::{METHODS as DURATION_METHODS, constructor as duration_constructor};
pub(crate) use list::{METHODS as LIST_METHODS, constructor as list_constructor};
pub(crate) use relative::{METHODS as RELATIVE_METHODS, constructor as relative_constructor};
pub(crate) use segmenter::{METHODS as SEGMENTER_METHODS, constructor as segmenter_constructor};

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value) {
    service::install_supported_locales(scope, intl, "ListFormat", list::supported_locales_of);
    service::install_supported_locales(
        scope,
        intl,
        "RelativeTimeFormat",
        relative::supported_locales_of,
    );
    service::install_supported_locales(scope, intl, "DisplayNames", display::supported_locales_of);
    service::install_supported_locales(
        scope,
        intl,
        "DurationFormat",
        duration::supported_locales_of,
    );
    service::install_supported_locales(scope, intl, "Segmenter", segmenter::supported_locales_of);
    let _ = segmenter::install(scope, intl);
}
