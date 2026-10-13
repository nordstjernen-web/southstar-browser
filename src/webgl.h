/* Southstar — minimalist WebGL: canvas.getContext mapped onto GLES, implemented in rust/webgl.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_WEBGL_H
#define NS_WEBGL_H

#include <glib.h>
#include <cairo.h>
#include "dom.h"

G_BEGIN_DECLS

cairo_surface_t *ns_webgl_canvas_surface(const ns_node *canvas);

char *ns_webgl_take_pending_origin(void);
void  ns_webgl_set_decision(const char *origin, int allow);

G_END_DECLS

#endif
