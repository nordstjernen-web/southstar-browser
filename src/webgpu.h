/* Southstar — experimental WebGPU (navigator.gpu) over wgpu-native.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_WEBGPU_H
#define NS_WEBGPU_H

#include <glib.h>
#include <cairo.h>
#include "dom.h"

G_BEGIN_DECLS

cairo_surface_t *ns_webgpu_canvas_surface(const ns_node *canvas);

G_END_DECLS

#endif
