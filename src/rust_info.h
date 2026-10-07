/* Southstar — what the Rust side of the build reports about itself, implemented in rust/southstar-ffi.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_RUST_INFO_H
#define NS_RUST_INFO_H

#include <glib.h>

G_BEGIN_DECLS

const char *ns_rust_compiler_version(void);
const char *ns_rust_minimum_version(void);
const char *ns_rust_build_profile(void);
guint       ns_rust_module_count(void);
const char *ns_rust_modules(void);

G_END_DECLS

#endif
