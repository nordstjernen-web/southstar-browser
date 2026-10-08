//! Southstar — networking from src/net.h: URL helpers, cookies, HSTS, storage paths, the transport plumbing, the network log, form encoding, body and header sinks, error pages, about: pages, data: URLs, file: and FTP listings and view-source: documents.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod about;
mod budget;
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
mod storage;
mod transport;
mod url;
mod view_source;
