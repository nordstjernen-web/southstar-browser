/* Southstar — CSS parser, selectors, cascade.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "css.h"
#include "css_internal.h"
#include "css_syntax.h"

#include "config.h"
#include "image.h"
#include "net.h"

#include <limits.h>
#include <math.h>
#include <string.h>

static gboolean
is_ws(char c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f'; }

static gunichar
css_unescape_cp(gunichar cp)
{
    if (cp == 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF))
        return 0xFFFD;
    return cp;
}

void
ns_css_append_unescaped(GString *out, const char **pp)
{
    const char *p = *pp;
    if (*p == '\\' && p[1]) {
        p++;
        if (g_ascii_isxdigit(*p)) {
            gunichar cp = 0;
            int n = 0;
            while (n < 6 && g_ascii_isxdigit(*p)) {
                cp = cp * 16 + (gunichar)g_ascii_xdigit_value(*p);
                p++;
                n++;
            }
            if (is_ws(*p)) {
                gboolean cr = *p == '\r';
                p++;
                if (cr && *p == '\n') p++;
            }
            g_string_append_unichar(out, css_unescape_cp(cp));
        } else {
            g_string_append_c(out, *p++);
        }
    } else {
        g_string_append_c(out, *p++);
    }
    *pp = p;
}

static ns_css_color_scheme g_color_scheme = NS_CSS_COLOR_SCHEME_LIGHT;
static ns_css_reduced_motion g_reduced_motion = NS_CSS_REDUCED_MOTION_NO_PREFERENCE;

void
ns_css_set_color_scheme(ns_css_color_scheme scheme)
{
    ns_css_color_scheme next = scheme == NS_CSS_COLOR_SCHEME_DARK
        ? NS_CSS_COLOR_SCHEME_DARK : NS_CSS_COLOR_SCHEME_LIGHT;
    if (g_color_scheme == next) return;
    g_color_scheme = next;
    ns_css_stylesheet_cache_drop();
}

void
ns_css_set_reduced_motion(ns_css_reduced_motion motion)
{
    ns_css_reduced_motion next = motion == NS_CSS_REDUCED_MOTION_REDUCE
        ? NS_CSS_REDUCED_MOTION_REDUCE : NS_CSS_REDUCED_MOTION_NO_PREFERENCE;
    if (g_reduced_motion == next) return;
    g_reduced_motion = next;
    ns_css_stylesheet_cache_drop();
}

ns_css_reduced_motion
ns_css_get_reduced_motion(void)
{
    return g_reduced_motion;
}

ns_css_color_scheme
ns_css_get_color_scheme(void)
{
    return g_color_scheme;
}

void
ns_css_style_scale_font_size(ns_style *s, double factor)
{
    if (!s || !s->values[NS_CSS_FONT_SIZE] ||
        s->values[NS_CSS_FONT_SIZE]->kind != NS_CSS_V_LENGTH)
        return;
    ns_css_value_cow(s, NS_CSS_FONT_SIZE)->u.length.v *= factor;
}
