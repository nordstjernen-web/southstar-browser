//! Southstar — the one static library meson links into the C targets, gathering every ported module.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub use southstar_about_style;
pub use southstar_bookmarks;
pub use southstar_bytecode_cache;
pub use southstar_config;
pub use southstar_csp;
pub use southstar_css_syntax;
pub use southstar_datetime;
pub use southstar_debuglog;
pub use southstar_glib;
pub use southstar_history;
pub use southstar_i18n;
pub use southstar_safebrowsing;
pub use southstar_spellcheck;
#[cfg(feature = "woff2")]
pub use southstar_woff2;
