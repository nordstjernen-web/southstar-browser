/* Southstar — the Rust functions only layout.c calls.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_LAYOUT_INTERNAL_H
#define NS_LAYOUT_INTERNAL_H

#include "dom.h"

char *ns_layout_choose_img_url(const ns_node *n, const ns_node **img_out,
                               double *density);

#endif
