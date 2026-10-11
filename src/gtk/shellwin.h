/* Southstar — the browser window's secondary windows, implemented in rust/shell.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef SOUTHSTAR_GTK_SHELLWIN_H
#define SOUTHSTAR_GTK_SHELLWIN_H

#include <gtk/gtk.h>

G_BEGIN_DECLS

void ns_downloads_show(GtkWidget *parent);

void ns_downloads_start(GtkWidget *parent, const char *url,
                        const char *suggested);

void ns_task_manager_show(GtkWidget *parent, GtkWidget *notebook);

G_END_DECLS

#endif
