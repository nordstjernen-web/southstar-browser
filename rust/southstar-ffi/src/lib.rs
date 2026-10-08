//! Southstar — the one static library meson links into the C targets, gathering every ported module.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod info;

pub use southstar_about_style;
pub use southstar_bookmarks;
pub use southstar_bytecode_cache;
pub use southstar_config;
pub use southstar_csp;
pub use southstar_css_media;
pub use southstar_css_prop_syntax;
pub use southstar_css_syntax;
pub use southstar_datetime;
pub use southstar_debuglog;
pub use southstar_eventsource;
pub use southstar_glctx;
pub use southstar_glib;
pub use southstar_history;
pub use southstar_http_cache;
pub use southstar_i18n;
pub use southstar_image_decoders;
pub use southstar_js_temporal;
pub use southstar_mat4;
pub use southstar_mic;
pub use southstar_netutil;
pub use southstar_pdf;
pub use southstar_safebrowsing;
pub use southstar_sandbox;
pub use southstar_spellcheck;
pub use southstar_texture;
pub use southstar_threaddump;
pub use southstar_webcrypto;
pub use southstar_websocket;
#[cfg(feature = "woff2")]
pub use southstar_woff2;
