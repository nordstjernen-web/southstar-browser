/* Southstar — SQLite-backed browsing history API, implemented in rust/history.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_HISTORY_H
#define NS_HISTORY_H

#include <glib.h>

#include "libsouthstar.h"

G_BEGIN_DECLS

void   ns_history_init(void);
void   ns_history_shutdown(void);

void   ns_history_clear(void);

char  *ns_history_html_page(void);

G_END_DECLS

#endif
