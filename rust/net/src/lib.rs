//! Southstar — networking: the request queue with its throttle, sharing and preloads, fetching with its redirects, cache and request headers, the curl transport, URL helpers, cookies, HSTS, storage paths, the transport plumbing, the network log, form encoding, body and header sinks, error pages, about: pages, data: URLs, file: and FTP listings and view-source: documents.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod about;
mod budget;
mod coalesce;
mod cookies;
mod data_url;
mod error_page;
mod ffi;
mod file;
mod forms;
mod ftp;
mod hsts;
mod listing;
mod netlog;
mod request;
pub mod route;
mod storage;
mod transport;
mod url;
mod view_source;

pub use about::{is_chrome_page, is_privileged_page};
pub use cookies::{
    for_js as cookies_for_js, jar_version as cookie_jar_version,
    store_from_js as cookie_store_from_js,
};
pub use error_page::build as build_error_page;
pub use ffi::fetch::{
    Response, Timing as ResponseTiming, accept_header_lines,
    ns_net_accept_headers_for as accept_headers_for, request_key,
};
pub use ffi::netlog::set_log_fetches;
pub use ffi::proxy::{http_proxy, https_proxy, no_proxy, proxy_override, set_proxy_override};
pub use ffi::queue::{
    ReadyCallback, fetch_blocking, idle, ns_net_fetch_finish as fetch_finish,
    ns_net_request_async as request_async, preconnect_async, preload_clear, preload_expect,
    shutdown,
};
pub use ffi::transport::{ca_bundle_path, init};
pub use ffi::url::{https_first_upgrade, strip_tracking_params};
pub use ffi::{NsResponse, decode_data_url, set_allow_file_urls};
pub use forms::set_submission_charset;
pub use netlog::{
    Snapshot as PerfSnapshot, clear as log_clear, dump as log_dump, snapshot as perf_snapshot,
};
pub use url::{
    is_nosniff as header_is_nosniff, origin_from, parse_refresh, raw_header_values, same_origin,
};
