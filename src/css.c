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

static double g_viewport_w = 1000;
static double g_viewport_h = 800;

static GHashTable *g_js_registered_props;
static guint64 g_js_registered_serial;

static void css_property_rule_free(gpointer data);

static guint64
ns_css_registered_property_serial(void)
{
    return g_js_registered_serial;
}

static gboolean
css_custom_property_name(const char *name)
{
    if (!name || name[0] != '-' || name[1] != '-' || !name[2]) return FALSE;
    for (const char *p = name + 2; *p; p++)
        if (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r' || *p == '\f')
            return FALSE;
    return TRUE;
}

ns_css_register_status
ns_css_register_property(const char *name, const char *syntax_text,
                         gboolean inherits, const char *initial_value,
                         gboolean has_initial)
{
    if (!css_custom_property_name(name)) return NS_CSS_REGISTER_BAD_NAME;
    if (g_js_registered_props &&
        g_hash_table_contains(g_js_registered_props, name))
        return NS_CSS_REGISTER_EXISTS;
    ns_css_syntax_def *syntax = ns_css_syntax_def_parse(syntax_text);
    if (!syntax) return NS_CSS_REGISTER_BAD_SYNTAX;
    gboolean universal = ns_css_syntax_def_universal(syntax);
    if ((!universal && !has_initial) ||
        (has_initial &&
         !ns_css_syntax_def_initial_valid(syntax, initial_value))) {
        ns_css_syntax_def_free(syntax);
        return NS_CSS_REGISTER_BAD_INITIAL;
    }
    if (!g_js_registered_props)
        g_js_registered_props = g_hash_table_new_full(
            g_str_hash, g_str_equal, NULL, css_property_rule_free);
    ns_css_property_rule *pr = g_new0(ns_css_property_rule, 1);
    pr->name = g_strdup(name);
    pr->initial_value = has_initial ? g_strdup(initial_value) : NULL;
    pr->syntax_text = g_strdup(syntax_text ? syntax_text : "*");
    pr->syntax = syntax;
    pr->inherits = inherits;
    pr->has_initial = has_initial;
    g_hash_table_replace(g_js_registered_props, pr->name, pr);
    g_js_registered_serial++;
    return NS_CSS_REGISTER_OK;
}

void
ns_css_clear_registered_properties(void)
{
    if (g_js_registered_props) {
        g_hash_table_destroy(g_js_registered_props);
        g_js_registered_props = NULL;
        g_js_registered_serial++;
    }
}



void
ns_css_set_viewport(double vw_px, double vh_px)
{
    if (vw_px > 0) g_viewport_w = vw_px;
    if (vh_px > 0) g_viewport_h = vh_px;
}

double ns_css_viewport_w(void) { return g_viewport_w; }
double ns_css_viewport_h(void) { return g_viewport_h; }


static void (*g_frame_viewport_cb)(const ns_node *frame, double *w, double *h);

void
ns_css_set_frame_viewport_cb(void (*cb)(const ns_node *frame,
                                        double *w, double *h))
{
    g_frame_viewport_cb = cb;
}


static double
viewport_coeff_px(double vw, double vh, double vmin, double vmax,
                  double width, double height)
{
    return (vw * width + vh * height + vmin * MIN(width, height) +
            vmax * MAX(width, height)) / 100.0;
}

static double
calc_viewport_refresh_px(const ns_css_value *v)
{
    double vw = v->u.calc.vw, vh = v->u.calc.vh;
    double vmin = v->u.calc.vmin, vmax = v->u.calc.vmax;
    if (vw == 0 && vh == 0 && vmin == 0 && vmax == 0) return 0;
    return viewport_coeff_px(vw, vh, vmin, vmax, g_viewport_w, g_viewport_h) -
           viewport_coeff_px(vw, vh, vmin, vmax, v->u.calc.parsed_vw,
                             v->u.calc.parsed_vh);
}

static const char *kProp[NS_CSS_PROP_COUNT] = {
    [NS_CSS_DISPLAY]              = "display",
    [NS_CSS_COLOR]                = "color",
    [NS_CSS_BACKGROUND_COLOR]     = "background-color",
    [NS_CSS_FONT_SIZE]            = "font-size",
    [NS_CSS_FONT_WEIGHT]          = "font-weight",
    [NS_CSS_FONT_STYLE]           = "font-style",
    [NS_CSS_FONT_STRETCH]         = "font-stretch",
    [NS_CSS_FONT_KERNING]         = "font-kerning",
    [NS_CSS_FONT_VARIANT_LIGATURES] = "font-variant-ligatures",
    [NS_CSS_FONT_FEATURE_SETTINGS] = "font-feature-settings",
    [NS_CSS_FONT_VARIATION_SETTINGS] = "font-variation-settings",
    [NS_CSS_FONT_FAMILY]          = "font-family",
    [NS_CSS_TEXT_ALIGN]           = "text-align",
    [NS_CSS_MARGIN_TOP]           = "margin-top",
    [NS_CSS_MARGIN_RIGHT]         = "margin-right",
    [NS_CSS_MARGIN_BOTTOM]        = "margin-bottom",
    [NS_CSS_MARGIN_LEFT]          = "margin-left",
    [NS_CSS_PADDING_TOP]          = "padding-top",
    [NS_CSS_PADDING_RIGHT]        = "padding-right",
    [NS_CSS_PADDING_BOTTOM]       = "padding-bottom",
    [NS_CSS_PADDING_LEFT]         = "padding-left",
    [NS_CSS_BORDER_TOP_WIDTH]     = "border-top-width",
    [NS_CSS_BORDER_RIGHT_WIDTH]   = "border-right-width",
    [NS_CSS_BORDER_BOTTOM_WIDTH]  = "border-bottom-width",
    [NS_CSS_BORDER_LEFT_WIDTH]    = "border-left-width",
    [NS_CSS_BORDER_TOP_COLOR]     = "border-top-color",
    [NS_CSS_BORDER_RIGHT_COLOR]   = "border-right-color",
    [NS_CSS_BORDER_BOTTOM_COLOR]  = "border-bottom-color",
    [NS_CSS_BORDER_LEFT_COLOR]    = "border-left-color",
    [NS_CSS_BORDER_TOP_STYLE]     = "border-top-style",
    [NS_CSS_BORDER_RIGHT_STYLE]   = "border-right-style",
    [NS_CSS_BORDER_BOTTOM_STYLE]  = "border-bottom-style",
    [NS_CSS_BORDER_LEFT_STYLE]    = "border-left-style",
    [NS_CSS_WIDTH]                = "width",
    [NS_CSS_HEIGHT]               = "height",
    [NS_CSS_MAX_WIDTH]            = "max-width",
    [NS_CSS_MAX_HEIGHT]           = "max-height",
    [NS_CSS_MIN_WIDTH]            = "min-width",
    [NS_CSS_MIN_HEIGHT]           = "min-height",
    [NS_CSS_LINE_HEIGHT]          = "line-height",
    [NS_CSS_TEXT_DECORATION]      = "text-decoration",
    [NS_CSS_POSITION]             = "position",
    [NS_CSS_TOP]                  = "top",
    [NS_CSS_RIGHT]                = "right",
    [NS_CSS_BOTTOM]               = "bottom",
    [NS_CSS_LEFT]                 = "left",
    [NS_CSS_Z_INDEX]              = "z-index",
    [NS_CSS_OPACITY]              = "opacity",
    [NS_CSS_CURSOR]               = "cursor",
    [NS_CSS_POINTER_EVENTS]       = "pointer-events",
    [NS_CSS_LETTER_SPACING]       = "letter-spacing",
    [NS_CSS_WORD_SPACING]         = "word-spacing",
    [NS_CSS_WHITE_SPACE]          = "white-space",
    [NS_CSS_BOX_SIZING]           = "box-sizing",
    [NS_CSS_TEXT_INDENT]          = "text-indent",
    [NS_CSS_TEXT_TRANSFORM]       = "text-transform",
    [NS_CSS_LIST_STYLE_TYPE]      = "list-style-type",
    [NS_CSS_VERTICAL_ALIGN]       = "vertical-align",
    [NS_CSS_VISIBILITY]           = "visibility",
    [NS_CSS_OVERFLOW]             = "overflow",
    [NS_CSS_OVERFLOW_X]           = "overflow-x",
    [NS_CSS_OVERFLOW_Y]           = "overflow-y",
    [NS_CSS_FONT_VARIANT]         = "font-variant",
    [NS_CSS_BORDER_RADIUS]            = "border-radius",
    [NS_CSS_BORDER_TOP_LEFT_RADIUS]     = "border-top-left-radius",
    [NS_CSS_BORDER_TOP_RIGHT_RADIUS]    = "border-top-right-radius",
    [NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS] = "border-bottom-right-radius",
    [NS_CSS_BORDER_BOTTOM_LEFT_RADIUS]  = "border-bottom-left-radius",
    [NS_CSS_FLEX_DIRECTION]       = "flex-direction",
    [NS_CSS_FLEX_WRAP]            = "flex-wrap",
    [NS_CSS_JUSTIFY_CONTENT]      = "justify-content",
    [NS_CSS_ALIGN_ITEMS]          = "align-items",
    [NS_CSS_ALIGN_SELF]           = "align-self",
    [NS_CSS_GAP]                  = "gap",
    [NS_CSS_ROW_GAP]              = "row-gap",
    [NS_CSS_COLUMN_GAP]           = "column-gap",
    [NS_CSS_FLEX_GROW]            = "flex-grow",
    [NS_CSS_FLEX_SHRINK]          = "flex-shrink",
    [NS_CSS_FLEX_BASIS]           = "flex-basis",
    [NS_CSS_ORDER]                = "order",
    [NS_CSS_FLOAT]                = "float",
    [NS_CSS_CLEAR]                = "clear",
    [NS_CSS_BOX_SHADOW]           = "box-shadow",
    [NS_CSS_OUTLINE_WIDTH]        = "outline-width",
    [NS_CSS_OUTLINE_STYLE]        = "outline-style",
    [NS_CSS_OUTLINE_COLOR]        = "outline-color",
    [NS_CSS_OUTLINE_OFFSET]       = "outline-offset",
    [NS_CSS_BACKGROUND_IMAGE]     = "background-image",
    [NS_CSS_BACKGROUND_REPEAT]    = "background-repeat",
    [NS_CSS_BACKGROUND_POSITION_X]= "background-position-x",
    [NS_CSS_BACKGROUND_POSITION_Y]= "background-position-y",
    [NS_CSS_BACKGROUND_SIZE]      = "background-size",
    [NS_CSS_BACKGROUND_CLIP]      = "background-clip",
    [NS_CSS_BACKGROUND_ORIGIN]    = "background-origin",
    [NS_CSS_SCROLLBAR_WIDTH]      = "scrollbar-width",
    [NS_CSS_SCROLLBAR_COLOR]      = "scrollbar-color",
    [NS_CSS_IMAGE_RENDERING]      = "image-rendering",
    [NS_CSS_CONTENT]              = "content",
    [NS_CSS_CLIP]                 = "clip",
    [NS_CSS_CONTENT_VISIBILITY]   = "content-visibility",
    [NS_CSS_GRID_TEMPLATE_COLUMNS]= "grid-template-columns",
    [NS_CSS_GRID_TEMPLATE_ROWS]   = "grid-template-rows",
    [NS_CSS_GRID_TEMPLATE_AREAS]  = "grid-template-areas",
    [NS_CSS_GRID_COLUMN]          = "grid-column",
    [NS_CSS_GRID_ROW]             = "grid-row",
    [NS_CSS_GRID_COLUMN_START]    = "grid-column-start",
    [NS_CSS_GRID_COLUMN_END]      = "grid-column-end",
    [NS_CSS_GRID_ROW_START]       = "grid-row-start",
    [NS_CSS_GRID_ROW_END]         = "grid-row-end",
    [NS_CSS_GRID_AREA]            = "grid-area",
    [NS_CSS_GRID_AUTO_ROWS]       = "grid-auto-rows",
    [NS_CSS_GRID_AUTO_COLUMNS]    = "grid-auto-columns",
    [NS_CSS_GRID_AUTO_FLOW]       = "grid-auto-flow",
    [NS_CSS_TRANSFORM]            = "transform",
    [NS_CSS_TRANSFORM_ORIGIN]     = "transform-origin",
    [NS_CSS_TRANSITION]           = "transition",
    [NS_CSS_ANIMATION]            = "animation",
    [NS_CSS_ASPECT_RATIO]         = "aspect-ratio",
    [NS_CSS_TEXT_SHADOW]          = "text-shadow",
    [NS_CSS_OVERFLOW_WRAP]        = "overflow-wrap",
    [NS_CSS_WORD_BREAK]           = "word-break",
    [NS_CSS_HYPHENS]              = "hyphens",
    [NS_CSS_TEXT_OVERFLOW]        = "text-overflow",
    [NS_CSS_TEXT_DECORATION_COLOR]= "text-decoration-color",
    [NS_CSS_TEXT_DECORATION_STYLE]= "text-decoration-style",
    [NS_CSS_LIST_STYLE_POSITION]  = "list-style-position",
    [NS_CSS_LIST_STYLE_IMAGE]     = "list-style-image",
    [NS_CSS_USER_SELECT]          = "user-select",
    [NS_CSS_QUOTES]               = "quotes",
    [NS_CSS_COLUMN_COUNT]         = "column-count",
    [NS_CSS_COLUMN_WIDTH]         = "column-width",
    [NS_CSS_COLUMN_RULE_WIDTH]    = "column-rule-width",
    [NS_CSS_COLUMN_RULE_STYLE]    = "column-rule-style",
    [NS_CSS_COLUMN_RULE_COLOR]    = "column-rule-color",
    [NS_CSS_FILTER]               = "filter",
    [NS_CSS_CLIP_PATH]            = "clip-path",
    [NS_CSS_MIX_BLEND_MODE]       = "mix-blend-mode",
    [NS_CSS_ACCENT_COLOR]         = "accent-color",
    [NS_CSS_COUNTER_RESET]        = "counter-reset",
    [NS_CSS_COUNTER_INCREMENT]    = "counter-increment",
    [NS_CSS_LINE_CLAMP]           = "-webkit-line-clamp",
    [NS_CSS_OBJECT_FIT]           = "object-fit",
    [NS_CSS_OBJECT_POSITION_X]    = "object-position-x",
    [NS_CSS_OBJECT_POSITION_Y]    = "object-position-y",
    [NS_CSS_MASK_IMAGE]           = "mask-image",
    [NS_CSS_APPEARANCE]           = "appearance",
    [NS_CSS_TABLE_LAYOUT]         = "table-layout",
    [NS_CSS_CAPTION_SIDE]         = "caption-side",
    [NS_CSS_BORDER_COLLAPSE]      = "border-collapse",
    [NS_CSS_BORDER_SPACING]       = "border-spacing",
    [NS_CSS_CONTAINER_TYPE]       = "container-type",
    [NS_CSS_CONTAINER_NAME]       = "container-name",
    [NS_CSS_WRITING_MODE]         = "writing-mode",
    [NS_CSS_TEXT_ORIENTATION]     = "text-orientation",
    [NS_CSS_TRANSITION_DELAY]     = "transition-delay",
    [NS_CSS_TRANSITION_DURATION]  = "transition-duration",
    [NS_CSS_ANIMATION_DELAY]      = "animation-delay",
    [NS_CSS_ANIMATION_DURATION]   = "animation-duration",
    [NS_CSS_ANIMATION_NAME]       = "animation-name",
    [NS_CSS_ANIMATION_TIMING_FUNCTION] = "animation-timing-function",
    [NS_CSS_ANIMATION_ITERATION_COUNT] = "animation-iteration-count",
    [NS_CSS_ANIMATION_DIRECTION]  = "animation-direction",
    [NS_CSS_ANIMATION_FILL_MODE]  = "animation-fill-mode",
    [NS_CSS_TRANSITION_PROPERTY]  = "transition-property",
    [NS_CSS_TRANSITION_TIMING_FUNCTION] = "transition-timing-function",
    [NS_CSS_TRANSITION_BEHAVIOR]  = "transition-behavior",
    [NS_CSS_ANIMATION_TIMELINE]   = "animation-timeline",
    [NS_CSS_ANIMATION_RANGE_START] = "animation-range-start",
    [NS_CSS_ANIMATION_RANGE_END]  = "animation-range-end",
    [NS_CSS_ANIMATION_COMPOSITION] = "animation-composition",
    [NS_CSS_COUNTER_SET]          = "counter-set",
    [NS_CSS_OVERFLOW_CLIP_MARGIN] = "overflow-clip-margin",
    [NS_CSS_WEBKIT_BOX_ORIENT]    = "-webkit-box-orient",
    [NS_CSS_MASK_CLIP]            = "mask-clip",
    [NS_CSS_MASK_COMPOSITE]       = "mask-composite",
    [NS_CSS_BACKGROUND_ATTACHMENT] = "background-attachment",
    [NS_CSS_TRANSFORM_BOX]        = "transform-box",
    [NS_CSS_ORPHANS]              = "orphans",
    [NS_CSS_WIDOWS]               = "widows",
    [NS_CSS_MAX_LINES]            = "max-lines",
    [NS_CSS_HYPHENATE_LIMIT_LINES] = "hyphenate-limit-lines",
    [NS_CSS_COLUMN_SPAN]          = "column-span",
    [NS_CSS_BREAK_BEFORE]         = "break-before",
    [NS_CSS_BREAK_AFTER]          = "break-after",
    [NS_CSS_BREAK_INSIDE]         = "break-inside",
    [NS_CSS_SCROLL_SNAP_TYPE]     = "scroll-snap-type",
    [NS_CSS_SCROLL_SNAP_ALIGN]    = "scroll-snap-align",
    [NS_CSS_SCROLL_SNAP_STOP]     = "scroll-snap-stop",
    [NS_CSS_SCROLL_PADDING_TOP]   = "scroll-padding-top",
    [NS_CSS_SCROLL_PADDING_RIGHT] = "scroll-padding-right",
    [NS_CSS_SCROLL_PADDING_BOTTOM]= "scroll-padding-bottom",
    [NS_CSS_SCROLL_PADDING_LEFT]  = "scroll-padding-left",
    [NS_CSS_SCROLL_MARGIN_TOP]    = "scroll-margin-top",
    [NS_CSS_SCROLL_MARGIN_RIGHT]  = "scroll-margin-right",
    [NS_CSS_SCROLL_MARGIN_BOTTOM] = "scroll-margin-bottom",
    [NS_CSS_SCROLL_MARGIN_LEFT]   = "scroll-margin-left",
    [NS_CSS_CARET_COLOR]          = "caret-color",
    [NS_CSS_TAB_SIZE]             = "tab-size",
    [NS_CSS_JUSTIFY_ITEMS]        = "justify-items",
    [NS_CSS_JUSTIFY_SELF]         = "justify-self",
    [NS_CSS_ALIGN_CONTENT]        = "align-content",
    [NS_CSS_DIRECTION]            = "direction",
    [NS_CSS_UNICODE_BIDI]         = "unicode-bidi",
    [NS_CSS_TRANSLATE]            = "translate",
    [NS_CSS_ROTATE]               = "rotate",
    [NS_CSS_SCALE]                = "scale",
    [NS_CSS_PERSPECTIVE]          = "perspective",
    [NS_CSS_PERSPECTIVE_ORIGIN]   = "perspective-origin",
    [NS_CSS_TRANSFORM_STYLE]      = "transform-style",
    [NS_CSS_BACKFACE_VISIBILITY]  = "backface-visibility",
    [NS_CSS_ANIMATION_PLAY_STATE] = "animation-play-state",
    [NS_CSS_BORDER_IMAGE_SOURCE]  = "border-image-source",
    [NS_CSS_BORDER_IMAGE_SLICE]   = "border-image-slice",
    [NS_CSS_BORDER_IMAGE_WIDTH]   = "border-image-width",
    [NS_CSS_BORDER_IMAGE_OUTSET]  = "border-image-outset",
    [NS_CSS_BORDER_IMAGE_REPEAT]  = "border-image-repeat",
    [NS_CSS_FILL]                 = "fill",
    [NS_CSS_FILL_OPACITY]         = "fill-opacity",
    [NS_CSS_FILL_RULE]            = "fill-rule",
    [NS_CSS_STROKE]               = "stroke",
    [NS_CSS_STROKE_WIDTH]         = "stroke-width",
    [NS_CSS_STROKE_OPACITY]       = "stroke-opacity",
    [NS_CSS_STROKE_LINECAP]       = "stroke-linecap",
    [NS_CSS_STROKE_LINEJOIN]      = "stroke-linejoin",
    [NS_CSS_STROKE_MITERLIMIT]    = "stroke-miterlimit",
    [NS_CSS_STROKE_DASHARRAY]     = "stroke-dasharray",
    [NS_CSS_STROKE_DASHOFFSET]    = "stroke-dashoffset",
    [NS_CSS_STOP_COLOR]           = "stop-color",
    [NS_CSS_STOP_OPACITY]         = "stop-opacity",
    [NS_CSS_CLIP_RULE]            = "clip-rule",
    [NS_CSS_TEXT_ANCHOR]          = "text-anchor",
    [NS_CSS_DOMINANT_BASELINE]    = "dominant-baseline",
    [NS_CSS_PAINT_ORDER]          = "paint-order",
    [NS_CSS_VECTOR_EFFECT]        = "vector-effect",
    [NS_CSS_SHAPE_RENDERING]      = "shape-rendering",
    [NS_CSS_SVG_X]                = "x",
    [NS_CSS_SVG_Y]                = "y",
    [NS_CSS_CX]                   = "cx",
    [NS_CSS_CY]                   = "cy",
    [NS_CSS_R]                    = "r",
    [NS_CSS_RX]                   = "rx",
    [NS_CSS_RY]                   = "ry",
};

static gboolean
prop_inherits(ns_css_prop p)
{
    switch (p) {
    case NS_CSS_COLOR:
    case NS_CSS_FONT_SIZE:
    case NS_CSS_FONT_WEIGHT:
    case NS_CSS_FONT_STYLE:
    case NS_CSS_FONT_STRETCH:
    case NS_CSS_FONT_KERNING:
    case NS_CSS_FONT_VARIANT_LIGATURES:
    case NS_CSS_FONT_FEATURE_SETTINGS:
    case NS_CSS_FONT_VARIATION_SETTINGS:
    case NS_CSS_FONT_FAMILY:
    case NS_CSS_FONT_VARIANT:
    case NS_CSS_LINE_HEIGHT:
    case NS_CSS_LETTER_SPACING:
    case NS_CSS_WORD_SPACING:
    case NS_CSS_WHITE_SPACE:
    case NS_CSS_HYPHENS:
    case NS_CSS_DIRECTION:
    case NS_CSS_WRITING_MODE:
    case NS_CSS_TEXT_ORIENTATION:
    case NS_CSS_CAPTION_SIDE:
    case NS_CSS_BORDER_COLLAPSE:
    case NS_CSS_BORDER_SPACING:
    case NS_CSS_TEXT_ALIGN:
    case NS_CSS_TEXT_INDENT:
    case NS_CSS_TEXT_TRANSFORM:
    case NS_CSS_LIST_STYLE_TYPE:
    case NS_CSS_LIST_STYLE_POSITION:
    case NS_CSS_LIST_STYLE_IMAGE:
    case NS_CSS_USER_SELECT:
    case NS_CSS_QUOTES:
    case NS_CSS_VISIBILITY:
    case NS_CSS_CURSOR:
    case NS_CSS_POINTER_EVENTS:
    case NS_CSS_SCROLLBAR_COLOR:
    case NS_CSS_IMAGE_RENDERING:
    case NS_CSS_TAB_SIZE:
    case NS_CSS_WORD_BREAK:
    case NS_CSS_OVERFLOW_WRAP:
    case NS_CSS_CARET_COLOR:
    case NS_CSS_ACCENT_COLOR:
    case NS_CSS_FILL:
    case NS_CSS_FILL_OPACITY:
    case NS_CSS_FILL_RULE:
    case NS_CSS_STROKE:
    case NS_CSS_STROKE_WIDTH:
    case NS_CSS_STROKE_OPACITY:
    case NS_CSS_STROKE_LINECAP:
    case NS_CSS_STROKE_LINEJOIN:
    case NS_CSS_STROKE_MITERLIMIT:
    case NS_CSS_STROKE_DASHARRAY:
    case NS_CSS_STROKE_DASHOFFSET:
    case NS_CSS_CLIP_RULE:
    case NS_CSS_TEXT_ANCHOR:
    case NS_CSS_PAINT_ORDER:
    case NS_CSS_SHAPE_RENDERING:
    case NS_CSS_TEXT_SHADOW:
    case NS_CSS_ORPHANS:
    case NS_CSS_WIDOWS:
    case NS_CSS_DOMINANT_BASELINE:
        return TRUE;
    default:
        return FALSE;
    }
}

int
ns_css_writing_mode(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_WRITING_MODE] : NULL;
    if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return 0;
    const char *k = v->u.keyword;
    if (strcmp(k, "vertical-rl") == 0 || strcmp(k, "sideways-rl") == 0 ||
        strcmp(k, "tb-rl") == 0 || strcmp(k, "tb") == 0)
        return 1;
    if (strcmp(k, "vertical-lr") == 0 || strcmp(k, "sideways-lr") == 0)
        return 2;
    return 0;
}

int
ns_css_text_orientation(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_TEXT_ORIENTATION] : NULL;
    if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return 0;
    const char *k = v->u.keyword;
    if (strcmp(k, "upright") == 0) return 1;
    if (strcmp(k, "sideways") == 0 || strcmp(k, "sideways-right") == 0) return 2;
    return 0;
}

static gboolean
is_ws(char c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f'; }

static gboolean
is_ident_start(char c)
{
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_' || c == '-' || (unsigned char)c >= 128;
}

static gboolean
is_ident(char c)
{
    return is_ident_start(c) || (c >= '0' && c <= '9');
}

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

static const char *css_skip_ws_comments(const char *p, const char *end);
static const char *css_scan_until(const char *p, const char *end,
                                  const char *terminators, char *terminator);
static const char *css_find_top_level_char(const char *p, const char *end,
                                           char needle);
static const char *css_skip_comment(const char *p, const char *end);
static void css_strip_important(char *text, gboolean *important);
static const char *match_close_paren(const char *p, const char *end);

ns_css_value *
ns_css_value_dup(const ns_css_value *v)
{
    if (!v) return NULL;
    ((ns_css_value *)v)->ref++;
    return (ns_css_value *)v;
}

void
ns_css_value_free(ns_css_value *v)
{
    while (v) {
        if (v->ref > 0) { v->ref--; return; }
        if (v->kind == NS_CSS_V_KEYWORD) g_free(v->u.keyword);
        else if (v->kind == NS_CSS_V_URL) g_free(v->u.url);
        else if (v->kind == NS_CSS_V_AREAS) {
            for (int i = 0; i < v->u.areas.n_rects; i++)
                g_free(v->u.areas.rects[i].name);
        }
        else if (v->kind == NS_CSS_V_ANIM) {
            for (int i = 0; i < v->u.anim.n; i++)
                g_free(v->u.anim.entries[i].name);
        }
        g_free(v->image_set_text);
        g_free(v->specified);
        ns_css_value *next = v->next_layer;
        g_free(v);
        v = next;
    }
}

int
ns_css_value_layer_count(const ns_css_value *head)
{
    int n = 0;
    for (const ns_css_value *l = head; l; l = l->next_layer) n++;
    return n;
}

const ns_css_value *
ns_css_value_layer(const ns_css_value *head, int index)
{
    int n = ns_css_value_layer_count(head);
    if (n == 0) return NULL;
    index %= n;
    const ns_css_value *l = head;
    while (index-- > 0) l = l->next_layer;
    return l;
}


static double
column_len_px(const ns_css_value *v, double basis, double fallback)
{
    if (!v) return fallback;
    if (ns_css_calc_is_math_fn(v))
        return ns_css_calc_math_fn_px(v, basis);
    if (v->kind == NS_CSS_V_CALC)
        return v->u.calc.pct / 100.0 * basis + v->u.calc.px;
    if (v->kind != NS_CSS_V_LENGTH) return fallback;
    switch (v->u.length.unit) {
    case NS_CSS_UNIT_PX:
    case NS_CSS_UNIT_NUMBER: return v->u.length.v;
    case NS_CSS_UNIT_EM:
    case NS_CSS_UNIT_REM:    return v->u.length.v * 16.0;
    case NS_CSS_UNIT_PERCENT: return v->u.length.v * basis / 100.0;
    case NS_CSS_UNIT_VW:     return v->u.length.v * ns_css_viewport_w() / 100.0;
    case NS_CSS_UNIT_VH:     return v->u.length.v * ns_css_viewport_h() / 100.0;
    default:                 return fallback;
    }
}

double
ns_css_dimension_px(const ns_css_value *v, double font_size, double basis)
{
    if (!v) return 0;
    if (ns_css_calc_is_math_fn(v) && basis > 0) {
        double out = ns_css_calc_math_fn_px(v, basis);
        return out > 0 ? out : 0;
    }
    if (v->kind == NS_CSS_V_CALC) {
        double out = v->u.calc.px;
        if (basis > 0) out += v->u.calc.pct * basis / 100.0;
        return out > 0 ? out : 0;
    }
    if (v->kind != NS_CSS_V_LENGTH) return 0;
    double n = v->u.length.v;
    ns_css_unit unit = v->u.length.unit;
    switch (unit) {
    case NS_CSS_UNIT_PX:
    case NS_CSS_UNIT_NUMBER: return n;
    case NS_CSS_UNIT_EM:     return n * font_size;
    case NS_CSS_UNIT_REM:    return n * 16.0;
    case NS_CSS_UNIT_PERCENT: return basis > 0 ? n * basis / 100.0 : 0;
    case NS_CSS_UNIT_EX:
    case NS_CSS_UNIT_CH:
    case NS_CSS_UNIT_CAP:
    case NS_CSS_UNIT_IC:
        return n * ns_css_font_relative_unit_px(unit, font_size, NULL, 400, FALSE);
    case NS_CSS_UNIT_LH:     return n * font_size * 1.5;
    case NS_CSS_UNIT_RLH:    return n * 24.0;
    case NS_CSS_UNIT_REX:
    case NS_CSS_UNIT_RCH:    return n * 8.0;
    case NS_CSS_UNIT_RCAP:   return n * 11.2;
    case NS_CSS_UNIT_RIC:    return n * 16.0;
    default: {
        double r = ns_css_container_unit_resolve(n, unit);
        if (r != 0) return r;
        return ns_css_viewport_resolve(n, unit);
    }
    }
}

int
ns_css_used_column_count(const ns_style *s, double avail_w, double *out_gap)
{
    double gap = 16.0;
    if (s) {
        const ns_css_value *cg = s->values[NS_CSS_COLUMN_GAP];
        if (!cg || cg->kind != NS_CSS_V_LENGTH)
            cg = s->values[NS_CSS_GAP];
        if (cg) {
            double g = column_len_px(cg, avail_w, -1);
            if (g >= 0) gap = g;
        }
    }
    if (out_gap) *out_gap = gap;
    int n = 1;
    if (s && s->values[NS_CSS_COLUMN_COUNT] &&
        s->values[NS_CSS_COLUMN_COUNT]->kind == NS_CSS_V_LENGTH) {
        double v = s->values[NS_CSS_COLUMN_COUNT]->u.length.v;
        if (v >= 2) n = (int)(v + 0.5);
    }
    if (n == 1 && s && s->values[NS_CSS_COLUMN_WIDTH] &&
        s->values[NS_CSS_COLUMN_WIDTH]->kind == NS_CSS_V_LENGTH) {
        double colw = column_len_px(s->values[NS_CSS_COLUMN_WIDTH], avail_w, 0);
        if (colw > 1 && avail_w > colw + gap) {
            int fit = (int)((avail_w + gap) / (colw + gap));
            if (fit > 1) n = fit;
        }
    }
    return n;
}

gboolean
ns_css_keyword_is(const ns_css_value *v, const char *kw)
{
    return v && v->kind == NS_CSS_V_KEYWORD && kw &&
           v->u.keyword && strcmp(v->u.keyword, kw) == 0;
}

typedef struct ns_css_scope {
    GPtrArray *roots;
    GPtrArray *limits;
} ns_css_scope;

#define NS_CSS_MAX_AT_NESTING 32

static const char *
match_close_paren(const char *p, const char *end)
{
    int depth = 1;
    while (p < end && depth > 0) {
        if (*p == '(') depth++;
        else if (*p == ')') { depth--; if (depth == 0) return p; }
        p++;
    }
    return NULL;
}








void
ns_css_style_effective_transform(const ns_style *st,
                                 const ns_css_transform *transform_override,
                                 ns_css_transform *out)
{
    memset(out, 0, sizeof(*out));
    static const ns_css_prop independent[3] = {
        NS_CSS_TRANSLATE, NS_CSS_ROTATE, NS_CSS_SCALE,
    };
    for (int i = 0; i < 3; i++) {
        const ns_css_value *v = st ? st->values[independent[i]] : NULL;
        if (v && v->kind == NS_CSS_V_TRANSFORM && v->u.transform.n_ops > 0 &&
            out->n_ops < NS_CSS_TRANSFORM_OPS_MAX)
            out->ops[out->n_ops++] = v->u.transform.ops[0];
    }
    const ns_css_transform *tf = transform_override;
    if (!tf && st && st->values[NS_CSS_TRANSFORM] &&
        st->values[NS_CSS_TRANSFORM]->kind == NS_CSS_V_TRANSFORM)
        tf = &st->values[NS_CSS_TRANSFORM]->u.transform;
    if (tf)
        for (int i = 0; i < tf->n_ops && out->n_ops < NS_CSS_TRANSFORM_OPS_MAX; i++)
            out->ops[out->n_ops++] = tf->ops[i];
}


static int prop_id(const char *name);


ns_display
ns_css_display_of(const ns_style *s)
{
    ns_display d = { 0 };
    return s ? s->display : d;
}




static ns_css_value *
keyword_value_dup(const char *canonical)
{
    ns_css_value *v = g_new0(ns_css_value, 1);
    v->kind = NS_CSS_V_KEYWORD;
    v->u.keyword = g_strdup(canonical);
    return v;
}

const char *
ns_css_alignment_base(const char *kw)
{
    if (!kw) return NULL;
    if (g_str_has_prefix(kw, "safe ")) return kw + 5;
    if (g_str_has_prefix(kw, "unsafe ")) return kw + 7;
    if (g_str_has_prefix(kw, "legacy ")) return kw + 7;
    return kw;
}

static gboolean
prop_is_alignment(ns_css_prop p)
{
    return p == NS_CSS_JUSTIFY_CONTENT || p == NS_CSS_ALIGN_ITEMS ||
           p == NS_CSS_ALIGN_SELF || p == NS_CSS_ALIGN_CONTENT ||
           p == NS_CSS_JUSTIFY_ITEMS || p == NS_CSS_JUSTIFY_SELF;
}

static ns_css_value *
keyword_value(char *owned)
{
    ns_css_value *v = g_new0(ns_css_value, 1);
    v->kind = NS_CSS_V_KEYWORD;
    v->u.keyword = owned;
    return v;
}

const ns_css_value *
ns_css_border_image_source(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_BORDER_IMAGE_SOURCE] : NULL;
    return v && (v->kind == NS_CSS_V_URL || v->kind == NS_CSS_V_GRADIENT)
        ? v : NULL;
}

static const struct { const char *logical; const char *physical; } kLogicalAlias[] = {
    { "margin-block-start",        "margin-top" },
    { "margin-block-end",          "margin-bottom" },
    { "margin-inline-start",       "margin-left" },
    { "margin-inline-end",         "margin-right" },
    { "padding-block-start",       "padding-top" },
    { "padding-block-end",         "padding-bottom" },
    { "padding-inline-start",      "padding-left" },
    { "padding-inline-end",        "padding-right" },
    { "border-block-start-width",  "border-top-width" },
    { "border-block-end-width",    "border-bottom-width" },
    { "border-inline-start-width", "border-left-width" },
    { "border-inline-end-width",   "border-right-width" },
    { "border-block-start-style",  "border-top-style" },
    { "border-block-end-style",    "border-bottom-style" },
    { "border-inline-start-style", "border-left-style" },
    { "border-inline-end-style",   "border-right-style" },
    { "border-block-start-color",  "border-top-color" },
    { "border-block-end-color",    "border-bottom-color" },
    { "border-inline-start-color", "border-left-color" },
    { "border-inline-end-color",   "border-right-color" },
    { "border-start-start-radius", "border-top-left-radius" },
    { "border-start-end-radius",   "border-top-right-radius" },
    { "border-end-start-radius",   "border-bottom-left-radius" },
    { "border-end-end-radius",     "border-bottom-right-radius" },
    { "inset-block-start",         "top" },
    { "inset-block-end",           "bottom" },
    { "inset-inline-start",        "left" },
    { "inset-inline-end",          "right" },
    { "block-size",                "height" },
    { "inline-size",               "width" },
    { "min-block-size",            "min-height" },
    { "min-inline-size",           "min-width" },
    { "max-block-size",            "max-height" },
    { "max-inline-size",           "max-width" },
};

static const char *
alias_logical(const char *name)
{
    for (gsize i = 0; i < G_N_ELEMENTS(kLogicalAlias); i++)
        if (g_ascii_strcasecmp(name, kLogicalAlias[i].logical) == 0)
            return kLogicalAlias[i].physical;
    return NULL;
}

static int
prop_id(const char *name)
{
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++) {
        if (g_ascii_strcasecmp(name, kProp[i]) == 0) return i;
    }
    if (g_ascii_strcasecmp(name, "word-wrap") == 0)
        return NS_CSS_OVERFLOW_WRAP;
    if (g_ascii_strcasecmp(name, "text-decoration-line") == 0)
        return NS_CSS_TEXT_DECORATION;
    if (g_ascii_strcasecmp(name, "line-clamp") == 0)
        return NS_CSS_LINE_CLAMP;
    if (g_ascii_strcasecmp(name, "text-wrap") == 0 ||
        g_ascii_strcasecmp(name, "text-wrap-mode") == 0)
        return NS_CSS_WHITE_SPACE;
    if (g_ascii_strcasecmp(name, "-webkit-mask-image") == 0)
        return NS_CSS_MASK_IMAGE;
    if (g_ascii_strcasecmp(name, "-webkit-mask-clip") == 0)
        return NS_CSS_MASK_CLIP;
    if (g_ascii_strcasecmp(name, "-webkit-mask-composite") == 0)
        return NS_CSS_MASK_COMPOSITE;
    if (g_ascii_strcasecmp(name, "-webkit-background-clip") == 0)
        return NS_CSS_BACKGROUND_CLIP;
    if (g_ascii_strcasecmp(name, "-webkit-border-radius") == 0)
        return NS_CSS_BORDER_RADIUS;
    if (g_ascii_strcasecmp(name, "-webkit-border-top-left-radius") == 0)
        return NS_CSS_BORDER_TOP_LEFT_RADIUS;
    if (g_ascii_strcasecmp(name, "-webkit-border-top-right-radius") == 0)
        return NS_CSS_BORDER_TOP_RIGHT_RADIUS;
    if (g_ascii_strcasecmp(name, "-webkit-border-bottom-right-radius") == 0)
        return NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS;
    if (g_ascii_strcasecmp(name, "-webkit-border-bottom-left-radius") == 0)
        return NS_CSS_BORDER_BOTTOM_LEFT_RADIUS;
    if (g_ascii_strcasecmp(name, "-webkit-appearance") == 0 ||
        g_ascii_strcasecmp(name, "-moz-appearance") == 0)
        return NS_CSS_APPEARANCE;
    const char *phys = alias_logical(name);
    if (phys) {
        for (int i = 0; i < NS_CSS_PROP_COUNT; i++)
            if (g_ascii_strcasecmp(phys, kProp[i]) == 0) return i;
    }
    return -1;
}

gboolean
ns_css_prop_inherits(int prop)
{
    return prop >= 0 && prop < NS_CSS_PROP_COUNT &&
           prop_inherits((ns_css_prop)prop);
}

const char *
ns_css_prop_name(int prop)
{
    return prop >= 0 && prop < NS_CSS_PROP_COUNT
        ? kProp[prop] : NULL;
}

int
ns_css_prop_id(const char *name)
{
    return name ? prop_id(name) : -1;
}



typedef enum ns_custom_prop_wide {
    NS_CUSTOM_WIDE_NONE,
    NS_CUSTOM_WIDE_INHERIT,
    NS_CUSTOM_WIDE_INITIAL,
    NS_CUSTOM_WIDE_UNSET,
    NS_CUSTOM_WIDE_REVERT,
    NS_CUSTOM_WIDE_REVERT_LAYER,
    NS_CUSTOM_WIDE_REVERT_RULE,
} ns_custom_prop_wide;

typedef struct ns_var_map {
    int ref;
    GHashTable *own;
    struct ns_var_map *parent;
    GPtrArray *names;
} ns_var_map;

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(ns_var_map) == 32);
#endif

static __thread GHashTable *g_registered_props;

static ns_var_map *
ns_var_map_new(GHashTable *own, ns_var_map *parent)
{
    ns_var_map *m = g_new0(ns_var_map, 1);
    m->ref = 1;
    m->own = own;
    m->parent = parent;
    return m;
}

static ns_var_map *
ns_var_map_ref(ns_var_map *m)
{
    if (m) m->ref++;
    return m;
}

static void
ns_var_map_unref(ns_var_map *m)
{
    while (m && --m->ref <= 0) {
        ns_var_map *parent = m->parent;
        if (m->own) g_hash_table_destroy(m->own);
        if (m->names) g_ptr_array_unref(m->names);
        g_free(m);
        m = parent;
    }
}


char *
ns_css_resolve_style_vars(const char *text, const ns_style *style)
{
    return ns_css_substitute_vars(text, style ? style->vars : NULL,
                                  g_registered_props, 0);
}

static const char *
css_skip_comment(const char *p, const char *end)
{
    if (p + 1 >= end || p[0] != '/' || p[1] != '*') return p;
    p += 2;
    while (p + 1 < end && !(p[0] == '*' && p[1] == '/')) p++;
    return p + 1 < end ? p + 2 : end;
}

static const char *
css_skip_ws_comments(const char *p, const char *end)
{
    for (;;) {
        while (p < end && is_ws(*p)) p++;
        if (p + 1 < end && p[0] == '/' && p[1] == '*') {
            p = css_skip_comment(p, end);
            continue;
        }
        return p;
    }
}

static const char *
css_scan_until(const char *p, const char *end,
               const char *terminators, char *terminator)
{
    char quote = 0;
    int paren = 0, bracket = 0, brace = 0;
    if (terminator) *terminator = 0;
    while (p < end) {
        char c = *p;
        if (quote) {
            if (c == '\\' && p + 1 < end) {
                p += 2;
                continue;
            }
            if (c == quote) quote = 0;
            else if (c == '\n' || c == '\r' || c == '\f') quote = 0;
            p++;
            continue;
        }
        if (c == '/' && p + 1 < end && p[1] == '*') {
            p = css_skip_comment(p, end);
            continue;
        }
        if (c == '\\' && p + 1 < end) {
            p += 2;
            continue;
        }
        if (c == '"' || c == '\'') {
            quote = c;
            p++;
            continue;
        }
        if (paren == 0 && bracket == 0 && brace == 0 &&
            strchr(terminators, c)) {
            if (terminator) *terminator = c;
            return p;
        }
        if (c == '(') paren++;
        else if (c == ')' && paren > 0) paren--;
        else if (c == '[') bracket++;
        else if (c == ']' && bracket > 0) bracket--;
        else if (c == '{') brace++;
        else if (c == '}' && brace > 0) brace--;
        p++;
    }
    return p;
}

static const char *
css_find_top_level_char(const char *p, const char *end, char needle)
{
    char terms[2] = { needle, 0 };
    char term = 0;
    const char *q = css_scan_until(p, end, terms, &term);
    return term == needle ? q : NULL;
}

static void
css_strip_important(char *text, gboolean *important)
{
    if (important) *important = FALSE;
    if (!text) return;
    const char *start = text;
    const char *end = text + strlen(text);
    const char *p = start;
    const char *bang = NULL;
    while (p < end) {
        const char *q = css_find_top_level_char(p, end, '!');
        if (!q) break;
        bang = q;
        p = q + 1;
    }
    if (!bang) return;
    const char *tail = css_skip_ws_comments(bang + 1, end);
    if ((gsize)(end - tail) < 9 ||
        g_ascii_strncasecmp(tail, "important", 9) != 0)
        return;
    const char *after = tail + 9;
    if (after < end && is_ident(*after)) return;
    after = css_skip_ws_comments(after, end);
    if (after != end) return;
    *((char *)bang) = '\0';
    g_strchomp(text);
    if (important) *important = TRUE;
}

static void
css_property_rule_free(gpointer data)
{
    ns_css_property_rule_clear(data);
    g_free(data);
}

ns_css_keyframes *
ns_css_keyframes_resolve(const ns_css_keyframes *kf,
                         const struct ns_var_map *vars)
{
    if (!kf) return NULL;
    gboolean any_raw = FALSE;
    for (int i = 0; i < kf->n_stops && !any_raw; i++)
        if (kf->stops[i].raw_props) any_raw = TRUE;
    if (!any_raw) return NULL;
    ns_css_keyframes *out = g_new0(ns_css_keyframes, 1);
    out->n_stops = kf->n_stops;
    out->stops = g_new(ns_css_keyframe_stop, (gsize)kf->n_stops);
    memcpy(out->stops, kf->stops,
           (gsize)kf->n_stops * sizeof(ns_css_keyframe_stop));
    for (int i = 0; i < out->n_stops; i++) {
        ns_css_keyframe_stop *s = &out->stops[i];
        const char *rawp = s->raw_props;
        s->raw_props = NULL;
        if (!rawp) continue;
        char *resolved = ns_css_substitute_vars(rawp, vars,
                                                g_registered_props, 0);
        if (!resolved) continue;
        ns_css_transform ind = { 0 };
        ns_css_transform list = s->has_transform ? s->transform
                                                 : (ns_css_transform){ 0 };
        char **decls = g_strsplit(resolved, ";", -1);
        for (int d = 0; decls[d]; d++) {
            char *colon = strchr(decls[d], ':');
            if (!colon) continue;
            *colon = '\0';
            char *prop = g_strstrip(decls[d]);
            char *val  = g_strstrip(colon + 1);
            ns_css_value *tv = NULL;
            if (g_ascii_strcasecmp(prop, "transform") == 0) {
                tv = ns_css_parse_transform(val);
                if (tv) list = tv->u.transform;
            } else if (g_ascii_strcasecmp(prop, "translate") == 0) {
                tv = ns_css_parse_translate_prop(val);
            } else if (g_ascii_strcasecmp(prop, "rotate") == 0) {
                tv = ns_css_parse_rotate_prop(val);
            } else if (g_ascii_strcasecmp(prop, "scale") == 0) {
                tv = ns_css_parse_scale_prop(val);
            }
            if (tv && g_ascii_strcasecmp(prop, "transform") != 0 &&
                ind.n_ops < NS_CSS_TRANSFORM_OPS_MAX)
                ind.ops[ind.n_ops++] = tv->u.transform.ops[0];
            if (tv) ns_css_value_free(tv);
        }
        g_strfreev(decls);
        s->raw_props = resolved;
        ns_css_transform merged = ind;
        for (int k = 0; k < list.n_ops &&
                        merged.n_ops < NS_CSS_TRANSFORM_OPS_MAX; k++)
            merged.ops[merged.n_ops++] = list.ops[k];
        if (merged.n_ops > 0) {
            s->transform = merged;
            s->has_transform = TRUE;
        }
    }
    return out;
}

void
ns_css_keyframes_resolved_free(ns_css_keyframes *kf)
{
    if (!kf) return;
    g_free(kf->name);
    for (int i = 0; i < kf->n_stops; i++)
        g_free(kf->stops[i].raw_props);
    g_free(kf->stops);
    g_free(kf);
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

#define NS_CSS_LAYER_NONE INT_MAX

static __thread GHashTable *g_var_adjust_cache;

typedef struct css_candidate {
    guint rule_idx;
    guint selector_idx;
} css_candidate;

typedef struct ns_css_rule_index {
    GHashTable *by_id;
    GHashTable *by_class;
    GHashTable *by_tag;
    GHashTable *by_attr;
    GArray     *universal;
} ns_css_rule_index;

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(ns_css_rule_index) == 40 && sizeof(css_candidate) == 8);
#endif

static void ns_css_rule_index_free(ns_css_rule_index *idx);

gboolean
ns_css_stylesheet_has_container_rules(const ns_css_stylesheet *sh)
{
    return sh && sh->has_container_rules;
}

gboolean
ns_css_stylesheet_has_container_units(const ns_css_stylesheet *sh)
{
    return sh && sh->has_container_units;
}

gboolean
ns_css_stylesheet_has_hover_rules(const ns_css_stylesheet *sh)
{
    return sh && sh->has_hover_rules;
}

gboolean
ns_css_stylesheet_has_active_rules(const ns_css_stylesheet *sh)
{
    return sh && sh->has_active_rules;
}

void
ns_css_stylesheet_free(ns_css_stylesheet *s)
{
    if (!s || s->cached) return;
    if (s->rules) g_ptr_array_free(s->rules, TRUE);
    if (s->imports) g_array_free(s->imports, TRUE);
    if (s->layers) g_hash_table_destroy(s->layers);
    if (s->layer_names) g_ptr_array_free(s->layer_names, TRUE);
    if (s->font_faces) g_array_free(s->font_faces, TRUE);
    if (s->keyframes) g_array_free(s->keyframes, TRUE);
    if (s->property_rules) g_array_free(s->property_rules, TRUE);
    g_clear_pointer(&s->page_rule, g_free);
    g_clear_pointer(&s->resolved_base, g_free);
    if (s->index) ns_css_rule_index_free(s->index);
    s->rules = NULL;
    s->imports = NULL;
    s->layers = NULL;
    s->layer_names = NULL;
    s->font_faces = NULL;
    s->keyframes = NULL;
    s->property_rules = NULL;
    s->index = NULL;
    g_free(s);
}

static void
ns_css_rule_index_free(ns_css_rule_index *idx)
{
    if (!idx) return;
    if (idx->by_id)    g_hash_table_destroy(idx->by_id);
    if (idx->by_class) g_hash_table_destroy(idx->by_class);
    if (idx->by_tag)   g_hash_table_destroy(idx->by_tag);
    if (idx->by_attr)  g_hash_table_destroy(idx->by_attr);
    if (idx->universal) g_array_free(idx->universal, TRUE);
    g_free(idx);
}

static const ns_css_rule_index *
ns_css_rule_index_ensure(const ns_css_stylesheet *sheet)
{
    if (!sheet) return NULL;
    if (!sheet->index)
        ((ns_css_stylesheet *)sheet)->index =
            ns_css_rule_index_build((ns_css_stylesheet *)sheet);
    return sheet->index;
}

#define CSS_ANCESTOR_FILTER_SIZE 4096

static guint8         g_ancestor_filter[CSS_ANCESTOR_FILTER_SIZE];
static gboolean       g_ancestor_filter_active;
static gboolean       g_ancestor_filter_attrs;
static const ns_node *g_ancestor_filter_subject;

static void
css_ancestor_filter_count(guint32 hash, int delta)
{
    guint slots[2] = { hash % CSS_ANCESTOR_FILTER_SIZE,
                       (hash >> 12) % CSS_ANCESTOR_FILTER_SIZE };
    for (guint i = 0; i < G_N_ELEMENTS(slots); i++) {
        guint8 *counter = &g_ancestor_filter[slots[i]];
        if (*counter == G_MAXUINT8) continue;
        if (delta > 0) (*counter)++;
        else if (*counter > 0) (*counter)--;
    }
}

static void
css_ancestor_filter_count_attrs(const ns_node *el, int delta)
{
    for (const ns_attr *a = el->attrs; a; a = a->next)
        if (a->name && a->value)
            css_ancestor_filter_count(
                ns_css_attr_value_hash(a->name, a->value, strlen(a->value)), delta);
}

static void
css_ancestor_filter_update(const ns_node *el, int delta)
{
    if (el->name)
        css_ancestor_filter_count(
            ns_css_identifier_hash('%', el->name, strlen(el->name)), delta);
    const char *id = ns_element_get_attr(el, "id");
    if (id)
        css_ancestor_filter_count(ns_css_identifier_hash('#', id, strlen(id)),
                                  delta);
    const char *cls = ns_element_get_attr(el, "class");
    for (const char *c = cls; c && *c; ) {
        while (*c && is_ws(*c)) c++;
        const char *token = c;
        while (*c && !is_ws(*c)) c++;
        if (c > token)
            css_ancestor_filter_count(
                ns_css_identifier_hash('.', token, (gsize)(c - token)), delta);
    }
    if (g_ancestor_filter_attrs) css_ancestor_filter_count_attrs(el, delta);
}

static gboolean
css_ancestor_filter_rejects(const ns_css_selector *sel)
{
    guint first = g_ancestor_filter_attrs ? 0 : sel->n_ancestor_attr_hashes;
    for (guint i = first; i < sel->n_ancestor_hashes; i++) {
        guint32 hash = sel->ancestor_hashes[i];
        if (!g_ancestor_filter[hash % CSS_ANCESTOR_FILTER_SIZE] ||
            !g_ancestor_filter[(hash >> 12) % CSS_ANCESTOR_FILTER_SIZE])
            return TRUE;
    }
    return FALSE;
}

static ns_style *g_style_pool[16384];
static int g_style_pool_n;

static ns_style *
ns_style_alloc(void)
{
    if (g_style_pool_n > 0) {
        ns_style *s = g_style_pool[--g_style_pool_n];
        memset(s, 0, sizeof(*s));
        return s;
    }
    return g_new0(ns_style, 1);
}

void
ns_style_free(ns_style *s)
{
    if (!s) return;
    if (s->ref > 0) { s->ref--; return; }
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++)
        if (s->values[i]) ns_css_value_free(s->values[i]);
    ns_style_free(s->before);
    ns_style_free(s->after);
    ns_style_free(s->first_letter);
    ns_style_free(s->first_line);
    ns_style_free(s->placeholder);
    ns_style_free(s->selection);
    ns_style_free(s->marker);
    ns_style_free(s->backdrop);
    ns_style_free(s->file_selector_button);
    ns_style_free(s->hidden_before);
    ns_style_free(s->hidden_after);
    if (s->vars) ns_var_map_unref(s->vars);
    if (g_style_pool_n < (int)G_N_ELEMENTS(g_style_pool))
        g_style_pool[g_style_pool_n++] = s;
    else
        g_free(s);
}

const char *
ns_style_keyword(const ns_style *s, ns_css_prop p)
{
    if (!s) return NULL;
    ns_css_value *v = s->values[p];
    if (!v || v->kind != NS_CSS_V_KEYWORD) return NULL;
    return prop_is_alignment(p) ? ns_css_alignment_base(v->u.keyword)
                                : v->u.keyword;
}

const char *
ns_style_overflow_keyword(const ns_style *s, ns_css_prop axis)
{
    const char *value = ns_style_keyword(s, axis);
    if (!value) value = ns_style_keyword(s, NS_CSS_OVERFLOW);
    if (!value) value = "visible";
    ns_css_prop other_axis = axis == NS_CSS_OVERFLOW_X
        ? NS_CSS_OVERFLOW_Y : NS_CSS_OVERFLOW_X;
    const char *other = ns_style_keyword(s, other_axis);
    if (!other) other = ns_style_keyword(s, NS_CSS_OVERFLOW);
    if (!other) other = "visible";
    gboolean other_scrollable =
        g_ascii_strcasecmp(other, "visible") != 0 &&
        g_ascii_strcasecmp(other, "clip") != 0;
    if (other_scrollable && g_ascii_strcasecmp(value, "visible") == 0)
        return "auto";
    if (other_scrollable && g_ascii_strcasecmp(value, "clip") == 0)
        return "hidden";
    return value;
}

static GHashTable *g_incr_exclude;

gboolean
ns_style_prop_from_currentcolor(const ns_style *s, int prop)
{
    static const int color_props[] = {
        NS_CSS_BACKGROUND_COLOR,
        NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
        NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
        NS_CSS_OUTLINE_COLOR,
        NS_CSS_TEXT_DECORATION_COLOR,
        NS_CSS_COLUMN_RULE_COLOR,
        NS_CSS_ACCENT_COLOR,
        NS_CSS_CARET_COLOR,
        NS_CSS_FILL,
        NS_CSS_STROKE,
        NS_CSS_STOP_COLOR,
    };
    if (!s) return FALSE;
    for (gsize i = 0; i < G_N_ELEMENTS(color_props); i++)
        if (color_props[i] == prop) return (s->currentcolor_bits >> i) & 1u;
    return FALSE;
}

void
ns_css_incremental_exclude(const void *node, gboolean exclude)
{
    if (!node) return;
    if (!g_incr_exclude)
        g_incr_exclude = g_hash_table_new(g_direct_hash, g_direct_equal);
    if (exclude) g_hash_table_add(g_incr_exclude, (gpointer)node);
    else g_hash_table_remove(g_incr_exclude, node);
}

gboolean
ns_css_prop_affects_layout(int prop)
{
    switch (prop) {
    case NS_CSS_OPACITY:
    case NS_CSS_COLOR:
    case NS_CSS_BACKGROUND_COLOR:
    case NS_CSS_TRANSFORM:
    case NS_CSS_VISIBILITY:
    case NS_CSS_BOX_SHADOW:
    case NS_CSS_TEXT_SHADOW:
    case NS_CSS_FILTER:
    case NS_CSS_BORDER_TOP_COLOR:
    case NS_CSS_BORDER_RIGHT_COLOR:
    case NS_CSS_BORDER_BOTTOM_COLOR:
    case NS_CSS_BORDER_LEFT_COLOR:
    case NS_CSS_OUTLINE_COLOR:
        return FALSE;
    default:
        return TRUE;
    }
}

typedef enum ns_css_origin {
    NS_CSS_ORIGIN_UA,
    NS_CSS_ORIGIN_PRESENTATIONAL,
    NS_CSS_ORIGIN_AUTHOR,
} ns_css_origin;

typedef struct match_entry {
    int          origin;
    int          spec_a, spec_b, spec_c;
    int          sheet_index;
    int          layer_order;
    int          scope_order;
    int          source_order;
    int          decl_order;
    gboolean     important;
    gboolean     inline_style;
    const ns_css_rule *rule;
    ns_css_value *value;
    ns_css_prop  prop;
} match_entry;

typedef struct var_match {
    int origin;
    int spec_a, spec_b, spec_c;
    int sheet_index;
    int layer_order;
    int scope_order;
    int source_order;
    int decl_order;
    gboolean important;
    gboolean inline_style;
    const ns_css_rule *rule;
    const char *name;
    const char *text;
} var_match;

typedef struct pending_match {
    int origin;
    int spec_a, spec_b, spec_c;
    int sheet_index;
    int layer_order;
    int scope_order;
    int source_order;
    int decl_order_base;
    gboolean inline_style;
    const ns_css_rule *rule;
    ns_css_pending_decl *pd;
} pending_match;

#define NS_CSS_DECL_SLOT_SPAN 64

static int
css_decl_slot(guint decl_index)
{
    return (int)decl_index * NS_CSS_DECL_SLOT_SPAN;
}

static int
css_pending_decl_slot(const ns_css_pending_decl *pd)
{
    int rank = pd->decl_rank < NS_CSS_DECL_SLOT_SPAN - 1
        ? pd->decl_rank : NS_CSS_DECL_SLOT_SPAN - 2;
    return css_decl_slot((guint)pd->decl_index) - NS_CSS_DECL_SLOT_SPAN + 1 + rank;
}

static int
css_layer_cmp(int a, int b, gboolean important)
{
    if (a == b) return 0;
    if (important) return a > b ? -1 : 1;
    return a < b ? -1 : 1;
}

static gboolean
css_same_revert_origin(int rollback_origin, int candidate_origin)
{
    if (rollback_origin == NS_CSS_ORIGIN_AUTHOR)
        return candidate_origin == NS_CSS_ORIGIN_AUTHOR ||
               candidate_origin == NS_CSS_ORIGIN_PRESENTATIONAL;
    return rollback_origin == candidate_origin;
}

static int
css_layer_rank_for(GHashTable *layer_ranks, const char *layer_name)
{
    if (!layer_name || !layer_ranks) return NS_CSS_LAYER_NONE;
    gpointer v = g_hash_table_lookup(layer_ranks, layer_name);
    return v ? GPOINTER_TO_INT(v) - 1 : NS_CSS_LAYER_NONE;
}

static void
css_layer_rank_add_sheet(GHashTable *layer_ranks,
                         const ns_css_stylesheet *sheet)
{
    if (!layer_ranks || !sheet || !sheet->layer_names) return;
    for (guint i = 0; i < sheet->layer_names->len; i++) {
        const char *name = g_ptr_array_index(sheet->layer_names, i);
        if (!name || g_hash_table_contains(layer_ranks, name)) continue;
        int rank = (int)g_hash_table_size(layer_ranks);
        g_hash_table_insert(layer_ranks, g_strdup(name),
                            GINT_TO_POINTER(rank + 1));
    }
}

static void
css_layer_prefix_note(GHashTable *first, const char *name, int index)
{
    const char *dot = name;
    for (;;) {
        dot = strchr(dot, '.');
        gsize len = dot ? (gsize)(dot - name) : strlen(name);
        char *prefix = g_strndup(name, len);
        gpointer prev = g_hash_table_lookup(first, prefix);
        if (prev && GPOINTER_TO_INT(prev) - 1 <= index)
            g_free(prefix);
        else
            g_hash_table_insert(first, prefix, GINT_TO_POINTER(index + 1));
        if (!dot) break;
        dot++;
    }
}

static int
css_layer_first_index(GHashTable *first, const char *name, gsize len)
{
    char *prefix = g_strndup(name, len);
    gpointer v = g_hash_table_lookup(first, prefix);
    g_free(prefix);
    return v ? GPOINTER_TO_INT(v) - 1 : INT_MAX;
}

static int
css_layer_name_cmp(gconstpointer a_, gconstpointer b_, gpointer user_data)
{
    GHashTable *first = user_data;
    const char *a = *(const char *const *)a_;
    const char *b = *(const char *const *)b_;
    const char *ap = a, *bp = b;
    for (;;) {
        const char *ad = strchr(ap, '.');
        const char *bd = strchr(bp, '.');
        gsize al = ad ? (gsize)(ad - ap) : strlen(ap);
        gsize bl = bd ? (gsize)(bd - bp) : strlen(bp);
        if (al != bl || memcmp(ap, bp, al) != 0) {
            int ai = css_layer_first_index(first, a, (gsize)(ap - a) + al);
            int bi = css_layer_first_index(first, b, (gsize)(bp - b) + bl);
            if (ai != bi) return ai < bi ? -1 : 1;
            return strcmp(a, b);
        }
        if (!ad && !bd) return 0;
        if (!ad) return 1;
        if (!bd) return -1;
        ap = ad + 1;
        bp = bd + 1;
    }
}

static void
css_layer_ranks_finalize(GHashTable *layer_ranks)
{
    if (!layer_ranks || g_hash_table_size(layer_ranks) == 0) return;
    GHashTable *first = g_hash_table_new_full(g_str_hash, g_str_equal,
                                              g_free, NULL);
    GHashTableIter it;
    gpointer k, v;
    g_hash_table_iter_init(&it, layer_ranks);
    while (g_hash_table_iter_next(&it, &k, &v))
        css_layer_prefix_note(first, k, GPOINTER_TO_INT(v) - 1);

    GPtrArray *names = g_ptr_array_new();
    g_hash_table_iter_init(&it, first);
    while (g_hash_table_iter_next(&it, &k, NULL))
        g_ptr_array_add(names, k);
    g_ptr_array_sort_with_data(names, css_layer_name_cmp, first);

    g_hash_table_remove_all(layer_ranks);
    for (guint i = 0; i < names->len; i++)
        g_hash_table_insert(layer_ranks,
                            g_strdup(g_ptr_array_index(names, i)),
                            GINT_TO_POINTER((int)i + 1));
    g_ptr_array_free(names, TRUE);
    g_hash_table_destroy(first);
}

static int
match_cmp(gconstpointer a_, gconstpointer b_)
{
    const match_entry *a = a_;
    const match_entry *b = b_;
    if (a->important != b->important) return a->important ? 1 : -1;
    if (a->origin    != b->origin)
        return a->important ? (a->origin > b->origin ? -1 : 1)
                            : (a->origin < b->origin ? -1 : 1);
    if (a->inline_style != b->inline_style) return a->inline_style ? 1 : -1;
    int layer_cmp = css_layer_cmp(a->layer_order, b->layer_order, a->important);
    if (layer_cmp != 0) return layer_cmp;
    if (a->spec_a    != b->spec_a)    return a->spec_a < b->spec_a ? -1 : 1;
    if (a->spec_b    != b->spec_b)    return a->spec_b < b->spec_b ? -1 : 1;
    if (a->spec_c    != b->spec_c)    return a->spec_c < b->spec_c ? -1 : 1;
    if (a->scope_order != b->scope_order)
        return a->scope_order < b->scope_order ? -1 : 1;
    if (a->sheet_index  != b->sheet_index)
        return a->sheet_index < b->sheet_index ? -1 : 1;
    if (a->source_order != b->source_order)
        return a->source_order < b->source_order ? -1 : 1;
    return a->decl_order < b->decl_order ? -1 : 1;
}

#define CSS_GATHER_DESTS_MAX (NS_CSS_PE_FILE_SELECTOR_BUTTON + 1)

typedef struct css_rule_match_accum {
    guint epoch;
    int layer_order;
    gboolean any[CSS_GATHER_DESTS_MAX];
    int spec_a[CSS_GATHER_DESTS_MAX];
    int spec_b[CSS_GATHER_DESTS_MAX];
    int spec_c[CSS_GATHER_DESTS_MAX];
    int scope_order[CSS_GATHER_DESTS_MAX];
} css_rule_match_accum;

static __thread css_candidate *g_cand_pool = NULL;
static __thread guint g_cand_pool_cap = 0;
static __thread css_rule_match_accum *g_rule_accum = NULL;
static __thread guint g_rule_accum_cap = 0;
static __thread guint *g_rule_matched = NULL;
static __thread guint g_rule_matched_cap = 0;
static __thread guint g_rule_match_epoch = 0;

typedef struct {
    const ns_css_rule     *rule;
    const ns_css_selector *selector;
    const ns_node         *element;
    ns_css_pseudo_element  pseudo;
} selector_cache_key;

typedef struct {
    int      scope_order;
    gboolean matched;
} selector_cache_value;

#define NS_SELECTOR_CACHE_MAX 262144

static __thread GHashTable *g_selector_cache;

static guint
selector_cache_hash(gconstpointer data)
{
    const selector_cache_key *key = data;
    guintptr h = (guintptr)key->rule;
    h ^= (guintptr)key->selector * 0x9e3779b1u;
    h ^= (guintptr)key->element * 0x85ebca6bu;
    h ^= (guintptr)key->pseudo * 0xc2b2ae35u;
    return (guint)((guint64)h ^ ((guint64)h >> 32));
}

static gboolean
selector_cache_equal(gconstpointer a, gconstpointer b)
{
    const selector_cache_key *left = a;
    const selector_cache_key *right = b;
    return left->rule == right->rule &&
           left->selector == right->selector &&
           left->element == right->element &&
           left->pseudo == right->pseudo;
}

void
ns_css_selector_cache_begin(void)
{
    g_clear_pointer(&g_selector_cache, g_hash_table_destroy);
    g_selector_cache = g_hash_table_new_full(selector_cache_hash,
                                              selector_cache_equal,
                                              g_free, g_free);
}

void
ns_css_selector_cache_end(void)
{
    g_clear_pointer(&g_selector_cache, g_hash_table_destroy);
}

static gboolean
selector_cache_lookup(const ns_css_rule *rule,
                      const ns_css_selector *selector,
                      const ns_node *element,
                      ns_css_pseudo_element pseudo,
                      gboolean *matched, int *scope_order)
{
    if (!g_selector_cache) return FALSE;
    selector_cache_key probe = { rule, selector, element, pseudo };
    gpointer cached;
    if (!g_hash_table_lookup_extended(g_selector_cache, &probe, NULL,
                                      &cached))
        return FALSE;
    selector_cache_value *value = cached;
    *matched = value->matched;
    *scope_order = value->scope_order;
    return TRUE;
}

static void
selector_cache_insert(const ns_css_rule *rule,
                      const ns_css_selector *selector,
                      const ns_node *element,
                      ns_css_pseudo_element pseudo,
                      gboolean matched, int scope_order)
{
    if (!g_selector_cache ||
        g_hash_table_size(g_selector_cache) >= NS_SELECTOR_CACHE_MAX)
        return;
    selector_cache_key *key = g_new(selector_cache_key, 1);
    *key = (selector_cache_key){ rule, selector, element, pseudo };
    selector_cache_value *value = g_new(selector_cache_value, 1);
    *value = (selector_cache_value){ scope_order, matched };
    g_hash_table_insert(g_selector_cache, key, value);
}

static GArray *
css_index_lookup_ci(GHashTable *table, const char *name, gsize nlen)
{
    for (gsize i = 0; i < nlen; i++)
        if (name[i] >= 'A' && name[i] <= 'Z') {
            char small[64];
            char *key;
            if (nlen < sizeof(small)) {
                for (gsize j = 0; j < nlen; j++) small[j] = g_ascii_tolower(name[j]);
                small[nlen] = '\0'; key = small;
            } else {
                key = g_ascii_strdown(name, (gssize)nlen);
            }
            GArray *bucket = g_hash_table_lookup(table, key);
            if (key != small) g_free(key);
            return bucket;
        }
    return g_hash_table_lookup(table, name);
}

typedef struct {
    ns_css_pseudo_element pe;
    GArray *out;
    GArray *var_out;
    GArray *pending_out;
} gather_dest;

static void
gather_matches_multi(const ns_css_stylesheet *sheet, int origin,
                     int sheet_index, const ns_node *el,
                     gather_dest *dests, guint n_dests,
                     GHashTable *layer_ranks)
{
    if (!sheet) return;
    const ns_css_rule_index *idx = ns_css_rule_index_ensure(sheet);
    if (!idx) return;

    css_candidate *cands = g_cand_pool;
    guint cand_cap = g_cand_pool_cap;
    guint cand_n = 0;
    #define CAND_PUSH_ARR(_arr) do { \
        if ((_arr)) { \
            guint push_len = (_arr)->len; \
            if (cand_n > G_MAXUINT - push_len) break; \
            if (cand_n + push_len > cand_cap) { \
                guint new_cap = cand_cap < 64 ? 64 : cand_cap; \
                while (cand_n + push_len > new_cap) { \
                    if (new_cap > G_MAXUINT / 2) { new_cap = G_MAXUINT; break; } \
                    new_cap *= 2; \
                } \
                if (new_cap > G_MAXUINT / sizeof(css_candidate)) break; \
                cands = g_renew(css_candidate, cands, new_cap); \
                cand_cap = new_cap; \
                g_cand_pool = cands; \
                g_cand_pool_cap = cand_cap; \
            } \
            if (push_len) memcpy(cands + cand_n, (_arr)->data, push_len * sizeof(css_candidate)); \
            cand_n += push_len; \
        } \
    } while (0)

    if (el && el->kind == NS_NODE_ELEMENT) {
        const char *id = ns_element_get_attr(el, "id");
        if (id && *id) {
            GArray *bucket = g_hash_table_lookup(idx->by_id, id);
            CAND_PUSH_ARR(bucket);
        }
        const char *cls = ns_element_get_attr(el, "class");
        if (cls && *cls) {
            const char *s = cls;
            while (*s) {
                while (*s && (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r' || *s == '\f')) s++;
                const char *tok = s;
                while (*s && !(*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r' || *s == '\f')) s++;
                if (s == tok) break;
                gsize tlen = (gsize)(s - tok);
                char small[64];
                char *key;
                if (tlen < sizeof(small)) {
                    memcpy(small, tok, tlen); small[tlen] = '\0'; key = small;
                } else {
                    key = g_strndup(tok, tlen);
                }
                GArray *bucket = g_hash_table_lookup(idx->by_class, key);
                if (key != small) g_free(key);
                CAND_PUSH_ARR(bucket);
            }
        }
        if (el->name && *el->name) {
            CAND_PUSH_ARR(css_index_lookup_ci(idx->by_tag, el->name,
                                              strlen(el->name)));
        }
        if (idx->by_attr && g_hash_table_size(idx->by_attr) > 0) {
            for (const ns_attr *a = el->attrs; a; a = a->next) {
                if (!a->name) continue;
                CAND_PUSH_ARR(css_index_lookup_ci(idx->by_attr, a->name,
                                                  strlen(a->name)));
            }
        }
    }
    CAND_PUSH_ARR(idx->universal);
    #undef CAND_PUSH_ARR

    guint n_rules = sheet->rules ? sheet->rules->len : 0;
    if (g_rule_accum_cap < n_rules) {
        guint new_cap = g_rule_accum_cap < 64 ? 64 : g_rule_accum_cap;
        while (new_cap < n_rules) {
            if (new_cap > G_MAXUINT / 2) { new_cap = n_rules; break; }
            new_cap *= 2;
        }
        g_rule_accum = g_renew(css_rule_match_accum, g_rule_accum, new_cap);
        memset(g_rule_accum + g_rule_accum_cap, 0,
               (gsize)(new_cap - g_rule_accum_cap) * sizeof(css_rule_match_accum));
        g_rule_accum_cap = new_cap;
    }
    if (g_rule_matched_cap < n_rules) {
        guint new_cap = g_rule_matched_cap < 64 ? 64 : g_rule_matched_cap;
        while (new_cap < n_rules) {
            if (new_cap > G_MAXUINT / 2) { new_cap = n_rules; break; }
            new_cap *= 2;
        }
        g_rule_matched = g_renew(guint, g_rule_matched, new_cap);
        g_rule_matched_cap = new_cap;
    }
    if (++g_rule_match_epoch == 0) {
        memset(g_rule_accum, 0,
               (gsize)g_rule_accum_cap * sizeof(css_rule_match_accum));
        g_rule_match_epoch = 1;
    }

    int dest_of_pe[CSS_GATHER_DESTS_MAX];
    for (gsize i = 0; i < G_N_ELEMENTS(dest_of_pe); i++) dest_of_pe[i] = -1;
    for (guint dd = 0; dd < n_dests; dd++)
        if ((gsize)dests[dd].pe < G_N_ELEMENTS(dest_of_pe))
            dest_of_pe[dests[dd].pe] = (int)dd;

    gboolean ancestor_filter_usable = g_ancestor_filter_active &&
                                      el == g_ancestor_filter_subject &&
                                      !ns_css_match_scope();
    guint matched_n = 0;
    for (guint ci = 0; ci < cand_n; ci++) {
        css_candidate cand = cands[ci];
        guint ri = cand.rule_idx;
        if (ri >= n_rules) continue;
        ns_css_rule *r = g_ptr_array_index(sheet->rules, ri);
        if (!r || cand.selector_idx >= r->selectors->len) continue;
        if (r->container_condition &&
            !ns_css_container_rule_matches(r->container_condition,
                                           &r->container_query))
            continue;
        ns_css_selector *cand_sel =
            g_ptr_array_index(r->selectors, cand.selector_idx);
        if (ancestor_filter_usable && cand_sel &&
            (!r->scopes || r->scopes->len == 0) &&
            css_ancestor_filter_rejects(cand_sel))
            continue;
        guint dd_first = 0, dd_last = n_dests - 1;
        if (cand_sel) {
            if ((gsize)cand_sel->pseudo_element >= G_N_ELEMENTS(dest_of_pe))
                continue;
            int dd_only = dest_of_pe[cand_sel->pseudo_element];
            if (dd_only < 0) continue;
            dd_first = dd_last = (guint)dd_only;
        }
        for (guint dd = dd_first; dd <= dd_last; dd++) {
            gather_dest *dst = &dests[dd];
            ns_css_pseudo_element pe = dst->pe;
            if (pe != NS_CSS_PE_NONE && !(r->pe_mask & (1u << pe)))
                continue;
            ns_css_selector *sel = cand_sel;
            if (sel && sel->pseudo_element != pe) continue;
            int scope_order = 0;
            gboolean matched = FALSE;
            if (!selector_cache_lookup(r, sel, el, pe, &matched,
                                       &scope_order)) {
                matched = ns_css_rule_selector_matches(r, sel, el, pe,
                                                       &scope_order);
                selector_cache_insert(r, sel, el, pe, matched, scope_order);
            }
            if (!matched) continue;
            if (r->container_condition) ns_css_container_features_note();
            css_rule_match_accum *acc = &g_rule_accum[ri];
            if (acc->epoch != g_rule_match_epoch) {
                acc->epoch = g_rule_match_epoch;
                acc->layer_order = INT_MIN;
                memset(acc->any, 0, sizeof acc->any);
                if (matched_n < g_rule_matched_cap)
                    g_rule_matched[matched_n++] = ri;
            }
            if (!acc->any[dd] || sel->spec_a > acc->spec_a[dd] ||
                (sel->spec_a == acc->spec_a[dd] &&
                 sel->spec_b > acc->spec_b[dd]) ||
                (sel->spec_a == acc->spec_a[dd] &&
                 sel->spec_b == acc->spec_b[dd] &&
                 sel->spec_c > acc->spec_c[dd])) {
                acc->any[dd] = TRUE;
                acc->spec_a[dd] = sel->spec_a;
                acc->spec_b[dd] = sel->spec_b;
                acc->spec_c[dd] = sel->spec_c;
                acc->scope_order[dd] = scope_order;
            } else if (sel->spec_a == acc->spec_a[dd] &&
                       sel->spec_b == acc->spec_b[dd] &&
                       sel->spec_c == acc->spec_c[dd] &&
                       scope_order > acc->scope_order[dd]) {
                acc->scope_order[dd] = scope_order;
            }
        }
    }

    for (guint mi = 0; mi < matched_n; mi++) {
        guint ri = g_rule_matched[mi];
        ns_css_rule *r = g_ptr_array_index(sheet->rules, ri);
        css_rule_match_accum *acc = &g_rule_accum[ri];
        if (acc->layer_order == INT_MIN)
            acc->layer_order = css_layer_rank_for(layer_ranks, r->layer_name);
        for (guint dd = 0; dd < n_dests; dd++) {
            if (!acc->any[dd]) continue;
            gather_dest *dst = &dests[dd];
            for (guint di = 0; di < r->decls->len; di++) {
                ns_css_decl *d = &g_array_index(r->decls, ns_css_decl, di);
                match_entry e = {
                    .origin = origin,
                    .spec_a = acc->spec_a[dd],
                    .spec_b = acc->spec_b[dd],
                    .spec_c = acc->spec_c[dd],
                    .sheet_index = sheet_index,
                    .layer_order = acc->layer_order,
                    .scope_order = acc->scope_order[dd],
                    .source_order = r->source_order,
                    .decl_order = css_decl_slot(di),
                    .important = d->important,
                    .rule = r,
                    .value = d->value,
                    .prop  = d->prop,
                };
                g_array_append_val(dst->out, e);
            }
            if (dst->var_out && r->vars) {
                GHashTableIter it;
                gpointer k, v;
                int decl_i = 0;
                g_hash_table_iter_init(&it, r->vars);
                while (g_hash_table_iter_next(&it, &k, &v)) {
                    var_match vm = {
                        .origin = origin,
                        .spec_a = acc->spec_a[dd],
                        .spec_b = acc->spec_b[dd],
                        .spec_c = acc->spec_c[dd],
                        .sheet_index = sheet_index,
                        .layer_order = acc->layer_order,
                        .scope_order = acc->scope_order[dd],
                        .source_order = r->source_order,
                        .decl_order = decl_i++,
                        .important = r->var_important &&
                                     g_hash_table_contains(r->var_important, k),
                        .rule = r,
                        .name = (const char *)k,
                        .text = (const char *)v,
                    };
                    g_array_append_val(dst->var_out, vm);
                }
            }
            if (dst->pending_out && r->pending) {
                for (guint pi = 0; pi < r->pending->len; pi++) {
                    ns_css_pending_decl *pd =
                        &g_array_index(r->pending, ns_css_pending_decl, pi);
                    pending_match pm = {
                        .origin = origin,
                        .spec_a = acc->spec_a[dd],
                        .spec_b = acc->spec_b[dd],
                        .spec_c = acc->spec_c[dd],
                        .sheet_index = sheet_index,
                        .layer_order = acc->layer_order,
                        .scope_order = acc->scope_order[dd],
                        .source_order = r->source_order,
                        .decl_order_base = css_pending_decl_slot(pd),
                        .rule = r,
                        .pd = pd,
                    };
                    g_array_append_val(dst->pending_out, pm);
                }
            }
        }
    }
    (void)cands;
}

static int
var_match_cmp(gconstpointer a_, gconstpointer b_)
{
    const var_match *a = a_;
    const var_match *b = b_;
    if (a->important != b->important) return a->important ? 1 : -1;
    if (a->origin    != b->origin)
        return a->important ? (a->origin > b->origin ? -1 : 1)
                            : (a->origin < b->origin ? -1 : 1);
    if (a->inline_style != b->inline_style) return a->inline_style ? 1 : -1;
    int layer_cmp = css_layer_cmp(a->layer_order, b->layer_order, a->important);
    if (layer_cmp != 0) return layer_cmp;
    if (a->spec_a    != b->spec_a)    return a->spec_a < b->spec_a ? -1 : 1;
    if (a->spec_b    != b->spec_b)    return a->spec_b < b->spec_b ? -1 : 1;
    if (a->spec_c    != b->spec_c)    return a->spec_c < b->spec_c ? -1 : 1;
    if (a->scope_order != b->scope_order)
        return a->scope_order < b->scope_order ? -1 : 1;
    if (a->sheet_index  != b->sheet_index)
        return a->sheet_index < b->sheet_index ? -1 : 1;
    if (a->source_order != b->source_order)
        return a->source_order < b->source_order ? -1 : 1;
    return a->decl_order < b->decl_order ? -1 : 1;
}

static const var_match *
var_rollback_match(GArray *matches, gint before, const var_match *rollback,
                   ns_custom_prop_wide kind)
{
    gboolean layer_only = kind == NS_CUSTOM_WIDE_REVERT_LAYER;
    gboolean rule_only = kind == NS_CUSTOM_WIDE_REVERT_RULE;
    for (gint j = before; j >= 0; j--) {
        var_match *prev = &g_array_index(matches, var_match, (guint)j);
        if (!prev->name || strcmp(prev->name, rollback->name) != 0) continue;
        if (rule_only) {
            if (prev->rule == rollback->rule) continue;
        } else if (layer_only) {
            if (prev->origin == rollback->origin) {
                if (rollback->inline_style) {
                    if (prev->inline_style)
                        continue;
                } else if (rollback->layer_order == NS_CSS_LAYER_NONE) {
                    if (prev->layer_order == NS_CSS_LAYER_NONE)
                        continue;
                } else if (prev->layer_order >= rollback->layer_order) {
                    continue;
                }
            }
        } else if (css_same_revert_origin(rollback->origin, prev->origin)) {
            continue;
        }
        ns_custom_prop_wide prev_kind = ns_css_custom_value_wide_kind(prev->text);
        if (prev_kind == NS_CUSTOM_WIDE_REVERT ||
            prev_kind == NS_CUSTOM_WIDE_REVERT_LAYER ||
            prev_kind == NS_CUSTOM_WIDE_REVERT_RULE)
            return var_rollback_match(matches, j - 1, prev, prev_kind);
        return prev;
    }
    return NULL;
}

static const var_match *
var_resolved_match(GArray *matches, guint index)
{
    var_match *match = &g_array_index(matches, var_match, index);
    ns_custom_prop_wide kind = ns_css_custom_value_wide_kind(match->text);
    if (kind == NS_CUSTOM_WIDE_REVERT ||
        kind == NS_CUSTOM_WIDE_REVERT_LAYER ||
        kind == NS_CUSTOM_WIDE_REVERT_RULE)
        return var_rollback_match(matches, (gint)index - 1, match, kind);
    return match;
}

static int
pending_match_cmp(gconstpointer a_, gconstpointer b_)
{
    const pending_match *a = a_;
    const pending_match *b = b_;
    gboolean ai = a->pd && a->pd->important;
    gboolean bi = b->pd && b->pd->important;
    if (ai != bi) return ai ? 1 : -1;
    if (a->origin    != b->origin)
        return ai ? (a->origin > b->origin ? -1 : 1)
                  : (a->origin < b->origin ? -1 : 1);
    if (a->inline_style != b->inline_style) return a->inline_style ? 1 : -1;
    int layer_cmp = css_layer_cmp(a->layer_order, b->layer_order, ai);
    if (layer_cmp != 0) return layer_cmp;
    if (a->spec_a    != b->spec_a)    return a->spec_a < b->spec_a ? -1 : 1;
    if (a->spec_b    != b->spec_b)    return a->spec_b < b->spec_b ? -1 : 1;
    if (a->spec_c    != b->spec_c)    return a->spec_c < b->spec_c ? -1 : 1;
    if (a->scope_order != b->scope_order)
        return a->scope_order < b->scope_order ? -1 : 1;
    if (a->sheet_index  != b->sheet_index)
        return a->sheet_index < b->sheet_index ? -1 : 1;
    return a->source_order < b->source_order ? -1 : 1;
}

static void
css_collect_property_rules(GHashTable *reg, const ns_css_stylesheet *sh)
{
    if (!reg || !sh || !sh->property_rules) return;
    for (guint i = 0; i < sh->property_rules->len; i++) {
        ns_css_property_rule *pr =
            &g_array_index(sh->property_rules, ns_css_property_rule, i);
        if (pr->name) g_hash_table_replace(reg, pr->name, pr);
    }
}

static GHashTable *
var_prefill_plain_values(GHashTable *own, GArray *matches)
{
    GHashTable *last = g_hash_table_new_full(g_str_hash, g_str_equal,
                                             g_free, NULL);
    GHashTable *prefilled = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                  g_free, NULL);
    for (guint i = 0; i < matches->len; i++) {
        var_match *vm = &g_array_index(matches, var_match, i);
        if (!vm->name || !vm->text) continue;
        gboolean plain = !strstr(vm->text, "var(") &&
            ns_css_custom_value_wide_kind(vm->text) == NS_CUSTOM_WIDE_NONE &&
            !(g_registered_props &&
              g_hash_table_lookup(g_registered_props, vm->name));
        gpointer seen = g_hash_table_lookup(last, vm->name);
        guint state = plain && (!seen || GPOINTER_TO_UINT(seen) & 1u)
            ? ((i + 1) << 1) | 1u : 2u;
        g_hash_table_replace(last, g_strdup(vm->name), GUINT_TO_POINTER(state));
    }
    GHashTableIter it;
    gpointer k, v;
    g_hash_table_iter_init(&it, last);
    while (g_hash_table_iter_next(&it, &k, &v)) {
        guint state = GPOINTER_TO_UINT(v);
        if (!(state & 1u)) continue;
        var_match *vm = &g_array_index(matches, var_match, (state >> 1) - 1);
        g_hash_table_replace(own, g_strdup(vm->name), g_strdup(vm->text));
        g_hash_table_add(prefilled, g_strdup(vm->name));
    }
    g_hash_table_destroy(last);
    return prefilled;
}

static void
var_map_apply_unregistered(GHashTable *own, const ns_var_map *parent,
                           GArray *matches, guint index)
{
    var_match *current = &g_array_index(matches, var_match, index);
    const var_match *resolved = var_resolved_match(matches, index);
    if (!resolved) {
        g_hash_table_remove(own, current->name);
        return;
    }
    const char *value_text = resolved->text;
    ns_custom_prop_wide kind = ns_css_custom_value_wide_kind(value_text);
    char *expanded = NULL;
    if (kind == NS_CUSTOM_WIDE_NONE && strstr(resolved->text, "var(")) {
        ns_var_map scope = { .ref = 1, .own = own,
                             .parent = (ns_var_map *)parent };
        expanded = ns_css_substitute_vars(value_text, &scope,
                                          g_registered_props, 0);
        kind = ns_css_custom_value_wide_kind(expanded);
    }
    if (kind == NS_CUSTOM_WIDE_REVERT ||
        kind == NS_CUSTOM_WIDE_REVERT_LAYER) {
        resolved = var_rollback_match(matches, (gint)index - 1, current, kind);
        if (resolved) {
            value_text = resolved->text;
            kind = ns_css_custom_value_wide_kind(value_text);
        }
    }
    if (kind == NS_CUSTOM_WIDE_INHERIT || kind == NS_CUSTOM_WIDE_UNSET ||
        kind == NS_CUSTOM_WIDE_REVERT ||
        kind == NS_CUSTOM_WIDE_REVERT_LAYER) {
        g_hash_table_remove(own, current->name);
    } else if (kind == NS_CUSTOM_WIDE_INITIAL) {
        g_hash_table_replace(own, g_strdup(current->name), g_strdup("initial"));
    } else {
        g_hash_table_replace(own, g_strdup(current->name),
                             g_strdup(value_text));
    }
    g_free(expanded);
}

static void
var_map_restore_default(GHashTable *vars, const ns_var_map *parent,
                        const char *name, const ns_css_property_rule *pr,
                        gboolean inherit)
{
    const char *parent_value = inherit && parent
        ? ns_var_map_lookup(parent, name) : NULL;
    if (parent_value) {
        g_hash_table_replace(vars, g_strdup(name), g_strdup(parent_value));
    } else if (pr && pr->has_initial) {
        g_hash_table_replace(vars, g_strdup(name),
                             g_strdup(pr->initial_value));
    } else if (inherit || ns_var_map_lookup(parent, name)) {
        g_hash_table_replace(vars, g_strdup(name), g_strdup("initial"));
    } else {
        g_hash_table_remove(vars, name);
    }
}

static void
var_map_reset_registered(GHashTable *own, const ns_var_map *parent)
{
    GHashTableIter it;
    gpointer k, v;
    g_hash_table_iter_init(&it, g_registered_props);
    while (g_hash_table_iter_next(&it, &k, &v)) {
        const ns_css_property_rule *pr = v;
        const char *inherited = ns_var_map_lookup(parent, k);
        const char *start = pr->inherits && inherited ? inherited
                          : pr->has_initial ? pr->initial_value : NULL;
        if (start) {
            if (g_strcmp0(inherited, start) != 0)
                g_hash_table_replace(own, g_strdup(k), g_strdup(start));
        } else if (inherited && g_ascii_strcasecmp(inherited, "initial") != 0) {
            g_hash_table_replace(own, g_strdup(k), g_strdup("initial"));
        }
    }
}

static void
var_map_apply_registered(GHashTable *vars, const ns_var_map *parent,
                         GArray *matches, guint index)
{
    var_match *current = &g_array_index(matches, var_match, index);
    const var_match *resolved = var_resolved_match(matches, index);
    ns_css_property_rule *pr = g_registered_props
        ? g_hash_table_lookup(g_registered_props, current->name) : NULL;
    if (!resolved) {
        var_map_restore_default(vars, parent, current->name, pr,
                                !pr || pr->inherits);
        return;
    }
    const char *value_text = resolved->text;
    ns_custom_prop_wide kind = ns_css_custom_value_wide_kind(value_text);
    char *expanded = NULL;
    if (kind == NS_CUSTOM_WIDE_NONE && strstr(resolved->text, "var(")) {
        ns_var_map scope = { .ref = 1, .own = vars,
                             .parent = (ns_var_map *)parent };
        expanded = ns_css_substitute_vars(value_text, &scope,
                                          g_registered_props, 0);
        kind = ns_css_custom_value_wide_kind(expanded);
    }
    if (kind == NS_CUSTOM_WIDE_REVERT ||
        kind == NS_CUSTOM_WIDE_REVERT_LAYER) {
        resolved = var_rollback_match(matches, (gint)index - 1, current, kind);
        if (resolved) {
            value_text = resolved->text;
            kind = ns_css_custom_value_wide_kind(value_text);
        }
    }
    if (kind == NS_CUSTOM_WIDE_INHERIT) {
        var_map_restore_default(vars, parent, current->name, pr, TRUE);
    } else if (kind == NS_CUSTOM_WIDE_UNSET) {
        var_map_restore_default(vars, parent, current->name, pr,
                                !pr || pr->inherits);
    } else if (kind == NS_CUSTOM_WIDE_INITIAL) {
        var_map_restore_default(vars, parent, current->name, pr, FALSE);
        if (!pr || !pr->has_initial)
            g_hash_table_replace(vars, g_strdup(current->name),
                                 g_strdup("initial"));
    } else if (kind == NS_CUSTOM_WIDE_REVERT ||
               kind == NS_CUSTOM_WIDE_REVERT_LAYER) {
        var_map_restore_default(vars, parent, current->name, pr,
                                !pr || pr->inherits);
    } else if (pr && pr->syntax && !ns_css_syntax_def_universal(pr->syntax) &&
               !ns_css_syntax_def_matches(pr->syntax,
                                          expanded ? expanded : value_text)) {
        var_map_restore_default(vars, parent, current->name, pr, pr->inherits);
    } else {
        g_hash_table_replace(vars, g_strdup(current->name),
                             g_strdup(value_text));
    }
    g_free(expanded);
}

static double
syntax_line_height_px(const ns_style *s, double font_px)
{
    const ns_css_value *v = s ? s->values[NS_CSS_LINE_HEIGHT] : NULL;
    if (v && v->kind == NS_CSS_V_LENGTH) {
        switch (v->u.length.unit) {
        case NS_CSS_UNIT_PX:      return v->u.length.v;
        case NS_CSS_UNIT_NUMBER:  return v->u.length.v * font_px;
        case NS_CSS_UNIT_PERCENT: return v->u.length.v * font_px / 100.0;
        case NS_CSS_UNIT_EM:      return v->u.length.v * font_px;
        default: break;
        }
    }
    return font_px * 1.4375;
}

static double
style_font_px(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_FONT_SIZE] : NULL;
    if (v && v->kind == NS_CSS_V_LENGTH && v->u.length.unit == NS_CSS_UNIT_PX)
        return v->u.length.v;
    return 16;
}

static gboolean
track_length_absolute(ns_css_unit unit, double v, double font_px,
                      double root_px, double *px)
{
    switch (unit) {
    case NS_CSS_UNIT_PX:  *px = v; return TRUE;
    case NS_CSS_UNIT_EM:  *px = v * font_px; return TRUE;
    case NS_CSS_UNIT_REM: *px = v * root_px; return TRUE;
    default:              return FALSE;
    }
}

static gboolean
track_length_computed_append(GString *out, const char *tok, gsize len,
                             double font_px, double root_px)
{
    char *text = g_strndup(tok, len);
    double px = 0, pct = 0;
    gboolean has_pct = FALSE, ok = FALSE, math = ns_css_is_math_fn_start(text);
    ns_css_value *v = math ? ns_css_parse_calc(text) : NULL;
    if (v && v->kind == NS_CSS_V_LENGTH) {
        has_pct = v->u.length.unit == NS_CSS_UNIT_PERCENT;
        if (has_pct) pct = v->u.length.v;
        ok = has_pct || track_length_absolute(v->u.length.unit, v->u.length.v,
                                              font_px, root_px, &px);
    } else if (v && v->kind == NS_CSS_V_CALC && !v->u.calc.fn &&
               v->u.calc.vw == 0 && v->u.calc.vh == 0 &&
               v->u.calc.vmin == 0 && v->u.calc.vmax == 0) {
        px = v->u.calc.px + v->u.calc.em * font_px + v->u.calc.rem * root_px;
        pct = v->u.calc.pct;
        has_pct = strchr(text, '%') != NULL;
        ok = TRUE;
    } else if (!math) {
        double num;
        ns_css_unit unit;
        if (ns_css_parse_length(text, &num, &unit)) {
            has_pct = unit == NS_CSS_UNIT_PERCENT;
            if (has_pct) pct = num;
            ok = has_pct || track_length_absolute(unit, num, font_px, root_px,
                                                  &px);
        }
    }
    ns_css_value_free(v);
    g_free(text);
    if (!ok) return FALSE;
    char *pct_str = ns_css_number_str(pct);
    char *px_str = ns_css_number_str(has_pct ? fabs(px) : MAX(px, 0));
    if (!has_pct)
        g_string_append_printf(out, "%spx", px_str);
    else if (!math)
        g_string_append_printf(out, "%s%%", pct_str);
    else
        g_string_append_printf(out, "calc(%s%% %c %spx)", pct_str,
                               px < 0 ? '-' : '+', px_str);
    g_free(pct_str);
    g_free(px_str);
    return TRUE;
}

static gboolean
track_number_start(const char *p)
{
    if (*p == '+' || *p == '-') p++;
    if (*p == '.') p++;
    return g_ascii_isdigit(*p);
}

static char *
tracks_computed_text(const char *text, double font_px, double root_px)
{
    GString *out = g_string_new(NULL);
    const char *p = text;
    const char *end = text + strlen(text);
    while (p < end) {
        gboolean boundary = p == text || !(is_ident(p[-1]) || p[-1] == '.');
        if (*p == '[') {
            const char *close = memchr(p, ']', (gsize)(end - p));
            const char *stop = close ? close + 1 : end;
            g_string_append_len(out, p, stop - p);
            p = stop;
            continue;
        }
        if (boundary && ns_css_is_math_fn_start(p)) {
            const char *close = match_close_paren(strchr(p, '(') + 1, end);
            const char *stop = close ? close + 1 : end;
            if (!track_length_computed_append(out, p, (gsize)(stop - p),
                                              font_px, root_px))
                g_string_append_len(out, p, stop - p);
            p = stop;
            continue;
        }
        if (boundary && track_number_start(p)) {
            const char *q = p + 1;
            while (q < end && (g_ascii_isdigit(*q) || *q == '.')) q++;
            if (q < end && (*q == 'e' || *q == 'E') && track_number_start(q + 1)) {
                q += 2;
                while (q < end && g_ascii_isdigit(*q)) q++;
            }
            const char *unit = q;
            while (q < end && (g_ascii_isalpha(*q) || *q == '%')) q++;
            gboolean flex = q - unit == 2 && g_ascii_strncasecmp(unit, "fr", 2) == 0;
            if (flex || !track_length_computed_append(out, p, (gsize)(q - p),
                                                      font_px, root_px))
                g_string_append_len(out, p, q - p);
            p = q;
            continue;
        }
        g_string_append_c(out, *p++);
    }
    return g_string_free(out, FALSE);
}

char *
ns_css_tracks_computed_serialize(const ns_style *s, const ns_style *root,
                                 int prop)
{
    const ns_css_value *v = s && prop >= 0 && prop < NS_CSS_PROP_COUNT
        ? s->values[prop] : NULL;
    if (!v || v->kind != NS_CSS_V_TRACKS || v->u.tracks.subgrid ||
        !v->specified)
        return NULL;
    return tracks_computed_text(v->specified, style_font_px(s),
                                style_font_px(root ? root : s));
}

static void
syntax_ctx_for_style(ns_css_syntax_ctx *ctx, const ns_style *s, double root_px)
{
    double font_px = style_font_px(s);
    if (root_px <= 0) root_px = font_px;
    const char *family =
        s && s->values[NS_CSS_FONT_FAMILY] &&
        s->values[NS_CSS_FONT_FAMILY]->kind == NS_CSS_V_KEYWORD
            ? s->values[NS_CSS_FONT_FAMILY]->u.keyword : NULL;
    int weight = s ? ns_css_font_weight_number(s->values[NS_CSS_FONT_WEIGHT],
                                               400) : 400;
    gboolean italic = s &&
        (ns_css_keyword_is(s->values[NS_CSS_FONT_STYLE], "italic") ||
         ns_css_keyword_is(s->values[NS_CSS_FONT_STYLE], "oblique"));
    double root_line = root_px * 1.4375;
    ctx->font_size = font_px;
    ctx->root_font_size = root_px;
    ctx->line_height = syntax_line_height_px(s, font_px);
    ctx->root_line_height = root_line;
    ctx->ex_px  = ns_css_font_relative_unit_px(NS_CSS_UNIT_EX, font_px, family,
                                        weight, italic);
    ctx->ch_px  = ns_css_font_relative_unit_px(NS_CSS_UNIT_CH, font_px, family,
                                        weight, italic);
    ctx->cap_px = ns_css_font_relative_unit_px(NS_CSS_UNIT_CAP, font_px, family,
                                        weight, italic);
    ctx->ic_px  = ns_css_font_relative_unit_px(NS_CSS_UNIT_IC, font_px, family,
                                        weight, italic);
    ctx->root_ex_px  = ns_css_font_relative_unit_px(NS_CSS_UNIT_EX, root_px, NULL,
                                             400, FALSE);
    ctx->root_ch_px  = ns_css_font_relative_unit_px(NS_CSS_UNIT_CH, root_px, NULL,
                                             400, FALSE);
    ctx->root_cap_px = ns_css_font_relative_unit_px(NS_CSS_UNIT_CAP, root_px, NULL,
                                             400, FALSE);
    ctx->root_ic_px  = ns_css_font_relative_unit_px(NS_CSS_UNIT_IC, root_px, NULL,
                                             400, FALSE);
    ctx->viewport_w = ns_css_viewport_resolve(100, NS_CSS_UNIT_VW);
    ctx->viewport_h = ns_css_viewport_resolve(100, NS_CSS_UNIT_VH);
    ctx->container_w = ns_css_container_w();
    ctx->container_h = ns_css_container_h();
    ctx->current_color = NULL;
}

static void
compute_registered_vars(ns_style *s, const ns_style *parent_style,
                        double root_px)
{
    if (!g_registered_props || !s || !s->vars || !s->vars->own) return;
    if (parent_style && s->vars == parent_style->vars) return;
    if (g_hash_table_size(g_registered_props) == 0) return;

    ns_css_syntax_ctx ctx;
    gboolean have_ctx = FALSE;
    char *current_color = NULL;
    GHashTableIter it;
    gpointer k, v;
    g_hash_table_iter_init(&it, s->vars->own);
    while (g_hash_table_iter_next(&it, &k, &v)) {
        const ns_css_property_rule *pr =
            g_hash_table_lookup(g_registered_props, k);
        if (!pr || !pr->syntax || ns_css_syntax_def_universal(pr->syntax))
            continue;
        if (!have_ctx) {
            syntax_ctx_for_style(&ctx, s, root_px);
            current_color = ns_css_value_serialize(s->values[NS_CSS_COLOR]);
            ctx.current_color = current_color;
            have_ctx = TRUE;
        }
        char *computed = ns_css_syntax_def_compute(pr->syntax, v, &ctx);
        if (computed) g_hash_table_iter_replace(&it, computed);
    }
    g_free(current_color);
}

static ns_var_map *
build_vars_for_element(const ns_style *parent_style, GArray *var_matches)
{
    ns_var_map *parent = parent_style ? parent_style->vars : NULL;
    gboolean parent_has = parent != NULL;
    gboolean have_regs  = g_registered_props &&
                          g_hash_table_size(g_registered_props) > 0;
    gboolean have_local = var_matches && var_matches->len > 0;
    if (!parent_has && !have_regs && !have_local)
        return NULL;

    if (!have_regs) {
        if (!have_local)
            return ns_var_map_ref(parent);
        GHashTable *own = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                g_free, g_free);
        g_array_sort(var_matches, var_match_cmp);
        GHashTable *prefilled = var_prefill_plain_values(own, var_matches);
        for (guint i = 0; i < var_matches->len; i++) {
            var_match *vm = &g_array_index(var_matches, var_match, i);
            if (!vm->name || !vm->text) continue;
            if (g_hash_table_contains(prefilled, vm->name)) continue;
            var_map_apply_unregistered(own, parent, var_matches, i);
        }
        g_hash_table_destroy(prefilled);
        return ns_var_map_new(own, ns_var_map_ref(parent));
    }

    if (parent_has && !have_local && g_var_adjust_cache) {
        ns_var_map *hit = g_hash_table_lookup(g_var_adjust_cache, parent);
        if (hit) return ns_var_map_ref(hit);
    }
    GHashTable *own = g_hash_table_new_full(g_str_hash, g_str_equal,
                                            g_free, g_free);
    var_map_reset_registered(own, parent);
    if (have_local) {
        g_array_sort(var_matches, var_match_cmp);
        GHashTable *prefilled = var_prefill_plain_values(own, var_matches);
        for (guint i = 0; i < var_matches->len; i++) {
            var_match *vm = &g_array_index(var_matches, var_match, i);
            if (!vm->name || !vm->text) continue;
            if (g_hash_table_contains(prefilled, vm->name)) continue;
            var_map_apply_registered(own, parent, var_matches, i);
        }
        g_hash_table_destroy(prefilled);
    }
    ns_var_map *built;
    if (parent_has && !have_local && g_hash_table_size(own) == 0) {
        g_hash_table_destroy(own);
        built = ns_var_map_ref(parent);
    } else {
        built = ns_var_map_new(own, ns_var_map_ref(parent));
    }
    if (parent_has && !have_local && g_var_adjust_cache)
        g_hash_table_insert(g_var_adjust_cache, ns_var_map_ref(parent),
                            ns_var_map_ref(built));
    return built;
}

static gboolean
pending_uses_attr(const GArray *pending_matches)
{
    if (!pending_matches) return FALSE;
    for (guint i = 0; i < pending_matches->len; i++) {
        const pending_match *e = &g_array_index((GArray *)pending_matches, pending_match, i);
        if (e->pd && e->pd->raw_vtext && strstr(e->pd->raw_vtext, "attr(")) return TRUE;
    }
    return FALSE;
}

static gboolean
append_pending_decls(const pending_match *pm, const char *value_text,
                     GArray *matches, GPtrArray *owned_values)
{
    char *synth = g_strdup_printf("%s: %s;}", pm->pd->pname, value_text);
    GArray *temp = g_array_new(FALSE, FALSE, sizeof(ns_css_decl));
    const char *sp = synth;
    ns_css_parse_declaration_block(sp, synth + strlen(synth), temp, NULL);
    g_free(synth);
    gboolean any = FALSE;
    for (guint i = 0; i < temp->len; i++) {
        ns_css_decl *d = &g_array_index(temp, ns_css_decl, i);
        if (!d->value) continue;
        g_ptr_array_add(owned_values, d->value);
        match_entry me = {
            .origin = pm->origin,
            .spec_a = pm->spec_a, .spec_b = pm->spec_b, .spec_c = pm->spec_c,
            .sheet_index = pm->sheet_index,
            .layer_order = pm->layer_order,
            .scope_order = pm->scope_order,
            .source_order = pm->source_order,
            .decl_order = pm->decl_order_base,
            .important = pm->pd->important || d->important,
            .inline_style = pm->inline_style,
            .rule = pm->rule,
            .value = d->value,
            .prop  = d->prop,
        };
        g_array_append_val(matches, me);
        any = TRUE;
    }
    g_array_free(temp, TRUE);
    return any;
}

static char *
pending_substituted_value(const pending_match *pm, const ns_var_map *vars,
                          const ns_node *node)
{
    gboolean custom = pm->pd->pname[0] == '-' && pm->pd->pname[1] == '-';
    char *substituted = ns_css_substitute_vars(pm->pd->raw_vtext, vars,
                                               g_registered_props, 0);
    if (substituted && strstr(substituted, "attr(")) {
        gboolean tainted = FALSE;
        char *with_attrs = ns_css_substitute_attrs(substituted, node, &tainted);
        g_free(substituted);
        substituted = with_attrs;
        if (substituted && tainted && !custom) {
            g_free(substituted);
            substituted = NULL;
        }
    }
    if (substituted) {
        gboolean important = FALSE;
        css_strip_important(substituted, &important);
        if (important) {
            g_free(substituted);
            substituted = NULL;
        }
    }
    return substituted;
}

static void
resolve_pending_into_matches(GArray *pending_matches,
                             const ns_var_map *vars,
                             GArray *matches,
                             GPtrArray *owned_values,
                             const ns_node *node)
{
    if (!pending_matches || pending_matches->len == 0) return;
    g_array_sort(pending_matches, pending_match_cmp);
    for (guint pmi = 0; pmi < pending_matches->len; pmi++) {
        pending_match *pm = &g_array_index(pending_matches, pending_match, pmi);
        if (!pm->pd || !pm->pd->pname || !pm->pd->raw_vtext) continue;
        char *substituted = pending_substituted_value(pm, vars, node);
        gboolean applied = substituted &&
            append_pending_decls(pm, substituted, matches, owned_values);
        g_free(substituted);
        if (!applied && !(pm->pd->pname[0] == '-' && pm->pd->pname[1] == '-'))
            append_pending_decls(pm, "unset", matches, owned_values);
    }
}

static const char *kUa =
    "html { display: block; color: CanvasText; "
    "font-family: serif; font-size: 16px; line-height: normal; }\n"
    "body { display: block; margin: 8px; }\n"
    "div, p, section, article, header, footer, nav, main, aside, "
    "dir, menu, ul, ol, dl, dt, dd, blockquote, pre, address, "
    "hr, form, fieldset, figure, figcaption, center, dialog, "
    "legend, search, hgroup { display: block; }\n"
    "li { display: list-item; }\n"
    "address { font-style: italic; }\n"
    "fieldset { margin-inline: 2px; border: groove 2px ThreeDFace; "
    "padding-block: 0.35em 0.625em; padding-inline: 0.75em; "
    "min-inline-size: min-content; }\n"
    "legend { padding-inline: 2px; }\n"
    "center { text-align: center; }\n"
    "h1, h2, h3, h4, h5, h6 { display: block; font-weight: bold; }\n"
    "span, a, b, i, em, strong, code, small, big, u, s, del, ins, mark, "
    "tt, kbd, samp, var, cite, dfn, abbr, acronym, sub, sup, q, time, "
    "bdi, bdo, ruby, rb, rt, output, "
    "button, label { display: inline; }\n"
    "var { font-style: italic; }\n"
    "[dir]:dir(ltr), bdi:dir(ltr), input[type=\"tel\" i]:dir(ltr) "
    "{ direction: ltr; }\n"
    "[dir]:dir(rtl), bdi:dir(rtl) { direction: rtl; }\n"
    "bdo { unicode-bidi: bidi-override; }\n"
    "bdi { unicode-bidi: isolate; }\n"
    "rt { font-size: 0.7em; }\n"
    "abbr[title], acronym[title] { text-decoration: underline dotted; cursor: help; }\n"
    "rp, datalist { display: none; }\n"
    "h1 { font-size: 2.0em;  margin: 0.67em 0; }\n"
    "h2 { font-size: 1.5em;  margin: 0.83em 0; }\n"
    "h3 { font-size: 1.17em; margin: 1.00em 0; }\n"
    "h4 { font-size: 1.0em;  margin: 1.33em 0; }\n"
    "h5 { font-size: 0.83em; margin: 1.67em 0; }\n"
    "h6 { font-size: 0.67em; margin: 2.33em 0; }\n"
    "p { margin: 1em 0; }\n"
    "blockquote { margin: 1em 40px; }\n"
    "hr { margin: 12px 0; height: 1px; background-color: #888888; "
    "overflow: hidden; }\n"
    "dir, dl, menu, ol, ul { margin-block: 1em; }\n"
    ":is(dir, dl, menu, ol, ul) :is(dir, dl, menu, ol, ul) { margin-block: 0; }\n"
    "dd { margin-inline-start: 40px; }\n"
    "dir, menu, ol, ul { padding-inline-start: 40px; }\n"
    "dd:dir(rtl) { margin-left: 0; margin-right: 40px; }\n"
    ":is(dir, menu, ol, ul):dir(rtl) { padding-left: 0; padding-right: 40px; }\n"
    "ol { list-style-type: decimal; }\n"
    "dir, menu, ul { list-style-type: disc; }\n"
    ":is(dir, menu, ol, ul) :is(dir, menu, ul) { list-style-type: circle; }\n"
    ":is(dir, menu, ol, ul) :is(dir, menu, ol, ul) :is(dir, menu, ul) "
    "{ list-style-type: square; }\n"
    ":link { color: LinkText; }\n"
    ":visited { color: VisitedText; }\n"
    ":link, :visited { text-decoration: underline; cursor: pointer; }\n"
    "b, strong { font-weight: bold; }\n"
    "i, em, cite, dfn { font-style: italic; }\n"
    "big { font-size: larger; }\n"
    "code, pre, kbd, samp, tt { font-family: monospace; }\n"
    "pre { margin-block: 1em; white-space: pre; }\n"
    "textarea { white-space: pre-wrap; }\n"
    "mark { background-color: Mark; color: MarkText; }\n"
    "small { font-size: smaller; }\n"
    "sub, sup { font-size: 0.75em; }\n"
    "table { display: table; border-collapse: separate; border-spacing: 2px; "
    "box-sizing: border-box; }\n"
    "caption { display: table-caption; text-align: center; }\n"
    "thead { display: table-header-group; }\n"
    "tbody { display: table-row-group; }\n"
    "tfoot { display: table-footer-group; }\n"
    "colgroup { display: table-column-group; }\n"
    "col { display: table-column; }\n"
    "tr { display: table-row; }\n"
    "td, th { display: table-cell; padding: 1px; }\n"
    "thead, tbody, tfoot, table > tr { vertical-align: middle; }\n"
    "tr, td, th { vertical-align: inherit; }\n"
    "th { font-weight: bold; text-align: center; }\n"
    "thead, tbody, tfoot, tr { border-color: inherit; }\n"
    "table:is([rules=none i], [rules=groups i], [rules=rows i], "
    "[rules=cols i], [rules=all i], [frame=void i], [frame=above i], "
    "[frame=below i], [frame=hsides i], [frame=lhs i], [frame=rhs i], "
    "[frame=vsides i], [frame=box i], [frame=border i]), "
    "table:is([rules=none i], [rules=groups i], [rules=rows i], "
    "[rules=cols i], [rules=all i]) > tr > :is(td, th), "
    "table:is([rules=none i], [rules=groups i], [rules=rows i], "
    "[rules=cols i], [rules=all i]) > :is(thead, tbody, tfoot) > tr > "
    ":is(td, th) { border-color: black; }\n"
    "img { display: inline; }\n"
    "figure { margin: 1em 40px; }\n"
    "input[type=\"radio\"], input[type=\"checkbox\"], input[type=\"reset\"], "
    "input[type=\"button\"], input[type=\"submit\"], input[type=\"color\"], "
    "input[type=\"search\"], select, button { box-sizing: border-box; }\n"
    "input, select, textarea, button { font-style: normal; font-weight: normal; "
    "font-size: 13.333333px; font-family: system-ui, sans-serif; }\n"
    "textarea { font-family: monospace; }\n"
    "button { display: inline-block; padding: 1px 6px; background-color: #e6e6e6; "
    "border-top-width: 2px; border-right-width: 2px; "
    "border-bottom-width: 2px; border-left-width: 2px; "
    "border-top-style: outset; border-right-style: outset; "
    "border-bottom-style: outset; border-left-style: outset; "
    "border-top-color: #b8b8b8; border-right-color: #b8b8b8; "
    "border-bottom-color: #b8b8b8; border-left-color: #b8b8b8; }\n"
    "input, select, textarea { color: FieldText; }\n"
    "input::placeholder, textarea::placeholder { color: #757575; }\n"
    "button { color: ButtonText; }\n"
    "input, button, textarea { letter-spacing: initial; "
    "word-spacing: initial; line-height: initial; }\n"
    "input, select, button, textarea { text-transform: initial; "
    "text-indent: initial; text-shadow: initial; appearance: auto; }\n"
    "input[type=\"hidden\" i], input[type=\"file\" i], "
    "input[type=\"image\" i] { appearance: none; }\n"
    "meter, progress { appearance: auto; }\n"
    "input, select, textarea { text-align: initial; }\n"
    ":is(input[type=\"reset\" i], input[type=\"button\" i], "
    "input[type=\"submit\" i], button) { text-align: center; }\n"
    "input, textarea, select { display: inline-block; }\n"
    "input, textarea, select { padding: 1px 2px; background-color: #ffffff; "
    "border-top-width: 2px; border-right-width: 2px; "
    "border-bottom-width: 2px; border-left-width: 2px; "
    "border-top-style: inset; border-right-style: inset; "
    "border-bottom-style: inset; border-left-style: inset; "
    "border-top-color: #767676; border-right-color: #767676; "
    "border-bottom-color: #767676; border-left-color: #767676; }\n"
    "select { padding: 0; }\n"
    "textarea { padding: 2px; }\n"
    "select, textarea { border-top-width: 1px; border-right-width: 1px; "
    "border-bottom-width: 1px; border-left-width: 1px; "
    "border-top-style: solid; border-right-style: solid; "
    "border-bottom-style: solid; border-left-style: solid; }\n"
    "area, base, head, script, style, title, meta, link "
    "{ display: none; }\n"
    "[data-nd-shadow-root] { display: block; }\n"
    "input[type=hidden i] { display: none !important; }\n"
    ":is(table, thead, tbody, tfoot, tr) > form "
    "{ display: none !important; }\n"
    "video { display: inline; object-fit: contain; }\n"
    "iframe { border: 2px inset; }\n"
    "canvas { display: inline; }\n"
    "iframe, frame, frameset, embed { display: none; }\n"
    "iframe[data-nd-frame-loaded], object[data-nd-frame-loaded] "
    "{ display: block; overflow: hidden; }\n"
    "object[data-nd-frame-loaded] > * { display: none; }\n"
    "audio, param { display: none; }\n"
    "audio[controls] { display: inline-block; }\n"
    ":fullscreen { position: fixed !important; top: 0 !important; "
    "right: 0 !important; bottom: 0 !important; left: 0 !important; "
    "width: 100vw !important; height: 100vh !important; "
    "margin: 0 !important; z-index: 2147483647 !important; "
    "background-color: #000 !important; }\n"
    "svg { display: inline; }\n"
    "noframes, frame, frameset, applet, basefont, "
    "noembed, isindex { display: none; }\n"
    "listing, xmp, plaintext { display: block; font-family: monospace; "
    "white-space: pre; margin-block: 1em; }\n"
    "details, summary { display: block; }\n"
    "details > summary:first-of-type { display: list-item; "
    "list-style: disclosure-closed inside; }\n"
    "details[open] > summary:first-of-type "
    "{ list-style-type: disclosure-open; }\n"
    "dialog:not([open]) { display: none; }\n"
    "dialog { position: absolute; inset-inline-start: 0; inset-inline-end: 0; "
    "width: fit-content; height: fit-content; margin: auto; border: solid; "
    "padding: 1em; background-color: Canvas; color: CanvasText; }\n"
    "dialog:modal { position: fixed; overflow: auto; inset-block: 0; "
    "max-width: calc(100% - 6px - 2em); "
    "max-height: calc(100% - 6px - 2em); }\n"
    "picture { display: inline; }\n"
    "[hidden]:not([hidden=\"until-found\" i]):not(embed) "
    "{ display: none; }\n"
    "[hidden=\"until-found\" i]:not(embed) "
    "{ content-visibility: hidden; }\n"
    "embed[hidden] { display: inline; height: 0; width: 0; }\n"
    "[popover] { position: fixed; inset: 0; width: fit-content; "
    "height: fit-content; margin: auto; border: solid; padding: 0.25em; "
    "overflow: auto; color: CanvasText; background-color: Canvas; }\n"
    "[popover]:not([data-nd-popover-open]):not(dialog[open]) "
    "{ display: none; }\n"
    "dialog[popover][data-nd-popover-open] { display: block; }\n"
    "template { display: none; }\n"
    "marquee { display: inline-block; text-align: initial; "
    "overflow: hidden; }\n"
    "nobr { white-space: nowrap; }\n"
    "br[clear=\"left\" i] { clear: left; }\n"
    "br[clear=\"right\" i] { clear: right; }\n"
    "br[clear=\"all\" i], br[clear=\"both\" i] { clear: both; }\n"
    "caption[align=\"left\" i] { text-align: left; }\n"
    "caption[align=\"right\" i] { text-align: right; }\n"
    "caption[align=\"bottom\" i] { caption-side: bottom; }\n";

static const char *kUaQuirks =
    "form { margin-block-end: 1em; }\n"
    "li { list-style-position: inside; }\n"
    "li :is(dir, menu, ol, ul) { list-style-position: outside; }\n"
    ":is(dir, menu, ol, ul) :is(dir, menu, ol, ul, li) "
    "{ list-style-position: unset; }\n"
    "table { font-weight: initial; font-style: initial; "
    "font-variant: initial; font-size: initial; line-height: initial; "
    "white-space: initial; text-align: initial; }\n"
    "input:not([type=image i]), textarea { box-sizing: border-box; }\n"
    "img[align=left i] { margin-right: 3px; }\n"
    "img[align=right i] { margin-left: 3px; }\n";

static const ns_css_stylesheet *
ua_sheet_for(const ns_node *doc)
{
    static ns_css_stylesheet *standard = NULL;
    static ns_css_stylesheet *quirks = NULL;
    if (doc && (doc->flags & NS_NODE_QUIRKS)) {
        if (!quirks) {
            char *css = g_strconcat(kUa, kUaQuirks, NULL);
            quirks = ns_css_stylesheet_parse(css, -1);
            g_free(css);
        }
        return quirks;
    }
    if (!standard) standard = ns_css_stylesheet_parse(kUa, -1);
    return standard;
}

static double
resolve_font_size_px(const ns_style *s, const ns_style *parent_style)
{
    double parent_px = 16;
    if (parent_style && parent_style->values[NS_CSS_FONT_SIZE] &&
        parent_style->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
        parent_style->values[NS_CSS_FONT_SIZE]->u.length.unit == NS_CSS_UNIT_PX)
        parent_px = parent_style->values[NS_CSS_FONT_SIZE]->u.length.v;
    ns_css_value *fs = s ? s->values[NS_CSS_FONT_SIZE] : NULL;
    if (fs && fs->kind == NS_CSS_V_CALC)
        return fs->u.calc.px + fs->u.calc.em * parent_px +
               fs->u.calc.rem * parent_px +
               fs->u.calc.pct * parent_px / 100.0 +
               calc_viewport_refresh_px(fs);
    if (!fs || fs->kind != NS_CSS_V_LENGTH) return parent_px;
    switch (fs->u.length.unit) {
    case NS_CSS_UNIT_PX:      return fs->u.length.v;
    case NS_CSS_UNIT_NUMBER:  return fs->u.length.v;
    case NS_CSS_UNIT_EM:      return fs->u.length.v * parent_px;
    case NS_CSS_UNIT_REM:     return fs->u.length.v * parent_px;
    case NS_CSS_UNIT_PERCENT: return fs->u.length.v * parent_px / 100.0;
    case NS_CSS_UNIT_LH:      return fs->u.length.v * parent_px * 1.5;
    case NS_CSS_UNIT_RLH:     return fs->u.length.v * 24.0;
    case NS_CSS_UNIT_EX:
    case NS_CSS_UNIT_CH:
    case NS_CSS_UNIT_CAP:
    case NS_CSS_UNIT_IC:
    case NS_CSS_UNIT_REX:
    case NS_CSS_UNIT_RCH:
    case NS_CSS_UNIT_RCAP:
    case NS_CSS_UNIT_RIC: {
        const char *pf =
            parent_style && parent_style->values[NS_CSS_FONT_FAMILY] &&
            parent_style->values[NS_CSS_FONT_FAMILY]->kind == NS_CSS_V_KEYWORD
            ? parent_style->values[NS_CSS_FONT_FAMILY]->u.keyword : NULL;
        int pw = parent_style
            ? ns_css_font_weight_number(parent_style->values[NS_CSS_FONT_WEIGHT], 400)
            : 400;
        gboolean pi = parent_style &&
            (ns_css_keyword_is(parent_style->values[NS_CSS_FONT_STYLE], "italic") ||
             ns_css_keyword_is(parent_style->values[NS_CSS_FONT_STYLE], "oblique"));
        return fs->u.length.v *
               ns_css_font_relative_unit_px(fs->u.length.unit, parent_px, pf, pw, pi);
    }
    case NS_CSS_UNIT_VW:
    case NS_CSS_UNIT_VH:
    case NS_CSS_UNIT_VMIN:
    case NS_CSS_UNIT_VMAX:
        return ns_css_viewport_resolve(fs->u.length.v, fs->u.length.unit);
    case NS_CSS_UNIT_CQW:
    case NS_CSS_UNIT_CQH:
    case NS_CSS_UNIT_CQMIN:
    case NS_CSS_UNIT_CQMAX:
        return ns_css_container_unit_resolve(fs->u.length.v, fs->u.length.unit);
    }
    return parent_px;
}

static ns_css_value *
ns_css_value_cow(ns_style *out, int prop)
{
    ns_css_value *v = out->values[prop];
    if (!v || v->ref == 0) return v;
    ns_css_value *copy = g_new0(ns_css_value, 1);
    *copy = *v;
    copy->ref = 0;
    copy->image_set_text = g_strdup(v->image_set_text);
    copy->specified = g_strdup(v->specified);
    if (copy->next_layer) copy->next_layer->ref++;
    v->ref--;
    out->values[prop] = copy;
    return copy;
}

#define NS_CSS_CALC_LIMIT 33554400.0

static double
calc_clamp_finite(double v)
{
    if (isnan(v)) return 0;
    if (isinf(v)) return v < 0 ? -NS_CSS_CALC_LIMIT : NS_CSS_CALC_LIMIT;
    return v;
}

static gboolean
calc_value_is_finite(const ns_css_value *v)
{
    if (v->kind == NS_CSS_V_LENGTH) return isfinite(v->u.length.v);
    if (v->kind == NS_CSS_V_TRANSFORM) {
        for (int k = 0; k < v->u.transform.n_ops; k++) {
            const ns_css_transform_op *op = &v->u.transform.ops[k];
            if (op->kind != NS_CSS_TFN_TRANSLATE && op->kind != NS_CSS_TFN_SCALE)
                continue;
            if (!isfinite(op->a) || !isfinite(op->b) || !isfinite(op->c) ||
                !isfinite(op->a_pct) || !isfinite(op->b_pct))
                return FALSE;
        }
        return TRUE;
    }
    if (v->kind != NS_CSS_V_CALC) return TRUE;
    if (!isfinite(v->u.calc.px) || !isfinite(v->u.calc.pct) ||
        !isfinite(v->u.calc.em) || !isfinite(v->u.calc.rem))
        return FALSE;
    for (int i = 0; i < v->u.calc.n_args && i < 4; i++)
        if (!isfinite(v->u.calc.args[i].px) || !isfinite(v->u.calc.args[i].pct))
            return FALSE;
    return TRUE;
}

static void
calc_value_clamp_finite(ns_css_value *v)
{
    if (v->kind == NS_CSS_V_LENGTH) {
        v->u.length.v = calc_clamp_finite(v->u.length.v);
        return;
    }
    if (v->kind == NS_CSS_V_TRANSFORM) {
        for (int k = 0; k < v->u.transform.n_ops; k++) {
            ns_css_transform_op *op = &v->u.transform.ops[k];
            if (op->kind != NS_CSS_TFN_TRANSLATE && op->kind != NS_CSS_TFN_SCALE)
                continue;
            op->a = calc_clamp_finite(op->a);
            op->b = calc_clamp_finite(op->b);
            op->c = calc_clamp_finite(op->c);
            op->a_pct = calc_clamp_finite(op->a_pct);
            op->b_pct = calc_clamp_finite(op->b_pct);
        }
        return;
    }
    v->u.calc.px = calc_clamp_finite(v->u.calc.px);
    v->u.calc.pct = calc_clamp_finite(v->u.calc.pct);
    v->u.calc.em = calc_clamp_finite(v->u.calc.em);
    v->u.calc.rem = calc_clamp_finite(v->u.calc.rem);
    for (int i = 0; i < v->u.calc.n_args && i < 4; i++) {
        v->u.calc.args[i].px = calc_clamp_finite(v->u.calc.args[i].px);
        v->u.calc.args[i].pct = calc_clamp_finite(v->u.calc.args[i].pct);
    }
}

static gboolean
transform_has_font_units(const ns_css_transform *tf)
{
    for (int k = 0; k < tf->n_ops; k++)
        for (int m = 0; m < 3; m++)
            if (tf->ops[k].em[m] != 0 || tf->ops[k].rem[m] != 0)
                return TRUE;
    return FALSE;
}

static gboolean
calc_has_percent(const ns_css_value *v)
{
    if (v->u.calc.pct != 0) return TRUE;
    for (int i = 0; i < v->u.calc.n_args && i < 4; i++)
        if (v->u.calc.args[i].pct != 0) return TRUE;
    return FALSE;
}

static void
calc_fold_percent(ns_css_value *v, double basis)
{
    if (v->u.calc.fn && v->u.calc.n_args) {
        for (int i = 0; i < v->u.calc.n_args && i < 4; i++) {
            v->u.calc.args[i].px += v->u.calc.args[i].pct * basis / 100.0;
            v->u.calc.args[i].pct = 0;
        }
        v->u.calc.px = ns_css_calc_math_fn_px(v, basis);
        v->u.calc.fn = 0;
        v->u.calc.n_args = 0;
        v->u.calc.arg_none = 0;
    } else {
        v->u.calc.px += v->u.calc.pct * basis / 100.0;
    }
    v->u.calc.pct = 0;
}

static void
resolve_em_units(ns_style *out, const ns_style *parent_style, double root_px)
{
    double my_font_px = resolve_font_size_px(out, parent_style);
    if (isnan(my_font_px) || my_font_px < 0) my_font_px = 0;
    double font_rem_px = root_px > 0 ? root_px : 16.0;
    if (out->values[NS_CSS_FONT_SIZE] &&
        out->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
        out->values[NS_CSS_FONT_SIZE]->u.length.unit == NS_CSS_UNIT_REM) {
        my_font_px = out->values[NS_CSS_FONT_SIZE]->u.length.v * font_rem_px;
    } else if (out->values[NS_CSS_FONT_SIZE] &&
               out->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_CALC &&
               out->values[NS_CSS_FONT_SIZE]->u.calc.rem != 0) {
        const ns_css_value *fsv = out->values[NS_CSS_FONT_SIZE];
        double parent_px = 16;
        if (parent_style && parent_style->values[NS_CSS_FONT_SIZE] &&
            parent_style->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
            parent_style->values[NS_CSS_FONT_SIZE]->u.length.unit ==
                NS_CSS_UNIT_PX)
            parent_px = parent_style->values[NS_CSS_FONT_SIZE]->u.length.v;
        my_font_px = fsv->u.calc.px + fsv->u.calc.em * parent_px +
                     fsv->u.calc.rem * font_rem_px +
                     fsv->u.calc.pct * parent_px / 100.0 +
                     calc_viewport_refresh_px(fsv);
    }
    if (isnan(my_font_px) || my_font_px < 0) my_font_px = 0;
    if (root_px <= 0) root_px = my_font_px;
    if (out->values[NS_CSS_FONT_SIZE] &&
        out->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH) {
        ns_css_value *fs = ns_css_value_cow(out, NS_CSS_FONT_SIZE);
        fs->u.length.v = my_font_px;
        fs->u.length.unit = NS_CSS_UNIT_PX;
    } else {
        ns_css_value *fs = g_new0(ns_css_value, 1);
        fs->kind = NS_CSS_V_LENGTH;
        fs->u.length.v = my_font_px;
        fs->u.length.unit = NS_CSS_UNIT_PX;
        out->values[NS_CSS_FONT_SIZE] = fs;
    }
    const char *fr_family =
        out->values[NS_CSS_FONT_FAMILY] &&
        out->values[NS_CSS_FONT_FAMILY]->kind == NS_CSS_V_KEYWORD
        ? out->values[NS_CSS_FONT_FAMILY]->u.keyword : NULL;
    int fr_weight = ns_css_font_weight_number(out->values[NS_CSS_FONT_WEIGHT], 400);
    gboolean fr_italic =
        ns_css_keyword_is(out->values[NS_CSS_FONT_STYLE], "italic") ||
        ns_css_keyword_is(out->values[NS_CSS_FONT_STYLE], "oblique");
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++) {
        if (i == NS_CSS_FONT_SIZE) continue;
        ns_css_value *v = out->values[i];
        if (!v) continue;
        if (!calc_value_is_finite(v)) {
            v = ns_css_value_cow(out, i);
            calc_value_clamp_finite(v);
        }
        if (v->kind == NS_CSS_V_SHADOW) {
            gboolean needs = FALSE;
            for (int k = 0; k < v->u.shadow.n && !needs; k++)
                for (int m = 0; m < 4; m++)
                    if (v->u.shadow.s[k].em[m] != 0 ||
                        v->u.shadow.s[k].rem[m] != 0)
                        needs = TRUE;
            if (!needs) continue;
            v = ns_css_value_cow(out, i);
            for (int k = 0; k < v->u.shadow.n; k++) {
                ns_css_shadow *sh = &v->u.shadow.s[k];
                double *fields[4] = { &sh->x, &sh->y, &sh->blur, &sh->spread };
                for (int m = 0; m < 4; m++) {
                    *fields[m] += sh->em[m] * my_font_px + sh->rem[m] * root_px;
                    sh->em[m] = 0;
                    sh->rem[m] = 0;
                }
                sh->blur = CLAMP(sh->blur, 0.0, 1000.0);
            }
            continue;
        }
        if (v->kind == NS_CSS_V_TRACKS) {
            gboolean needs = FALSE;
            for (int k = 0; k < v->u.tracks.n && !needs; k++) {
                const ns_css_track *t = &v->u.tracks.tracks[k];
                needs = t->em != 0 || t->rem != 0 || t->min_em != 0 ||
                        t->min_rem != 0;
            }
            if (!needs) continue;
            v = ns_css_value_cow(out, i);
            for (int k = 0; k < v->u.tracks.n; k++) {
                ns_css_track *t = &v->u.tracks.tracks[k];
                t->v += t->em * my_font_px + t->rem * root_px;
                t->em = 0;
                t->rem = 0;
                t->min_v += t->min_em * my_font_px + t->min_rem * root_px;
                t->min_em = 0;
                t->min_rem = 0;
            }
            continue;
        }
        if (v->kind == NS_CSS_V_TRANSFORM) {
            if (!transform_has_font_units(&v->u.transform)) continue;
            v = ns_css_value_cow(out, i);
            for (int k = 0; k < v->u.transform.n_ops; k++) {
                ns_css_transform_op *op = &v->u.transform.ops[k];
                double *axes[3] = { &op->a, &op->b, &op->c };
                for (int m = 0; m < 3; m++) {
                    *axes[m] += op->em[m] * my_font_px + op->rem[m] * root_px;
                    op->em[m] = 0;
                    op->rem[m] = 0;
                }
            }
            continue;
        }
        if (v->kind == NS_CSS_V_CALC) {
            double viewport_refresh = calc_viewport_refresh_px(v);
            gboolean line_pct = i == NS_CSS_LINE_HEIGHT && calc_has_percent(v);
            if (v->u.calc.em == 0 && v->u.calc.rem == 0 &&
                v->u.calc.vw == 0 && v->u.calc.vh == 0 &&
                v->u.calc.vmin == 0 && v->u.calc.vmax == 0 && !line_pct)
                continue;
            v = ns_css_value_cow(out, i);
            v->u.calc.px += v->u.calc.em * my_font_px +
                            v->u.calc.rem * root_px + viewport_refresh;
            v->u.calc.em = 0;
            v->u.calc.rem = 0;
            v->u.calc.vw = 0;
            v->u.calc.vh = 0;
            v->u.calc.vmin = 0;
            v->u.calc.vmax = 0;
            if (line_pct) calc_fold_percent(v, my_font_px);
            continue;
        }
        if (v->kind == NS_CSS_V_SIZE && !v->u.size.w_auto && !v->u.size.h_auto) {
            gboolean needs = v->u.size.w_unit == NS_CSS_UNIT_EM ||
                             v->u.size.w_unit == NS_CSS_UNIT_REM ||
                             v->u.size.h_unit == NS_CSS_UNIT_EM ||
                             v->u.size.h_unit == NS_CSS_UNIT_REM;
            if (!needs) continue;
            v = ns_css_value_cow(out, i);
            if (v->u.size.w_unit == NS_CSS_UNIT_EM || v->u.size.w_unit == NS_CSS_UNIT_REM) {
                v->u.size.w *= v->u.size.w_unit == NS_CSS_UNIT_EM ? my_font_px : root_px;
                v->u.size.w_unit = NS_CSS_UNIT_PX;
            }
            if (v->u.size.h_unit == NS_CSS_UNIT_EM || v->u.size.h_unit == NS_CSS_UNIT_REM) {
                v->u.size.h *= v->u.size.h_unit == NS_CSS_UNIT_EM ? my_font_px : root_px;
                v->u.size.h_unit = NS_CSS_UNIT_PX;
            }
            continue;
        }
        if (v->kind != NS_CSS_V_LENGTH) continue;
        switch (v->u.length.unit) {
        case NS_CSS_UNIT_PERCENT:
            if (i != NS_CSS_LINE_HEIGHT) break;
            v = ns_css_value_cow(out, i);
            v->u.length.v *= my_font_px / 100.0;
            v->u.length.unit = NS_CSS_UNIT_PX;
            break;
        case NS_CSS_UNIT_EM:
            v = ns_css_value_cow(out, i);
            v->u.length.v *= my_font_px;
            v->u.length.unit = NS_CSS_UNIT_PX;
            break;
        case NS_CSS_UNIT_REM:
            v = ns_css_value_cow(out, i);
            v->u.length.v *= root_px;
            v->u.length.unit = NS_CSS_UNIT_PX;
            break;
        case NS_CSS_UNIT_VW:
        case NS_CSS_UNIT_VH:
        case NS_CSS_UNIT_VMIN:
        case NS_CSS_UNIT_VMAX:
            v = ns_css_value_cow(out, i);
            v->u.length.v = ns_css_viewport_resolve(v->u.length.v, v->u.length.unit);
            v->u.length.unit = NS_CSS_UNIT_PX;
            break;
        case NS_CSS_UNIT_EX:
        case NS_CSS_UNIT_CH:
        case NS_CSS_UNIT_CAP:
        case NS_CSS_UNIT_IC:
            v = ns_css_value_cow(out, i);
            v->u.length.v *= ns_css_font_relative_unit_px(v->u.length.unit, my_font_px,
                                                   fr_family, fr_weight,
                                                   fr_italic);
            v->u.length.unit = NS_CSS_UNIT_PX;
            break;
        default:
            break;
        }
    }
}

static gboolean
value_is_inherit(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "inherit") == 0;
}

static gboolean
value_is_initial(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "initial") == 0;
}

static gboolean
value_is_unset(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "unset") == 0;
}

static gboolean
value_is_revert(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "revert") == 0;
}

static gboolean
value_is_revert_layer(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "revert-layer") == 0;
}

static gboolean
value_is_revert_rule(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "revert-rule") == 0;
}

static const ns_css_value *
cascade_rollback_value(GArray *matches, gint before,
                       const match_entry *rollback)
{
    gboolean layer_only = value_is_revert_layer(rollback->value);
    gboolean rule_only = value_is_revert_rule(rollback->value);
    for (gint j = before; j >= 0; j--) {
        match_entry *prev = &g_array_index(matches, match_entry, (guint)j);
        if (prev->prop != rollback->prop) continue;
        if (rule_only) {
            if (prev->rule == rollback->rule) continue;
        } else if (layer_only) {
            if (prev->origin == rollback->origin) {
                if (rollback->inline_style) {
                    if (prev->inline_style)
                        continue;
                } else if (rollback->layer_order == NS_CSS_LAYER_NONE) {
                    if (prev->layer_order == NS_CSS_LAYER_NONE)
                        continue;
                } else if (prev->layer_order >= rollback->layer_order) {
                    continue;
                }
            }
        } else if (css_same_revert_origin(rollback->origin, prev->origin)) {
            continue;
        }
        if (value_is_revert(prev->value) ||
            value_is_revert_layer(prev->value) ||
            value_is_revert_rule(prev->value))
            return cascade_rollback_value(matches, j - 1, prev);
        return prev->value;
    }
    return NULL;
}

static gboolean
style_is_out_of_flow(const ns_style *s)
{
    const ns_css_value *pos = s->values[NS_CSS_POSITION];
    if (ns_css_keyword_is(pos, "absolute") || ns_css_keyword_is(pos, "fixed"))
        return TRUE;
    const ns_css_value *flt = s->values[NS_CSS_FLOAT];
    return flt && flt->kind == NS_CSS_V_KEYWORD && flt->u.keyword &&
           strcmp(flt->u.keyword, "none") != 0;
}

static ns_display
legacy_webkit_box_display(ns_style *s, ns_display d)
{
    const ns_css_value *disp = s->values[NS_CSS_DISPLAY];
    if (!disp || disp->kind != NS_CSS_V_KEYWORD || !disp->u.keyword ||
        strncmp(disp->u.keyword, "-webkit-", 8) != 0)
        return d;
    const ns_css_value *orient = s->values[NS_CSS_WEBKIT_BOX_ORIENT];
    if (!ns_css_keyword_is(orient, "vertical") &&
        !ns_css_keyword_is(orient, "block-axis"))
        return d;
    const ns_css_value *clamp = s->values[NS_CSS_LINE_CLAMP];
    if (clamp && clamp->kind == NS_CSS_V_LENGTH && clamp->u.length.v >= 1) {
        d.inner = NS_DISPLAY_INNER_FLOW_ROOT;
        return d;
    }
    ns_css_value_free(s->values[NS_CSS_FLEX_DIRECTION]);
    s->values[NS_CSS_FLEX_DIRECTION] = keyword_value_dup("column");
    return d;
}

static ns_display
display_after_blockification(ns_display d, const ns_style *s,
                             const ns_style *layout_parent, gboolean is_root)
{
    if (d.box == NS_DISPLAY_BOX_NONE) return d;
    if (is_root) {
        if (d.box == NS_DISPLAY_BOX_CONTENTS) {
            d.box = NS_DISPLAY_BOX_NORMAL;
            d.inner = NS_DISPLAY_INNER_FLOW;
        }
        return ns_css_display_blockified(d);
    }
    if (d.box != NS_DISPLAY_BOX_NORMAL) return d;
    if (style_is_out_of_flow(s)) return ns_css_display_blockified(d);
    ns_display parent = ns_css_display_of(layout_parent);
    if (ns_display_is_flex_container(parent) ||
        ns_display_is_grid_container(parent))
        return ns_css_display_blockified(d);
    return d;
}

static void
overflow_pair_normalize(ns_style *out)
{
    const ns_css_value *x = out->values[NS_CSS_OVERFLOW_X];
    const ns_css_value *y = out->values[NS_CSS_OVERFLOW_Y];
    const char *kx = x && x->kind == NS_CSS_V_KEYWORD ? x->u.keyword : NULL;
    const char *ky = y && y->kind == NS_CSS_V_KEYWORD ? y->u.keyword : NULL;
    gboolean x_vis = !kx || strcmp(kx, "visible") == 0;
    gboolean y_vis = !ky || strcmp(ky, "visible") == 0;
    gboolean x_scrolls = kx && strcmp(kx, "visible") != 0 && strcmp(kx, "clip") != 0;
    gboolean y_scrolls = ky && strcmp(ky, "visible") != 0 && strcmp(ky, "clip") != 0;
    if (x_vis && y_scrolls) {
        ns_css_value_free(out->values[NS_CSS_OVERFLOW_X]);
        out->values[NS_CSS_OVERFLOW_X] = keyword_value(g_strdup("auto"));
    } else if (y_vis && x_scrolls) {
        ns_css_value_free(out->values[NS_CSS_OVERFLOW_Y]);
        out->values[NS_CSS_OVERFLOW_Y] = keyword_value(g_strdup("auto"));
    }
}

static ns_css_value *
initial_value_of(int prop)
{
    static __thread ns_css_value *parsed[NS_CSS_PROP_COUNT];
    static __thread gboolean tried[NS_CSS_PROP_COUNT];
    if (!tried[prop]) {
        tried[prop] = TRUE;
        const char *text = ns_css_initial_value_text(ns_css_prop_name(prop));
        if (text) parsed[prop] = ns_css_parse_value_for((ns_css_prop)prop, text);
    }
    return ns_css_value_dup(parsed[prop]);
}

static void
cascade_for(GArray *matches, ns_style *out, const ns_style *parent_style,
            const ns_style *layout_parent, gboolean is_root, double root_px)
{
    g_array_sort(matches, match_cmp);
    for (guint i = 0; i < matches->len; i++) {
        match_entry *m = &g_array_index(matches, match_entry, i);
        if (value_is_revert(m->value) || value_is_revert_layer(m->value) ||
            value_is_revert_rule(m->value)) {
            const ns_css_value *fallback =
                cascade_rollback_value(matches, (gint)i - 1, m);
            ns_css_value_free(out->values[m->prop]);
            out->values[m->prop] = ns_css_value_dup(fallback);
            continue;
        }
        ns_css_value_free(out->values[m->prop]);
        out->values[m->prop] = ns_css_value_dup(m->value);
    }
    gboolean explicit_initial[NS_CSS_PROP_COUNT] = {0};
    overflow_pair_normalize(out);
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++) {
        if (value_is_inherit(out->values[i])) {
            ns_css_value_free(out->values[i]);
            out->values[i] = parent_style && parent_style->values[i]
                             ? ns_css_value_dup(parent_style->values[i])
                             : NULL;
        } else if (value_is_initial(out->values[i])) {
            ns_css_value_free(out->values[i]);
            out->values[i] = prop_inherits((ns_css_prop)i) ? initial_value_of(i)
                                                           : NULL;
            explicit_initial[i] = TRUE;
        } else if (value_is_unset(out->values[i])) {
            ns_css_value_free(out->values[i]);
            out->values[i] = NULL;
        }
    }
    if (parent_style) {
        for (int i = 0; i < NS_CSS_PROP_COUNT; i++) {
            if (out->values[i]) continue;
            if (explicit_initial[i]) continue;
            if (!prop_inherits((ns_css_prop)i)) continue;
            if (parent_style->values[i])
                out->values[i] = ns_css_value_dup(parent_style->values[i]);
        }
    }
    if (ns_css_keyword_is(out->values[NS_CSS_COLOR], "currentcolor")) {
        ns_css_value_free(out->values[NS_CSS_COLOR]);
        out->values[NS_CSS_COLOR] = parent_style
            ? ns_css_value_dup(parent_style->values[NS_CSS_COLOR])
            : initial_value_of(NS_CSS_COLOR);
    }
    if (ns_css_keyword_is(out->values[NS_CSS_FONT_WEIGHT], "bolder") ||
        ns_css_keyword_is(out->values[NS_CSS_FONT_WEIGHT], "lighter")) {
        int parent_weight = parent_style
            ? ns_css_font_weight_number(
                  parent_style->values[NS_CSS_FONT_WEIGHT], 400)
            : 400;
        gboolean bolder =
            ns_css_keyword_is(out->values[NS_CSS_FONT_WEIGHT], "bolder");
        ns_css_value_free(out->values[NS_CSS_FONT_WEIGHT]);
        out->values[NS_CSS_FONT_WEIGHT] = keyword_value(
            g_strdup_printf("%d", ns_css_font_weight_relative(parent_weight, bolder)));
    }
    {
        const ns_css_prop color_props[] = {
            NS_CSS_BACKGROUND_COLOR,
            NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
            NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
            NS_CSS_OUTLINE_COLOR,
            NS_CSS_TEXT_DECORATION_COLOR,
            NS_CSS_COLUMN_RULE_COLOR,
            NS_CSS_ACCENT_COLOR,
            NS_CSS_CARET_COLOR,
            NS_CSS_FILL,
            NS_CSS_STROKE,
            NS_CSS_STOP_COLOR,
        };
        for (gsize i = 0; i < G_N_ELEMENTS(color_props); i++) {
            ns_css_value *v = out->values[color_props[i]];
            if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) continue;
            if (strcmp(v->u.keyword, "currentcolor") == 0) {
                ns_css_value_free(out->values[color_props[i]]);
                out->values[color_props[i]] = out->values[NS_CSS_COLOR]
                    ? ns_css_value_dup(out->values[NS_CSS_COLOR])
                    : NULL;
                out->currentcolor_bits |= 1u << i;
            } else if (strcmp(v->u.keyword, "transparent") == 0) {
                ns_css_value_free(out->values[color_props[i]]);
                ns_css_value *t = g_new0(ns_css_value, 1);
                t->kind = NS_CSS_V_COLOR;
                t->u.color.r = t->u.color.g = t->u.color.b = 0;
                t->u.color.a = 0;
                out->values[color_props[i]] = t;
            }
        }
        const ns_css_prop shadow_props[] = { NS_CSS_BOX_SHADOW, NS_CSS_TEXT_SHADOW };
        const ns_css_value *cur = out->values[NS_CSS_COLOR];
        for (gsize i = 0; i < G_N_ELEMENTS(shadow_props); i++) {
            ns_css_value *v = out->values[shadow_props[i]];
            if (!v || v->kind != NS_CSS_V_SHADOW) continue;
            gboolean needs = FALSE;
            for (int k = 0; k < v->u.shadow.n; k++)
                if (v->u.shadow.s[k].currentcolor) needs = TRUE;
            if (!needs) continue;
            v = ns_css_value_cow(out, shadow_props[i]);
            for (int k = 0; k < v->u.shadow.n; k++) {
                ns_css_shadow *sh = &v->u.shadow.s[k];
                if (!sh->currentcolor) continue;
                sh->currentcolor = FALSE;
                if (cur && cur->kind == NS_CSS_V_COLOR) {
                    sh->r = cur->u.color.r;
                    sh->g = cur->u.color.g;
                    sh->b = cur->u.color.b;
                    sh->a = cur->u.color.a;
                } else {
                    sh->r = sh->g = sh->b = 0;
                    sh->a = 255;
                }
            }
        }
    }
    {
        const ns_css_value *disp = out->values[NS_CSS_DISPLAY];
        ns_display d = { .outer = NS_DISPLAY_OUTER_INLINE };
        if (disp && disp->kind == NS_CSS_V_KEYWORD && disp->u.keyword)
            d = ns_css_display_from_keyword(disp->u.keyword);
        out->specified_inline = d.box == NS_DISPLAY_BOX_NORMAL &&
                                d.outer == NS_DISPLAY_OUTER_INLINE;
        ns_display used = display_after_blockification(
            legacy_webkit_box_display(out, d), out, layout_parent, is_root);
        if (memcmp(&d, &used, sizeof d) != 0) {
            ns_css_value *nv = g_new0(ns_css_value, 1);
            nv->kind = NS_CSS_V_KEYWORD;
            nv->u.keyword = ns_css_display_serialize(used);
            ns_css_value_free(out->values[NS_CSS_DISPLAY]);
            out->values[NS_CSS_DISPLAY] = nv;
        }
        out->display = used;
    }
    resolve_em_units(out, parent_style, root_px);
}

#define NS_CSS_MAX_CASCADE_DEPTH 512

static GHashTable *g_decl_sheet_cache;

static const ns_css_stylesheet *
ns_css_cached_decl_sheet(const char *decls)
{
    if (!decls || !*decls) return NULL;
    if (!g_decl_sheet_cache)
        g_decl_sheet_cache = g_hash_table_new_full(
            g_str_hash, g_str_equal, g_free,
            (GDestroyNotify)ns_css_stylesheet_free);
    ns_css_stylesheet *s = g_hash_table_lookup(g_decl_sheet_cache, decls);
    if (s) return s;
    char *wrapped = g_strconcat("* { ", decls, " }", NULL);
    s = ns_css_stylesheet_parse(wrapped, -1);
    g_free(wrapped);
    if (s) g_hash_table_insert(g_decl_sheet_cache, g_strdup(decls), s);
    return s;
}

static void
cascade_walk(ns_node *node,
             const ns_css_stylesheet *ua,
             const ns_css_stylesheet *const *author, gsize n_author,
             const ns_style *parent_style,
             const ns_style *layout_parent,
             double *root_px,
             GHashTable *layer_ranks,
             GHashTable *out,
             gboolean under_dirty);

static GHashTable    *g_incr_prev_styles;
static GHashTable    *g_incr_before_styles;

const ns_style *
ns_css_style_before_change(const void *node)
{
    if (!node || !g_incr_before_styles) return NULL;
    return g_hash_table_lookup(g_incr_before_styles, node);
}
static ns_node       *g_incr_prev_doc;
static guint64        g_incr_prev_sig;
static guint64        g_incr_prev_cq_sig;
static const ns_node *g_incr_prev_focus;
static const ns_node *g_incr_prev_hover;
static const ns_node *g_incr_prev_active;
static const ns_node *g_incr_prev_fullscreen;
static GHashTable    *g_incr_dirty;
static gboolean       g_incr_pass_active;
static guint64        g_incr_has_sig;
static gboolean       g_incr_eligible;
static guint          g_incr_reused;
static guint          g_incr_recomputed;
static double         g_incr_zoom = 1.0;

static GHashTable    *g_struct_keys;
static GHashTable    *g_struct_anc_keys;
static GPtrArray     *g_struct_attrs;
static GPtrArray     *g_struct_anc_attrs;
static GHashTable    *g_sib_keys;
static GHashTable    *g_sib_attrs;
static GHashTable    *g_sib_value_attrs;
static GHashTable    *g_attr_keys;
static GHashTable    *g_class_keys;
static GHashTable    *g_id_keys;
static gboolean       g_class_keys_loose;
static gboolean       g_id_keys_loose;
static GPtrArray     *g_has_anchors;
static gboolean       g_has_cq_loose;
static gboolean       g_struct_loose;
static gboolean       g_sib_loose;
static guint64        g_struct_sig;
static gboolean       g_struct_ready;

static gboolean incr_node_matches_has_cq(const ns_node *n);

static void
incr_mark_has_region(ns_node *anchor)
{
    if (!anchor) return;
    if (!g_incr_dirty)
        g_incr_dirty = g_hash_table_new(g_direct_hash, g_direct_equal);
    for (ns_node *n = anchor; n; n = n->next_sibling)
        if (n->kind == NS_NODE_ELEMENT)
            g_hash_table_add(g_incr_dirty, n);
}

static void
incr_mark_has_subjects(ns_node *changed)
{
    if (!changed || !g_incr_eligible || g_has_cq_loose) return;
    if (!g_has_anchors || g_has_anchors->len == 0)
        return;
    if (!g_incr_dirty)
        g_incr_dirty = g_hash_table_new(g_direct_hash, g_direct_equal);
    for (ns_node *a = changed; a; a = a->parent) {
        if (a->kind == NS_NODE_ELEMENT && incr_node_matches_has_cq(a))
            incr_mark_has_region(a);
        guint scanned = 0;
        for (ns_node *s = a->prev_sibling; s; s = s->prev_sibling) {
            if (s->kind != NS_NODE_ELEMENT) continue;
            if (++scanned > 256) {
                if (a->parent)
                    g_hash_table_add(g_incr_dirty, a->parent);
                break;
            }
            if (incr_node_matches_has_cq(s))
                incr_mark_has_region(s);
        }
    }
}

void
ns_css_set_render_zoom(double zoom)
{
    g_incr_zoom = zoom > 0 ? zoom : 1.0;
}

void
ns_css_mark_restyle_dirty(ns_node *parent)
{
    if (!parent) return;
    if (!g_incr_dirty)
        g_incr_dirty = g_hash_table_new(g_direct_hash, g_direct_equal);
    g_hash_table_add(g_incr_dirty, parent);
    incr_mark_has_subjects(parent);
}

static gboolean incr_pc_is_structural(ns_css_pseudo k)
{
    switch (k) {
    case NS_CSS_PC_FIRST_CHILD: case NS_CSS_PC_LAST_CHILD:
    case NS_CSS_PC_ONLY_CHILD:  case NS_CSS_PC_ONLY_OF_TYPE:
    case NS_CSS_PC_FIRST_OF_TYPE: case NS_CSS_PC_LAST_OF_TYPE:
    case NS_CSS_PC_EMPTY:
    case NS_CSS_PC_NTH_CHILD:   case NS_CSS_PC_NTH_LAST_CHILD:
    case NS_CSS_PC_NTH_OF_TYPE: case NS_CSS_PC_NTH_LAST_OF_TYPE:
        return TRUE;
    default:
        return FALSE;
    }
}

static gboolean incr_selector_has_structural(const ns_css_selector *sel, int d);

static gboolean
incr_simple_has_structural(const ns_css_simple *c, int d)
{
    if (!c) return FALSE;
    if (d > 6) return TRUE;
    if (c->pseudos)
        for (guint i = 0; i < c->pseudos->len; i++) {
            const ns_css_pseudo_pred *p =
                &g_array_index(c->pseudos, ns_css_pseudo_pred, i);
            if (incr_pc_is_structural(p->kind)) return TRUE;
            if (p->of_group)
                for (guint gi = 0; gi < p->of_group->len; gi++)
                    if (incr_selector_has_structural(
                            g_ptr_array_index(p->of_group, gi), d + 1))
                        return TRUE;
        }
    GPtrArray *gls[3] = { c->matches_any, c->matches_none, c->has_groups };
    for (int g = 0; g < 3; g++) {
        if (!gls[g]) continue;
        for (guint gi = 0; gi < gls[g]->len; gi++) {
            const GPtrArray *grp = g_ptr_array_index(gls[g], gi);
            for (guint si = 0; grp && si < grp->len; si++)
                if (incr_selector_has_structural(
                        g_ptr_array_index(grp, si), d + 1))
                    return TRUE;
        }
    }
    return FALSE;
}

static gboolean
incr_selector_has_structural(const ns_css_selector *sel, int d)
{
    if (!sel || !sel->compounds) return FALSE;
    if (d > 6) return TRUE;
    for (guint i = 0; i < sel->compounds->len; i++)
        if (incr_simple_has_structural(g_ptr_array_index(sel->compounds, i), d))
            return TRUE;
    return FALSE;
}

static gboolean
incr_add_compound_keys(GHashTable *keys, const ns_css_simple *c)
{
    gboolean any = FALSE;
    if (!c) return FALSE;
    if (c->id && *c->id) {
        g_hash_table_add(keys, g_strconcat("#", c->id, NULL));
        any = TRUE;
    }
    if (c->classes)
        for (guint i = 0; i < c->classes->len; i++) {
            const char *cls = g_ptr_array_index(c->classes, i);
            if (cls && *cls) {
                g_hash_table_add(keys, g_strconcat(".", cls, NULL));
                any = TRUE;
            }
        }
    if (c->type && *c->type && strcmp(c->type, "*") != 0) {
        char *t = g_ascii_strdown(c->type, -1);
        g_hash_table_add(keys, g_strconcat("%", t, NULL));
        g_free(t);
        any = TRUE;
    }
    return any;
}

static void
incr_attr_dep_free(gpointer data)
{
    ns_css_attr_pred *a = data;
    g_free(a->name);
    g_free(a->value);
    g_free(a);
}

static void
incr_own_attr_deps(GPtrArray *attrs)
{
    if (!attrs) return;
    for (guint i = 0; i < attrs->len; i++) {
        const ns_css_attr_pred *source = g_ptr_array_index(attrs, i);
        ns_css_attr_pred *copy = g_new0(ns_css_attr_pred, 1);
        *copy = *source;
        copy->name = g_strdup(source->name);
        copy->value = g_strdup(source->value);
        attrs->pdata[i] = copy;
    }
}

static gboolean
incr_add_positive_compound_deps(GHashTable *keys, GPtrArray *attrs,
                                const ns_css_simple *c, int depth);

static const char *
incr_state_pseudo_attr(ns_css_pseudo k)
{
    switch (k) {
    case NS_CSS_PC_DISABLED:
    case NS_CSS_PC_ENABLED:       return "disabled";
    case NS_CSS_PC_CHECKED:       return "data-nd-checked";
    case NS_CSS_PC_REQUIRED:
    case NS_CSS_PC_OPTIONAL:      return "required";
    case NS_CSS_PC_READ_ONLY:
    case NS_CSS_PC_READ_WRITE:    return "readonly";
    case NS_CSS_PC_LINK:
    case NS_CSS_PC_VISITED:
    case NS_CSS_PC_ANY_LINK:
    case NS_CSS_PC_HOVER:
    case NS_CSS_PC_ACTIVE:
    case NS_CSS_PC_FOCUS:
    case NS_CSS_PC_FOCUS_VISIBLE:
    case NS_CSS_PC_FOCUS_WITHIN:
    case NS_CSS_PC_TARGET:
    case NS_CSS_PC_TARGET_WITHIN:
    case NS_CSS_PC_FIRST_CHILD:
    case NS_CSS_PC_LAST_CHILD:
    case NS_CSS_PC_ONLY_CHILD:
    case NS_CSS_PC_ONLY_OF_TYPE:
    case NS_CSS_PC_FIRST_OF_TYPE:
    case NS_CSS_PC_LAST_OF_TYPE:
    case NS_CSS_PC_EMPTY:
    case NS_CSS_PC_NTH_CHILD:
    case NS_CSS_PC_NTH_LAST_CHILD:
    case NS_CSS_PC_NTH_OF_TYPE:
    case NS_CSS_PC_NTH_LAST_OF_TYPE:
    case NS_CSS_PC_ROOT:
    case NS_CSS_PC_SCOPE:
    case NS_CSS_PC_DEFINED:       return "";
    default:                      return NULL;
    }
}

static void
incr_collect_sib_left(const ns_css_simple *c)
{
    gboolean handled = incr_add_compound_keys(g_sib_keys, c);
    if (c->attrs)
        for (guint i = 0; i < c->attrs->len; i++) {
            const ns_css_attr_pred *a =
                &g_array_index(c->attrs, ns_css_attr_pred, i);
            if (a->name && *a->name) {
                char *low = g_ascii_strdown(a->name, -1);
                g_hash_table_add(g_sib_attrs, g_strdup(low));
                if (a->op != NS_CSS_ATTR_PRESENT)
                    g_hash_table_add(g_sib_value_attrs, low);
                else
                    g_free(low);
                handled = TRUE;
            }
        }
    if (c->pseudos)
        for (guint i = 0; i < c->pseudos->len; i++) {
            const ns_css_pseudo_pred *p =
                &g_array_index(c->pseudos, ns_css_pseudo_pred, i);
            const char *attr = incr_state_pseudo_attr(p->kind);
            if (attr == NULL) { g_sib_loose = TRUE; }
            else if (*attr) {
                g_hash_table_add(g_sib_attrs, g_strdup(attr));
                g_hash_table_add(g_sib_value_attrs, g_strdup(attr));
            }
            handled = TRUE;
        }
    if (c->matches_any || c->matches_none || c->has_groups)
        g_sib_loose = TRUE;
    (void)handled;
}

static void incr_collect_attr_keys_selector(const ns_css_selector *sel, int depth);

static void
incr_collect_attr_keys_simple(const ns_css_simple *c, int depth)
{
    if (!c || depth > 6) return;
    if (c->attrs)
        for (guint i = 0; i < c->attrs->len; i++) {
            const ns_css_attr_pred *a =
                &g_array_index(c->attrs, ns_css_attr_pred, i);
            if (a->name && *a->name)
                g_hash_table_add(g_attr_keys, g_ascii_strdown(a->name, -1));
        }
    if (c->pseudos)
        for (guint i = 0; i < c->pseudos->len; i++) {
            const ns_css_pseudo_pred *p =
                &g_array_index(c->pseudos, ns_css_pseudo_pred, i);
            const char *attr = incr_state_pseudo_attr(p->kind);
            if (attr && *attr)
                g_hash_table_add(g_attr_keys, g_strdup(attr));
            if (p->kind == NS_CSS_PC_LANG) {
                g_hash_table_add(g_attr_keys, g_strdup("lang"));
                g_hash_table_add(g_attr_keys, g_strdup("xml:lang"));
            } else if (p->kind == NS_CSS_PC_DIR) {
                g_hash_table_add(g_attr_keys, g_strdup("dir"));
            } else if (p->kind == NS_CSS_PC_OPEN) {
                g_hash_table_add(g_attr_keys, g_strdup("open"));
            } else if (p->kind == NS_CSS_PC_POPOVER_OPEN) {
                g_hash_table_add(g_attr_keys, g_strdup("data-nd-popover-open"));
            } else if (p->kind == NS_CSS_PC_MODAL) {
                g_hash_table_add(g_attr_keys, g_strdup("data-nd-modal"));
            }
            if (p->of_group)
                for (guint gi = 0; gi < p->of_group->len; gi++)
                    incr_collect_attr_keys_selector(
                        g_ptr_array_index(p->of_group, gi), depth + 1);
        }
    GPtrArray *groups[3] = { c->matches_any, c->matches_none, c->has_groups };
    for (guint i = 0; i < G_N_ELEMENTS(groups); i++)
        if (groups[i])
            for (guint gi = 0; gi < groups[i]->len; gi++) {
                const GPtrArray *group = g_ptr_array_index(groups[i], gi);
                for (guint si = 0; group && si < group->len; si++)
                    incr_collect_attr_keys_selector(
                        g_ptr_array_index(group, si), depth + 1);
            }
}

static void
incr_collect_attr_keys_selector(const ns_css_selector *sel, int depth)
{
    if (!sel || !sel->compounds || depth > 6) return;
    for (guint i = 0; i < sel->compounds->len; i++)
        incr_collect_attr_keys_simple(g_ptr_array_index(sel->compounds, i),
                                      depth);
}

static void
incr_collect_struct_keys(const ns_css_stylesheet *sh)
{
    if (!sh || !sh->rules) return;
    for (guint ri = 0; ri < sh->rules->len; ri++) {
        const ns_css_rule *r = g_ptr_array_index(sh->rules, ri);
        if (!r || !r->selectors) continue;
        for (guint si = 0; si < r->selectors->len; si++) {
            const ns_css_selector *sel = g_ptr_array_index(r->selectors, si);
            if (!sel || !sel->compounds) continue;
            incr_collect_attr_keys_selector(sel, 0);
            guint nc = sel->compounds->len;
            for (guint ci = 0; ci < nc; ci++) {
                const ns_css_simple *c = g_ptr_array_index(sel->compounds, ci);
                ns_css_comb left = NS_CSS_COMB_NONE;
                if (sel->combinators && ci < sel->combinators->len)
                    left = g_array_index(sel->combinators, ns_css_comb, ci);
                ns_css_comb right = NS_CSS_COMB_NONE;
                if (sel->combinators && ci + 1 < sel->combinators->len)
                    right = g_array_index(sel->combinators, ns_css_comb, ci + 1);
                if (right == NS_CSS_COMB_ADJACENT || right == NS_CSS_COMB_SIBLING)
                    incr_collect_sib_left(c);
                gboolean sib_subject = (left == NS_CSS_COMB_ADJACENT ||
                                        left == NS_CSS_COMB_SIBLING);
                if (!incr_simple_has_structural(c, 0) && !sib_subject)
                    continue;
                if (incr_add_positive_compound_deps(
                        g_struct_keys, g_struct_attrs, c, 0))
                    continue;
                gboolean sib_ctx = TRUE, found = FALSE;
                for (int j = (int)ci - 1; j >= 0; j--) {
                    ns_css_comb cb = NS_CSS_COMB_NONE;
                    if (sel->combinators &&
                        (guint)(j + 1) < sel->combinators->len)
                        cb = g_array_index(sel->combinators, ns_css_comb, j + 1);
                    const ns_css_simple *jc =
                        g_ptr_array_index(sel->compounds, j);
                    if (sib_ctx && (cb == NS_CSS_COMB_ADJACENT ||
                                    cb == NS_CSS_COMB_SIBLING)) {
                        if (incr_add_positive_compound_deps(
                                g_struct_keys, g_struct_attrs, jc, 0)) {
                            found = TRUE; break;
                        }
                    } else {
                        sib_ctx = FALSE;
                        if (incr_add_positive_compound_deps(
                                g_struct_anc_keys, g_struct_anc_attrs, jc, 0)) {
                            found = TRUE; break;
                        }
                    }
                }
                if (!found) g_struct_loose = TRUE;
            }
        }
    }
}

static void incr_collect_name_keys_selector(const ns_css_selector *sel,
                                            int depth);

static void
incr_collect_name_keys_group(const GPtrArray *group, int depth)
{
    for (guint i = 0; group && i < group->len; i++)
        incr_collect_name_keys_selector(g_ptr_array_index(group, i), depth);
}

static void
incr_collect_name_keys_simple(const ns_css_simple *c, int depth)
{
    if (!c) return;
    if (depth > 6) {
        g_class_keys_loose = TRUE;
        g_id_keys_loose = TRUE;
        return;
    }
    if (c->id && *c->id)
        g_hash_table_add(g_id_keys, g_ascii_strdown(c->id, -1));
    for (guint i = 0; c->classes && i < c->classes->len; i++) {
        const char *cls = g_ptr_array_index(c->classes, i);
        if (cls && *cls)
            g_hash_table_add(g_class_keys, g_ascii_strdown(cls, -1));
    }
    for (guint i = 0; c->attrs && i < c->attrs->len; i++) {
        const ns_css_attr_pred *a =
            &g_array_index(c->attrs, ns_css_attr_pred, i);
        if (!a->name) continue;
        if (g_ascii_strcasecmp(a->name, "class") == 0)
            g_class_keys_loose = TRUE;
        else if (g_ascii_strcasecmp(a->name, "id") == 0)
            g_id_keys_loose = TRUE;
    }
    for (guint i = 0; c->pseudos && i < c->pseudos->len; i++) {
        const ns_css_pseudo_pred *pc =
            &g_array_index(c->pseudos, ns_css_pseudo_pred, i);
        if (pc->kind == NS_CSS_PC_TARGET || pc->kind == NS_CSS_PC_TARGET_WITHIN)
            g_id_keys_loose = TRUE;
        if (pc->of_group) incr_collect_name_keys_group(pc->of_group, depth + 1);
    }
    GPtrArray *groups[3] = { c->matches_any, c->matches_none, c->has_groups };
    for (guint g = 0; g < G_N_ELEMENTS(groups); g++)
        for (guint gi = 0; groups[g] && gi < groups[g]->len; gi++)
            incr_collect_name_keys_group(g_ptr_array_index(groups[g], gi),
                                         depth + 1);
}

static void
incr_collect_name_keys_selector(const ns_css_selector *sel, int depth)
{
    for (guint i = 0; sel && sel->compounds && i < sel->compounds->len; i++)
        incr_collect_name_keys_simple(g_ptr_array_index(sel->compounds, i),
                                      depth);
}

static void
incr_collect_name_keys(const ns_css_stylesheet *sh)
{
    for (guint ri = 0; sh && sh->rules && ri < sh->rules->len; ri++) {
        const ns_css_rule *r = g_ptr_array_index(sh->rules, ri);
        if (!r) continue;
        incr_collect_name_keys_group(r->selectors, 0);
        for (guint si = 0; r->scopes && si < r->scopes->len; si++) {
            const ns_css_scope *scope = g_ptr_array_index(r->scopes, si);
            incr_collect_name_keys_group(scope->roots, 0);
            incr_collect_name_keys_group(scope->limits, 0);
        }
    }
}

static gboolean
incr_name_in_keys(const char *name, gsize len, GHashTable *keys)
{
    char stack[64];
    char *low = len < sizeof stack ? stack : g_malloc(len + 1);
    for (gsize i = 0; i < len; i++) low[i] = g_ascii_tolower(name[i]);
    low[len] = '\0';
    gboolean hit = g_hash_table_contains(keys, low);
    if (low != stack) g_free(low);
    return hit;
}

static gboolean
incr_class_tokens_in_keys(const char *value)
{
    for (const char *p = value; p && *p; ) {
        while (*p && g_ascii_isspace((guchar)*p)) p++;
        const char *tok = p;
        while (*p && !g_ascii_isspace((guchar)*p)) p++;
        if (p > tok && incr_name_in_keys(tok, (gsize)(p - tok), g_class_keys))
            return TRUE;
    }
    return FALSE;
}

static gboolean
incr_id_in_keys(const char *value)
{
    return value && incr_name_in_keys(value, strlen(value), g_id_keys);
}

static gboolean
incr_name_change_unused(const ns_node *target, const char *name,
                        const char *old_value)
{
    if (!g_struct_ready || !name) return FALSE;
    if (g_ascii_strcasecmp(name, "class") == 0)
        return g_class_keys && !g_class_keys_loose &&
               !incr_class_tokens_in_keys(old_value) &&
               !incr_class_tokens_in_keys(ns_element_get_attr(target, "class"));
    if (g_ascii_strcasecmp(name, "id") == 0)
        return g_id_keys && !g_id_keys_loose &&
               !incr_id_in_keys(old_value) &&
               !incr_id_in_keys(ns_element_get_attr(target, "id"));
    return FALSE;
}

static void
incr_ensure_struct_keys(const ns_css_stylesheet *ua,
                        const ns_css_stylesheet *const *author, gsize n,
                        guint64 sig)
{
    if (g_struct_ready && g_struct_sig == sig) return;
    if (g_struct_keys) g_hash_table_remove_all(g_struct_keys);
    else g_struct_keys = g_hash_table_new_full(g_str_hash, g_str_equal,
                                               g_free, NULL);
    if (g_struct_anc_keys) g_hash_table_remove_all(g_struct_anc_keys);
    else g_struct_anc_keys = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                   g_free, NULL);
    if (g_struct_attrs) g_ptr_array_set_size(g_struct_attrs, 0);
    else g_struct_attrs = g_ptr_array_new_with_free_func(incr_attr_dep_free);
    if (g_struct_anc_attrs) g_ptr_array_set_size(g_struct_anc_attrs, 0);
    else g_struct_anc_attrs =
        g_ptr_array_new_with_free_func(incr_attr_dep_free);
    if (g_sib_keys) g_hash_table_remove_all(g_sib_keys);
    else g_sib_keys = g_hash_table_new_full(g_str_hash, g_str_equal,
                                            g_free, NULL);
    if (g_sib_attrs) g_hash_table_remove_all(g_sib_attrs);
    else g_sib_attrs = g_hash_table_new_full(g_str_hash, g_str_equal,
                                             g_free, NULL);
    if (g_sib_value_attrs) g_hash_table_remove_all(g_sib_value_attrs);
    else g_sib_value_attrs = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                   g_free, NULL);
    if (g_attr_keys) g_hash_table_remove_all(g_attr_keys);
    else g_attr_keys = g_hash_table_new_full(g_str_hash, g_str_equal,
                                             g_free, NULL);
    if (g_class_keys) g_hash_table_remove_all(g_class_keys);
    else g_class_keys = g_hash_table_new_full(g_str_hash, g_str_equal,
                                              g_free, NULL);
    if (g_id_keys) g_hash_table_remove_all(g_id_keys);
    else g_id_keys = g_hash_table_new_full(g_str_hash, g_str_equal,
                                           g_free, NULL);
    g_class_keys_loose = FALSE;
    g_id_keys_loose = FALSE;
    g_struct_loose = FALSE;
    g_sib_loose = FALSE;
    incr_collect_struct_keys(ua);
    incr_collect_name_keys(ua);
    for (gsize i = 0; i < n; i++) {
        incr_collect_struct_keys(author[i]);
        incr_collect_name_keys(author[i]);
    }
    incr_own_attr_deps(g_struct_attrs);
    incr_own_attr_deps(g_struct_anc_attrs);
    g_struct_sig = sig;
    g_struct_ready = TRUE;
}

static gboolean
incr_keyset_contains(GHashTable *keyset, char prefix, const char *name,
                     gsize len, gboolean lower)
{
    char stack[128];
    char *key = len + 2 <= sizeof stack ? stack : g_malloc(len + 2);
    key[0] = prefix;
    for (gsize i = 0; i < len; i++)
        key[i + 1] = lower ? g_ascii_tolower(name[i]) : name[i];
    key[len + 1] = '\0';
    gboolean hit = g_hash_table_contains(keyset, key);
    if (key != stack) g_free(key);
    return hit;
}

static gboolean
incr_class_list_matches_keys(const char *cls, GHashTable *keyset)
{
    for (const char *p = cls; p && *p; ) {
        while (*p && g_ascii_isspace((guchar)*p)) p++;
        const char *tok = p;
        while (*p && !g_ascii_isspace((guchar)*p)) p++;
        if (p > tok && incr_keyset_contains(keyset, '.', tok,
                                            (gsize)(p - tok), FALSE))
            return TRUE;
    }
    return FALSE;
}

static gboolean
incr_node_matches_keys(const ns_node *n, GHashTable *keyset)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !keyset ||
        g_hash_table_size(keyset) == 0)
        return FALSE;
    if (n->name && incr_keyset_contains(keyset, '%', n->name,
                                        strlen(n->name), TRUE))
        return TRUE;
    const char *id = ns_element_get_attr(n, "id");
    if (id && *id && incr_keyset_contains(keyset, '#', id, strlen(id), FALSE))
        return TRUE;
    return incr_class_list_matches_keys(ns_element_get_attr(n, "class"),
                                        keyset);
}

static gboolean
incr_attr_pred_matches(const ns_node *n, const ns_css_attr_pred *wanted)
{
    const char *value = ns_element_get_attr(n, wanted->name);
    if (wanted->op == NS_CSS_ATTR_PRESENT) return value != NULL;
    if (!value || !wanted->value) return FALSE;
    gsize value_len = strlen(value);
    gsize wanted_len = strlen(wanted->value);
    gboolean html_doc = !(n->flags & NS_NODE_XML_DOC) &&
                        !(n->flags & (NS_NODE_FOREIGN_NS | NS_NODE_SVG_NS));
    gboolean ci = wanted->case_insensitive ||
        (!wanted->case_sensitive && html_doc &&
         wanted->html_ci);
    switch (wanted->op) {
    case NS_CSS_ATTR_EQ:
        return ci ? g_ascii_strcasecmp(value, wanted->value) == 0
                  : strcmp(value, wanted->value) == 0;
    case NS_CSS_ATTR_PREFIX:
        return wanted_len > 0 && value_len >= wanted_len &&
               (ci ? g_ascii_strncasecmp(value, wanted->value,
                                         wanted_len) == 0
                   : strncmp(value, wanted->value, wanted_len) == 0);
    case NS_CSS_ATTR_SUFFIX:
        return wanted_len > 0 && value_len >= wanted_len &&
               (ci ? g_ascii_strcasecmp(value + value_len - wanted_len,
                                        wanted->value) == 0
                   : strcmp(value + value_len - wanted_len,
                            wanted->value) == 0);
    case NS_CSS_ATTR_SUBSTR:
        if (wanted_len == 0) return FALSE;
        if (!ci) return strstr(value, wanted->value) != NULL;
        for (gsize i = 0; i + wanted_len <= value_len; i++)
            if (g_ascii_strncasecmp(value + i, wanted->value,
                                    wanted_len) == 0)
                return TRUE;
        return FALSE;
    case NS_CSS_ATTR_WORD: {
        const char *p = value;
        while (*p) {
            while (*p && is_ws(*p)) p++;
            const char *token = p;
            while (*p && !is_ws(*p)) p++;
            if ((gsize)(p - token) == wanted_len &&
                (ci ? g_ascii_strncasecmp(token, wanted->value,
                                          wanted_len) == 0
                    : strncmp(token, wanted->value, wanted_len) == 0))
                return TRUE;
        }
        return FALSE;
    }
    case NS_CSS_ATTR_HYPHEN:
        return value_len >= wanted_len &&
               (ci ? g_ascii_strncasecmp(value, wanted->value,
                                         wanted_len) == 0
                   : strncmp(value, wanted->value, wanted_len) == 0) &&
               (value_len == wanted_len || value[wanted_len] == '-');
    case NS_CSS_ATTR_PRESENT:
        return TRUE;
    }
    return FALSE;
}

static gboolean
incr_node_matches_attr_preds(const ns_node *n, const GPtrArray *preds)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !preds) return FALSE;
    for (guint i = 0; i < preds->len; i++) {
        const ns_css_attr_pred *wanted = g_ptr_array_index(preds, i);
        if (!wanted || !wanted->name) continue;
        if (incr_attr_pred_matches(n, wanted)) return TRUE;
    }
    return FALSE;
}

typedef struct {
    char *type;
    char *id;
    GPtrArray *classes;
    GPtrArray *attrs;
} incr_has_anchor;

static void
incr_has_anchor_free(gpointer data)
{
    incr_has_anchor *a = data;
    g_free(a->type);
    g_free(a->id);
    g_ptr_array_free(a->classes, TRUE);
    g_ptr_array_free(a->attrs, TRUE);
    g_free(a);
}

static gboolean
incr_has_anchor_matches(const ns_node *n, const incr_has_anchor *a)
{
    if (a->type && (!n->name || g_ascii_strcasecmp(n->name, a->type) != 0))
        return FALSE;
    if (a->id) {
        const char *id = ns_element_get_attr(n, "id");
        if (!id || strcmp(id, a->id) != 0) return FALSE;
    }
    for (guint i = 0; i < a->classes->len; i++) {
        const char *cls = g_ptr_array_index(a->classes, i);
        if (!ns_node_has_class(n, cls, strlen(cls))) return FALSE;
    }
    for (guint i = 0; i < a->attrs->len; i++)
        if (!incr_attr_pred_matches(n, g_ptr_array_index(a->attrs, i)))
            return FALSE;
    return TRUE;
}

static gboolean
incr_node_matches_has_cq(const ns_node *n)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !g_has_anchors) return FALSE;
    for (guint i = 0; i < g_has_anchors->len; i++)
        if (incr_has_anchor_matches(n, g_ptr_array_index(g_has_anchors, i)))
            return TRUE;
    return FALSE;
}

static gboolean
incr_childlist_needs_flood(const ns_node *parent)
{
    if (g_struct_loose) return TRUE;
    if (g_incr_dirty && g_hash_table_contains(g_incr_dirty, parent))
        return TRUE;
    if (incr_node_matches_keys(parent, g_struct_keys) ||
        incr_node_matches_attr_preds(parent, g_struct_attrs))
        return TRUE;
    for (const ns_node *c = parent->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT &&
            (incr_node_matches_keys(c, g_struct_keys) ||
             incr_node_matches_attr_preds(c, g_struct_attrs)))
            return TRUE;
    }
    for (const ns_node *a = parent; a; a = a->parent)
        if (incr_node_matches_keys(a, g_struct_anc_keys) ||
            incr_node_matches_attr_preds(a, g_struct_anc_attrs))
            return TRUE;
    return FALSE;
}

void
ns_css_mark_childlist_dirty(ns_node *parent, ns_node *added)
{
    if (!parent) return;
    if (!g_struct_ready || incr_childlist_needs_flood(parent))
        ns_css_mark_restyle_dirty(parent);
    else if (added)
        ns_css_mark_restyle_dirty(added);
    else
        incr_mark_has_subjects(parent);
}

static gboolean
incr_mark_following_siblings(ns_node *from)
{
    int marked = 0;
    for (ns_node *s = from; s; s = s->next_sibling)
        if (s->kind == NS_NODE_ELEMENT) {
            if (++marked > 256) return FALSE;
            ns_css_mark_restyle_dirty(s);
        }
    return TRUE;
}

static gboolean
incr_old_class_is_sib(const char *old_value)
{
    if (!old_value || !*old_value || !g_sib_keys ||
        g_hash_table_size(g_sib_keys) == 0)
        return FALSE;
    char **toks = g_strsplit_set(old_value, " \t\r\n\f", -1);
    gboolean hit = FALSE;
    for (int i = 0; toks && toks[i] && !hit; i++) {
        if (!*toks[i]) continue;
        char *key = g_strconcat(".", toks[i], NULL);
        if (g_hash_table_contains(g_sib_keys, key)) hit = TRUE;
        g_free(key);
    }
    g_strfreev(toks);
    return hit;
}

static gboolean
incr_attr_key_change_is_sib(const ns_node *target, const char *name,
                            const char *old_value)
{
    if (!target || !name || !g_sib_keys) return FALSE;
    if (g_ascii_strcasecmp(name, "id") == 0) {
        const char *value = ns_element_get_attr(target, "id");
        char *new_key = value && *value ? g_strconcat("#", value, NULL) : NULL;
        char *old_key = old_value && *old_value ?
            g_strconcat("#", old_value, NULL) : NULL;
        gboolean hit = (new_key && g_hash_table_contains(g_sib_keys, new_key)) ||
                       (old_key && g_hash_table_contains(g_sib_keys, old_key));
        g_free(new_key);
        g_free(old_key);
        return hit;
    }
    if (g_ascii_strcasecmp(name, "class") != 0) return FALSE;
    if (incr_node_matches_keys(target, g_sib_keys)) return TRUE;
    return incr_old_class_is_sib(old_value);
}

void
ns_css_mark_attr_dirty(ns_node *target, const char *name, const char *old_value)
{
    if (!target) return;
    if (name && old_value &&
        g_strcmp0(ns_element_get_attr(target, name), old_value) == 0)
        return;
    if (!ns_css_attr_may_affect_style(target, name)) return;
    if (incr_name_change_unused(target, name, old_value)) return;
    if (!g_struct_ready) {
        ns_css_mark_restyle_dirty(target->parent ? target->parent : target);
        return;
    }
    gboolean sib = g_sib_loose ||
        incr_attr_key_change_is_sib(target, name, old_value);
    if (!sib && name && g_sib_attrs) {
        char *low = g_ascii_strdown(name, -1);
        if (g_hash_table_contains(g_sib_attrs, low)) {
            gboolean value_sensitive = g_sib_value_attrs &&
                g_hash_table_contains(g_sib_value_attrs, low);
            gboolean was_present = old_value != NULL;
            gboolean is_present = ns_element_get_attr(target, name) != NULL;
            sib = value_sensitive || was_present != is_present;
        }
        g_free(low);
    }
    if (sib) {
        ns_css_mark_restyle_dirty(target);
        if (target->next_sibling &&
            !incr_mark_following_siblings(target->next_sibling))
            ns_css_mark_restyle_dirty(target->parent ? target->parent : target);
    } else {
        ns_css_mark_restyle_dirty(target);
    }
}

gboolean
ns_css_attr_may_affect_style(const ns_node *target, const char *name)
{
    (void)target;
    if (!name || !*name || !g_struct_ready || !g_attr_keys) return TRUE;
    if (ns_css_is_presentational_attr(name)) return TRUE;
    char *low = g_ascii_strdown(name, -1);
    gboolean affects = g_hash_table_contains(g_attr_keys, low);
    static const char *const intrinsic[] = {
        "class", "id", "style", "hidden", "lang", "xml:lang", "dir",
        "width", "height", "src", "srcset", "sizes", "href", "type",
        "value", "checked", "selected", "open", "disabled", "readonly",
        "required", "placeholder", "multiple", "size", "rows", "cols",
        "rowspan", "colspan", "span", "start", "reversed", "wrap",
        "contenteditable", "inert", "popover", "popovertarget", "slot",
        "name", "form", "list", "min", "max", "step", "media",
    };
    for (guint i = 0; !affects && i < G_N_ELEMENTS(intrinsic); i++)
        affects = strcmp(low, intrinsic[i]) == 0;
    g_free(low);
    return affects;
}

static gboolean
incr_add_positive_subject_deps(GHashTable *keys, GPtrArray *attrs,
                               const ns_css_selector *sel, int depth);

static gboolean
incr_add_positive_compound_deps(GHashTable *keys, GPtrArray *attrs,
                                const ns_css_simple *c, int depth)
{
    if (!c || depth > 6) return FALSE;
    if (c->id && *c->id) {
        g_hash_table_add(keys, g_strconcat("#", c->id, NULL));
        return TRUE;
    }
    if (c->classes && c->classes->len > 0) {
        const char *cls = g_ptr_array_index(c->classes, 0);
        if (cls && *cls) {
            g_hash_table_add(keys, g_strconcat(".", cls, NULL));
            return TRUE;
        }
    }
    if (c->attrs && c->attrs->len > 0) {
        g_ptr_array_add(attrs, &g_array_index(c->attrs, ns_css_attr_pred, 0));
        return TRUE;
    }
    if (c->type && *c->type && strcmp(c->type, "*") != 0) {
        char *type = g_ascii_strdown(c->type, -1);
        g_hash_table_add(keys, g_strconcat("%", type, NULL));
        g_free(type);
        return TRUE;
    }
    if (!c->matches_any) return FALSE;
    for (guint gi = 0; gi < c->matches_any->len; gi++) {
        const GPtrArray *group = g_ptr_array_index(c->matches_any, gi);
        if (!group || group->len == 0) continue;
        GHashTable *group_keys = g_hash_table_new_full(
            g_str_hash, g_str_equal, g_free, NULL);
        GPtrArray *group_attrs = g_ptr_array_new();
        gboolean complete = TRUE;
        for (guint si = 0; si < group->len; si++)
            if (!incr_add_positive_subject_deps(
                    group_keys, group_attrs,
                    g_ptr_array_index(group, si), depth + 1)) {
                complete = FALSE;
                break;
            }
        if (complete) {
            GHashTableIter it;
            gpointer key;
            g_hash_table_iter_init(&it, group_keys);
            while (g_hash_table_iter_next(&it, &key, NULL))
                g_hash_table_add(keys, g_strdup(key));
            for (guint ai = 0; ai < group_attrs->len; ai++)
                g_ptr_array_add(attrs, g_ptr_array_index(group_attrs, ai));
            g_hash_table_destroy(group_keys);
            g_ptr_array_free(group_attrs, TRUE);
            return TRUE;
        }
        g_hash_table_destroy(group_keys);
        g_ptr_array_free(group_attrs, TRUE);
    }
    return FALSE;
}

static gboolean
incr_add_positive_subject_deps(GHashTable *keys, GPtrArray *attrs,
                               const ns_css_selector *sel, int depth)
{
    if (!sel || !sel->compounds || sel->compounds->len == 0)
        return FALSE;
    const ns_css_simple *subject =
        g_ptr_array_index(sel->compounds, sel->compounds->len - 1);
    return incr_add_positive_compound_deps(keys, attrs, subject, depth);
}

static gboolean incr_selector_uses_has(const ns_css_selector *sel, int depth);

static gboolean
incr_simple_uses_has(const ns_css_simple *c, int depth)
{
    if (!c) return FALSE;
    if (depth > 6) return TRUE;
    if (c->has_groups && c->has_groups->len > 0) return TRUE;
    if (c->pseudos)
        for (guint i = 0; i < c->pseudos->len; i++) {
            const ns_css_pseudo_pred *p =
                &g_array_index(c->pseudos, ns_css_pseudo_pred, i);
            if (p->of_group)
                for (guint gi = 0; gi < p->of_group->len; gi++)
                    if (incr_selector_uses_has(
                            g_ptr_array_index(p->of_group, gi), depth + 1))
                        return TRUE;
        }
    GPtrArray *groups[2] = { c->matches_any, c->matches_none };
    for (guint g = 0; g < G_N_ELEMENTS(groups); g++) {
        if (!groups[g]) continue;
        for (guint gi = 0; gi < groups[g]->len; gi++) {
            const GPtrArray *group = g_ptr_array_index(groups[g], gi);
            for (guint si = 0; group && si < group->len; si++)
                if (incr_selector_uses_has(
                        g_ptr_array_index(group, si), depth + 1))
                    return TRUE;
        }
    }
    return FALSE;
}

static gboolean
incr_selector_uses_has(const ns_css_selector *sel, int depth)
{
    if (!sel || !sel->compounds) return FALSE;
    for (guint i = 0; i < sel->compounds->len; i++)
        if (incr_simple_uses_has(
                g_ptr_array_index(sel->compounds, i), depth))
            return TRUE;
    return FALSE;
}

typedef struct incr_has_ctx {
    const ns_css_selector *sel;
    guint idx;
    const struct incr_has_ctx *outer;
} incr_has_ctx;

static void
incr_has_anchor_copy_keys(incr_has_anchor *a, const ns_css_simple *c)
{
    for (guint i = 0; c->classes && i < c->classes->len; i++) {
        const char *cls = g_ptr_array_index(c->classes, i);
        if (cls && *cls) g_ptr_array_add(a->classes, g_strdup(cls));
    }
    for (guint i = 0; c->attrs && i < c->attrs->len; i++) {
        const ns_css_attr_pred *src =
            &g_array_index(c->attrs, ns_css_attr_pred, i);
        if (!src->name) continue;
        ns_css_attr_pred *copy = g_new0(ns_css_attr_pred, 1);
        *copy = *src;
        copy->name = g_strdup(src->name);
        copy->value = g_strdup(src->value);
        g_ptr_array_add(a->attrs, copy);
    }
}

static incr_has_anchor *
incr_has_anchor_from_compound(const ns_css_simple *c)
{
    incr_has_anchor *a = g_new0(incr_has_anchor, 1);
    a->classes = g_ptr_array_new_with_free_func(g_free);
    a->attrs = g_ptr_array_new_with_free_func(incr_attr_dep_free);
    if (c->type && *c->type && strcmp(c->type, "*") != 0)
        a->type = g_ascii_strdown(c->type, -1);
    if (c->id && *c->id) a->id = g_strdup(c->id);
    incr_has_anchor_copy_keys(a, c);
    if (a->type || a->id || a->classes->len > 0 || a->attrs->len > 0)
        return a;
    incr_has_anchor_free(a);
    return NULL;
}

static gboolean incr_add_has_anchor_compound(const ns_css_simple *c,
                                             int depth);

static gboolean
incr_add_has_anchor_group(const GPtrArray *group, int depth)
{
    guint mark = g_has_anchors->len;
    for (guint si = 0; si < group->len; si++) {
        const ns_css_selector *alt = g_ptr_array_index(group, si);
        if (!alt || !alt->compounds || alt->compounds->len == 0 ||
            !incr_add_has_anchor_compound(
                g_ptr_array_index(alt->compounds, alt->compounds->len - 1),
                depth + 1)) {
            g_ptr_array_set_size(g_has_anchors, mark);
            return FALSE;
        }
    }
    return TRUE;
}

static gboolean
incr_add_has_anchor_compound(const ns_css_simple *c, int depth)
{
    if (!c || depth > 6) return FALSE;
    incr_has_anchor *a = incr_has_anchor_from_compound(c);
    if (a) {
        g_ptr_array_add(g_has_anchors, a);
        return TRUE;
    }
    for (guint gi = 0; c->matches_any && gi < c->matches_any->len; gi++) {
        const GPtrArray *group = g_ptr_array_index(c->matches_any, gi);
        if (group && group->len > 0 && incr_add_has_anchor_group(group, depth))
            return TRUE;
    }
    return FALSE;
}

static gboolean
incr_add_has_anchor_deps(const incr_has_ctx *at, int depth)
{
    for (const incr_has_ctx *cx = at; cx; cx = cx->outer) {
        for (guint i = cx->idx + 1; i-- > 0; )
            if (incr_add_has_anchor_compound(
                    g_ptr_array_index(cx->sel->compounds, i), depth))
                return TRUE;
        if (cx->idx + 1 != cx->sel->compounds->len) return FALSE;
    }
    return FALSE;
}

static gboolean incr_collect_has_anchors_selector(const ns_css_selector *sel,
                                                  const incr_has_ctx *outer,
                                                  int depth);

static gboolean
incr_collect_has_anchors_simple(const incr_has_ctx *at, int depth)
{
    const ns_css_simple *c = g_ptr_array_index(at->sel->compounds, at->idx);
    if (!c || depth > 6) return FALSE;
    gboolean found = FALSE;
    if (c->has_groups && c->has_groups->len > 0) {
        found = TRUE;
        if (!incr_add_has_anchor_deps(at, depth))
            g_has_cq_loose = TRUE;
    }
    if (c->pseudos)
        for (guint i = 0; i < c->pseudos->len; i++) {
            const ns_css_pseudo_pred *p =
                &g_array_index(c->pseudos, ns_css_pseudo_pred, i);
            if (!p->of_group) continue;
            for (guint gi = 0; gi < p->of_group->len; gi++)
                found |= incr_collect_has_anchors_selector(
                    g_ptr_array_index(p->of_group, gi), at, depth + 1);
        }
    GPtrArray *groups[2] = { c->matches_any, c->matches_none };
    for (guint g = 0; g < G_N_ELEMENTS(groups); g++) {
        if (!groups[g]) continue;
        for (guint gi = 0; gi < groups[g]->len; gi++) {
            const GPtrArray *group = g_ptr_array_index(groups[g], gi);
            for (guint si = 0; group && si < group->len; si++)
                found |= incr_collect_has_anchors_selector(
                    g_ptr_array_index(group, si), at, depth + 1);
        }
    }
    return found;
}

static gboolean
incr_collect_has_anchors_selector(const ns_css_selector *sel,
                                  const incr_has_ctx *outer, int depth)
{
    if (!sel || !sel->compounds || depth > 6) return FALSE;
    gboolean found = FALSE;
    for (guint i = 0; i < sel->compounds->len; i++) {
        incr_has_ctx at = { sel, i, outer };
        found |= incr_collect_has_anchors_simple(&at, depth);
    }
    return found;
}

static void
incr_collect_has_cq_keys(const ns_css_stylesheet *sh)
{
    if (!sh || !sh->rules) return;
    for (guint ri = 0; ri < sh->rules->len; ri++) {
        const ns_css_rule *r = g_ptr_array_index(sh->rules, ri);
        if (!r || !r->selectors) continue;
        for (guint si = 0; si < r->selectors->len; si++) {
            const ns_css_selector *sel = g_ptr_array_index(r->selectors, si);
            if (!sel || !sel->compounds || sel->compounds->len == 0) continue;
            if (!incr_selector_uses_has(sel, 0)) continue;
            if (!incr_collect_has_anchors_selector(sel, NULL, 0))
                g_has_cq_loose = TRUE;
        }
    }
}

static guint64
incr_sheet_sig(const ns_css_stylesheet *ua,
               const ns_css_stylesheet *const *author, gsize n)
{
    guint64 h = 1469598103934665603ULL;
    guint64 vals[3] = { ua ? ua->serial : 0, (guint64)n,
                        ns_css_registered_property_serial() };
    for (int i = 0; i < 3; i++) { h ^= vals[i]; h *= 1099511628211ULL; }
    for (gsize i = 0; i < n; i++) {
        h ^= author[i] ? author[i]->serial : 0;
        h *= 1099511628211ULL;
    }
    return h;
}

static GHashTable *g_style_share;
static GByteArray *g_share_scratch;
static guint64 g_style_share_next_id;

typedef struct {
    guint32  hash;
    guint32  len;
    guint8  *data;
} share_key_t;

static guint
share_key_hash(gconstpointer p)
{
    return ((const share_key_t *)p)->hash;
}

static gboolean
share_key_equal(gconstpointer a, gconstpointer b)
{
    const share_key_t *x = a, *y = b;
    return x->hash == y->hash && x->len == y->len &&
           memcmp(x->data, y->data, x->len) == 0;
}

static void
share_key_free(gpointer p)
{
    share_key_t *k = p;
    g_free(k->data);
    g_free(k);
}

static guint32
share_key_djb2(const guint8 *d, guint32 n)
{
    guint64 h = 1469598103934665603ULL;
    guint32 i = 0;
    for (; i + 8 <= n; i += 8) {
        guint64 w;
        memcpy(&w, d + i, 8);
        h = (h ^ w) * 1099511628211ULL;
    }
    for (; i < n; i++)
        h = (h ^ d[i]) * 1099511628211ULL;
    return (guint32)(h ^ (h >> 32));
}

typedef struct {
    ns_css_pseudo_element pe;
    GArray *m;
    GArray *v;
    GArray *p;
} ns_pe_gather;

static ns_var_map *
ns_style_vars_clone(ns_var_map *vars)
{
    return ns_var_map_ref(vars);
}

static ns_style *
ns_style_clone_shared(const ns_style *s)
{
    if (!s) return NULL;
    ns_style *c = ns_style_alloc();
    c->share_id = s->share_id;
    c->display  = s->display;
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++) {
        c->values[i] = s->values[i];
        if (c->values[i]) c->values[i]->ref++;
    }
    c->before       = ns_style_clone_shared(s->before);
    c->after        = ns_style_clone_shared(s->after);
    c->first_letter = ns_style_clone_shared(s->first_letter);
    c->first_line   = ns_style_clone_shared(s->first_line);
    c->placeholder  = ns_style_clone_shared(s->placeholder);
    c->selection    = ns_style_clone_shared(s->selection);
    c->marker       = ns_style_clone_shared(s->marker);
    c->backdrop     = ns_style_clone_shared(s->backdrop);
    c->file_selector_button = ns_style_clone_shared(
        s->file_selector_button);
    c->hidden_before = ns_style_clone_shared(s->hidden_before);
    c->hidden_after  = ns_style_clone_shared(s->hidden_after);
    c->vars = ns_style_vars_clone(s->vars);
    return c;
}

#define SHARE_KEY_MATCH_BYTES \
    (sizeof(int) * 9 + sizeof(((match_entry *)0)->important) + \
     sizeof(((match_entry *)0)->inline_style) + \
     sizeof(((match_entry *)0)->rule) + sizeof(((match_entry *)0)->value) + \
     sizeof(((match_entry *)0)->prop))
#define SHARE_KEY_VAR_BYTES \
    (sizeof(int) * 9 + sizeof(((var_match *)0)->important) + \
     sizeof(((var_match *)0)->inline_style) + \
     sizeof(((var_match *)0)->rule) + sizeof(((var_match *)0)->name) + \
     sizeof(((var_match *)0)->text))
#define SHARE_KEY_PENDING_BYTES \
    (sizeof(int) * 9 + sizeof(((pending_match *)0)->inline_style) + \
     sizeof(((pending_match *)0)->rule) + sizeof(((pending_match *)0)->pd))

static inline guint8 *
share_key_put_raw(guint8 *p, const void *src, gsize n)
{
    memcpy(p, src, n);
    return p + n;
}

static guint8 *
share_key_put_matches(guint8 *p, const GArray *arr)
{
    guint n = arr ? arr->len : 0;
    p = share_key_put_raw(p, &n, sizeof n);
    for (guint i = 0; i < n; i++) {
        const match_entry *e = &g_array_index((GArray *)arr, match_entry, i);
        p = share_key_put_raw(p, &e->origin, sizeof(int) * 9);
        p = share_key_put_raw(p, &e->important, sizeof e->important);
        p = share_key_put_raw(p, &e->inline_style, sizeof e->inline_style);
        p = share_key_put_raw(p, &e->rule, sizeof e->rule);
        p = share_key_put_raw(p, &e->value, sizeof e->value);
        p = share_key_put_raw(p, &e->prop, sizeof e->prop);
    }
    return p;
}

static guint8 *
share_key_put_vars(guint8 *p, const GArray *arr)
{
    guint n = arr ? arr->len : 0;
    p = share_key_put_raw(p, &n, sizeof n);
    for (guint i = 0; i < n; i++) {
        const var_match *e = &g_array_index((GArray *)arr, var_match, i);
        p = share_key_put_raw(p, &e->origin, sizeof(int) * 9);
        p = share_key_put_raw(p, &e->important, sizeof e->important);
        p = share_key_put_raw(p, &e->inline_style, sizeof e->inline_style);
        p = share_key_put_raw(p, &e->rule, sizeof e->rule);
        p = share_key_put_raw(p, &e->name, sizeof e->name);
        p = share_key_put_raw(p, &e->text, sizeof e->text);
    }
    return p;
}

static guint8 *
share_key_put_pending(guint8 *p, const GArray *arr)
{
    guint n = arr ? arr->len : 0;
    p = share_key_put_raw(p, &n, sizeof n);
    for (guint i = 0; i < n; i++) {
        const pending_match *e = &g_array_index((GArray *)arr, pending_match, i);
        p = share_key_put_raw(p, &e->origin, sizeof(int) * 9);
        p = share_key_put_raw(p, &e->inline_style, sizeof e->inline_style);
        p = share_key_put_raw(p, &e->rule, sizeof e->rule);
        p = share_key_put_raw(p, &e->pd, sizeof e->pd);
    }
    return p;
}

static gboolean
container_relative_unit(ns_css_unit unit)
{
    return unit == NS_CSS_UNIT_CQW || unit == NS_CSS_UNIT_CQH ||
           unit == NS_CSS_UNIT_CQMIN || unit == NS_CSS_UNIT_CQMAX;
}

static gboolean
share_matches_need_container(const GArray *matches)
{
    if (!matches) return FALSE;
    for (guint i = 0; i < matches->len; i++) {
        const match_entry *e = &g_array_index((GArray *)matches,
                                               match_entry, i);
        if (e->prop == NS_CSS_FONT_SIZE && e->value &&
            e->value->kind == NS_CSS_V_LENGTH &&
            container_relative_unit(e->value->u.length.unit))
            return TRUE;
    }
    return FALSE;
}

static gboolean
share_vars_need_container(const GArray *matches)
{
    if (!matches) return FALSE;
    for (guint i = 0; i < matches->len; i++) {
        const var_match *e = &g_array_index((GArray *)matches, var_match, i);
        if (ns_css_text_has_container_units(e->text, -1)) return TRUE;
    }
    return FALSE;
}

static gboolean
share_pending_need_container(const GArray *matches)
{
    if (!matches) return FALSE;
    for (guint i = 0; i < matches->len; i++) {
        const pending_match *e = &g_array_index((GArray *)matches,
                                                 pending_match, i);
        if (e->pd && ns_css_text_has_container_units(e->pd->raw_vtext, -1))
            return TRUE;
    }
    return FALSE;
}

static gboolean
share_key_needs_container(const GArray *matches,
                          const GArray *var_matches,
                          const GArray *pending_matches,
                          const ns_pe_gather *pe_g, int n_pe)
{
    if (share_matches_need_container(matches) ||
        share_vars_need_container(var_matches) ||
        share_pending_need_container(pending_matches))
        return TRUE;
    for (int i = 0; i < n_pe; i++)
        if (share_matches_need_container(pe_g[i].m) ||
            share_vars_need_container(pe_g[i].v) ||
            share_pending_need_container(pe_g[i].p))
            return TRUE;
    return FALSE;
}

static gsize
share_key_arrays_bytes(const GArray *matches, const GArray *var_matches,
                       const GArray *pending_matches)
{
    return sizeof(guint) * 3 +
           (matches ? matches->len : 0) * SHARE_KEY_MATCH_BYTES +
           (var_matches ? var_matches->len : 0) * SHARE_KEY_VAR_BYTES +
           (pending_matches ? pending_matches->len : 0) *
               SHARE_KEY_PENDING_BYTES;
}

static void
style_share_key(GByteArray *b,
                const ns_style *parent_style, double root_px,
                const GArray *matches, const GArray *var_matches,
                const GArray *pending_matches,
                const ns_pe_gather *pe_g, int n_pe)
{
    gsize cq_bytes = ns_css_container_stack_copy(NULL, 0);
    if (cq_bytes && !share_key_needs_container(matches, var_matches,
                                               pending_matches, pe_g, n_pe))
        cq_bytes = 0;
    guint cq_len = (guint)(cq_bytes / NS_CSS_CONTAINER_BYTES);

    gsize need = sizeof(guint64) + sizeof(double) + sizeof(guint) +
                 cq_bytes +
                 share_key_arrays_bytes(matches, var_matches, pending_matches);
    for (int i = 0; i < n_pe; i++)
        need += sizeof(guint) +
                share_key_arrays_bytes(pe_g[i].m, pe_g[i].v, pe_g[i].p);
    if (b->len < need) g_byte_array_set_size(b, (guint)need);

    guint8 *p = b->data;
    guint64 parent_id = parent_style ? parent_style->share_id : 0;
    p = share_key_put_raw(p, &parent_id, sizeof parent_id);
    p = share_key_put_raw(p, &root_px, sizeof root_px);
    p = share_key_put_raw(p, &cq_len, sizeof cq_len);
    if (cq_len) {
        ns_css_container_stack_copy(p, cq_bytes);
        p += cq_bytes;
    }
    p = share_key_put_matches(p, matches);
    p = share_key_put_vars(p, var_matches);
    p = share_key_put_pending(p, pending_matches);
    for (int i = 0; i < n_pe; i++) {
        guint pe = (guint)pe_g[i].pe;
        p = share_key_put_raw(p, &pe, sizeof pe);
        p = share_key_put_matches(p, pe_g[i].m);
        p = share_key_put_vars(p, pe_g[i].v);
        p = share_key_put_pending(p, pe_g[i].p);
    }
    b->len = (guint)(p - b->data);
}

static gboolean
element_cannot_be_unboxed(const ns_node *el)
{
    if (el->kind != NS_NODE_ELEMENT || !el->name) return FALSE;
    if (el->flags & NS_NODE_SVG_NS)
        return strcmp(el->name, "svg") == 0 && el->parent &&
               !(el->parent->flags & NS_NODE_SVG_NS);
    if (el->flags & NS_NODE_FOREIGN_NS) return FALSE;
    static const char *const unusual[] = {
        "audio", "br", "canvas", "embed", "frame", "frameset", "iframe",
        "img", "input", "meter", "object", "progress", "select",
        "textarea", "video", "wbr",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(unusual); i++)
        if (g_ascii_strcasecmp(el->name, unusual[i]) == 0) return TRUE;
    return FALSE;
}

static gboolean
display_contents_to_none(const ns_node *el, ns_style *s)
{
    if (s->display.box != NS_DISPLAY_BOX_CONTENTS ||
        !element_cannot_be_unboxed(el))
        return FALSE;
    ns_css_value_free(s->values[NS_CSS_DISPLAY]);
    s->values[NS_CSS_DISPLAY] = keyword_value_dup("none");
    s->display = ns_css_display_from_keyword("none");
    return TRUE;
}

static void
strip_native_widget_decorations(const ns_node *el, ns_style *s)
{
    if (!ns_node_is_element_named(el, "input")) return;
    const char *type = ns_element_get_attr(el, "type");
    if (!type || (g_ascii_strcasecmp(type, "checkbox") != 0 &&
                  g_ascii_strcasecmp(type, "radio") != 0))
        return;
    const ns_css_value *ap = s->values[NS_CSS_APPEARANCE];
    if (ap && ap->kind == NS_CSS_V_KEYWORD && ap->u.keyword &&
        strcmp(ap->u.keyword, "none") == 0)
        return;
    static const ns_css_prop stripped[] = {
        NS_CSS_BACKGROUND_COLOR, NS_CSS_BACKGROUND_IMAGE,
        NS_CSS_BORDER_TOP_WIDTH, NS_CSS_BORDER_RIGHT_WIDTH,
        NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_LEFT_WIDTH,
        NS_CSS_BORDER_TOP_STYLE, NS_CSS_BORDER_RIGHT_STYLE,
        NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_BORDER_LEFT_STYLE,
        NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
        NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
        NS_CSS_BORDER_TOP_LEFT_RADIUS, NS_CSS_BORDER_TOP_RIGHT_RADIUS,
        NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS, NS_CSS_BORDER_BOTTOM_LEFT_RADIUS,
        NS_CSS_PADDING_TOP, NS_CSS_PADDING_RIGHT,
        NS_CSS_PADDING_BOTTOM, NS_CSS_PADDING_LEFT,
        NS_CSS_BOX_SHADOW,
    };
    for (gsize i = 0; i < G_N_ELEMENTS(stripped); i++) {
        if (s->values[stripped[i]]) {
            ns_css_value_free(s->values[stripped[i]]);
            s->values[stripped[i]] = NULL;
        }
    }
}

static double
frame_edge_px(const ns_style *s, ns_css_prop prop)
{
    const ns_css_value *v = s->values[prop];
    if (!v || v->kind != NS_CSS_V_LENGTH || v->u.length.unit != NS_CSS_UNIT_PX)
        return 0;
    return v->u.length.v;
}

static double
frame_border_px(const ns_style *s, ns_css_prop width_prop,
                ns_css_prop style_prop)
{
    const ns_css_value *st = s->values[style_prop];
    if (!st || (st->kind == NS_CSS_V_KEYWORD && st->u.keyword &&
                (strcmp(st->u.keyword, "none") == 0 ||
                 strcmp(st->u.keyword, "hidden") == 0)))
        return 0;
    return frame_edge_px(s, width_prop);
}

static gboolean
frame_viewport_from_style(const ns_style *s, double *w, double *h)
{
    if (!s) return FALSE;
    const ns_css_value *wv = s->values[NS_CSS_WIDTH];
    const ns_css_value *hv = s->values[NS_CSS_HEIGHT];
    if (!wv || wv->kind != NS_CSS_V_LENGTH ||
        wv->u.length.unit != NS_CSS_UNIT_PX ||
        !hv || hv->kind != NS_CSS_V_LENGTH ||
        hv->u.length.unit != NS_CSS_UNIT_PX)
        return FALSE;
    double fw = wv->u.length.v, fh = hv->u.length.v;
    if (ns_css_keyword_is(s->values[NS_CSS_BOX_SIZING], "border-box")) {
        fw -= frame_edge_px(s, NS_CSS_PADDING_LEFT) +
              frame_edge_px(s, NS_CSS_PADDING_RIGHT) +
              frame_border_px(s, NS_CSS_BORDER_LEFT_WIDTH,
                              NS_CSS_BORDER_LEFT_STYLE) +
              frame_border_px(s, NS_CSS_BORDER_RIGHT_WIDTH,
                              NS_CSS_BORDER_RIGHT_STYLE);
        fh -= frame_edge_px(s, NS_CSS_PADDING_TOP) +
              frame_edge_px(s, NS_CSS_PADDING_BOTTOM) +
              frame_border_px(s, NS_CSS_BORDER_TOP_WIDTH,
                              NS_CSS_BORDER_TOP_STYLE) +
              frame_border_px(s, NS_CSS_BORDER_BOTTOM_WIDTH,
                              NS_CSS_BORDER_BOTTOM_STYLE);
    }
    if (fw <= 0 || fh <= 0) return FALSE;
    *w = fw;
    *h = fh;
    return TRUE;
}

/* The sheets of each document, when the caller of ns_css_compute() says
 * whose each sheet is. */
static __thread GHashTable *g_doc_sheets;

static GHashTable *
doc_sheets_new(const ns_css_stylesheet *const *sheets,
               const ns_node *const *docs, gsize n)
{
    if (!docs) return NULL;
    GHashTable *map = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                            NULL,
                                            (GDestroyNotify)g_ptr_array_unref);
    for (gsize i = 0; i < n; i++) {
        GPtrArray *own = g_hash_table_lookup(map, docs[i]);
        if (!own) {
            own = g_ptr_array_new();
            g_hash_table_insert(map, (gpointer)docs[i], own);
        }
        g_ptr_array_add(own, (gpointer)sheets[i]);
    }
    return map;
}

/* The elements of a document are styled by its own sheets only, not by
 * those of the document its frame is in, nor by those of its frames'
 * documents. */
static void
doc_own_sheets(const ns_node *node, const ns_css_stylesheet *const **author,
               gsize *n_author)
{
    if (!g_doc_sheets || node->kind != NS_NODE_DOCUMENT) return;
    const GPtrArray *own = g_hash_table_lookup(g_doc_sheets, node);
    *author = own ? (const ns_css_stylesheet *const *)own->pdata : NULL;
    *n_author = own ? own->len : 0;
}

static void
cascade_walk(ns_node *node,
             const ns_css_stylesheet *ua,
             const ns_css_stylesheet *const *author, gsize n_author,
             const ns_style *parent_style,
             const ns_style *layout_parent,
             double *root_px,
             GHashTable *layer_ranks,
             GHashTable *out,
             gboolean under_dirty)
{
    static int depth;
    if (depth >= NS_CSS_MAX_CASCADE_DEPTH) return;
    depth++;
    double frame_vw = 0, frame_vh = 0;
    gboolean frame_viewport = FALSE;
    if (node->kind == NS_NODE_DOCUMENT && node->parent && g_frame_viewport_cb) {
        double fw = 0, fh = 0;
        if (!frame_viewport_from_style(parent_style, &fw, &fh))
            g_frame_viewport_cb(node->parent, &fw, &fh);
        if (fw > 0 && fh > 0 &&
            (fabs(fw - g_viewport_w) > 0.01 ||
             fabs(fh - g_viewport_h) > 0.01)) {
            frame_vw = g_viewport_w;
            frame_vh = g_viewport_h;
            g_viewport_w = fw;
            g_viewport_h = fh;
            frame_viewport = TRUE;
        }
    }
    doc_own_sheets(node, &author, &n_author);
    const ns_style *child_parent_style = parent_style;
    const ns_style *child_layout_parent = layout_parent;
    gboolean nd_recurse_dirty = under_dirty;
    if (node->kind == NS_NODE_ELEMENT) {
        gboolean nd_node_dirty = under_dirty ||
            (g_incr_dirty && g_hash_table_contains(g_incr_dirty, node)) ||
            (g_incr_exclude && g_hash_table_contains(g_incr_exclude, node));
        ns_style *nd_prev =
            (g_incr_pass_active && !nd_node_dirty && g_incr_prev_styles)
            ? g_hash_table_lookup(g_incr_prev_styles, node) : NULL;
        ns_style *s;
        if (nd_prev) {
            s = nd_prev;
            s->ref++;
            g_incr_reused++;
        } else {
        s = ns_style_alloc();
        g_incr_recomputed++;
        nd_node_dirty = TRUE;
        static GArray *sc_matches, *sc_var, *sc_pending;
        static GPtrArray *sc_owned;
        static GArray *sc_pe_m[9], *sc_pe_v[9], *sc_pe_p[9];
        if (!sc_matches) {
            sc_matches  = g_array_new(FALSE, FALSE, sizeof(match_entry));
            sc_var      = g_array_new(FALSE, FALSE, sizeof(var_match));
            sc_pending  = g_array_new(FALSE, FALSE, sizeof(pending_match));
            sc_owned    = g_ptr_array_new_with_free_func(
                              (GDestroyNotify)ns_css_value_free);
        }
        GArray *matches = sc_matches;
        GArray *var_matches = sc_var;
        GArray *pending_matches = sc_pending;
        GPtrArray *owned_values = sc_owned;
        g_array_set_size(matches, 0);
        g_array_set_size(var_matches, 0);
        g_array_set_size(pending_matches, 0);
        g_ptr_array_set_size(owned_values, 0);
        guint pe_mask = ua ? ua->pseudo_mask : 0;
        for (gsize i = 0; i < n_author; i++)
            if (author[i]) pe_mask |= author[i]->pseudo_mask;
        ns_pe_gather pe_g[9];
        int n_pe = 0;
        gather_dest dests[10];
        dests[0].pe = NS_CSS_PE_NONE;
        dests[0].out = matches;
        dests[0].var_out = var_matches;
        dests[0].pending_out = pending_matches;
        for (int pi = 0; pe_mask && pi < 9; pi++) {
            ns_css_pseudo_element pe = (pi == 0) ? NS_CSS_PE_BEFORE :
                                       (pi == 1) ? NS_CSS_PE_AFTER :
                                       (pi == 2) ? NS_CSS_PE_FIRST_LETTER :
                                       (pi == 3) ? NS_CSS_PE_FIRST_LINE :
                                       (pi == 4) ? NS_CSS_PE_SELECTION :
                                       (pi == 5) ? NS_CSS_PE_MARKER :
                                       (pi == 6) ? NS_CSS_PE_BACKDROP :
                                       (pi == 7) ? NS_CSS_PE_PLACEHOLDER :
                                                   NS_CSS_PE_FILE_SELECTOR_BUTTON;
            if (!(pe_mask & (1u << pe))) continue;
            ns_pe_gather *pg = &pe_g[n_pe];
            pg->pe = pe;
            if (!sc_pe_m[n_pe]) {
                sc_pe_m[n_pe] = g_array_new(FALSE, FALSE, sizeof(match_entry));
                sc_pe_v[n_pe] = g_array_new(FALSE, FALSE, sizeof(var_match));
                sc_pe_p[n_pe] = g_array_new(FALSE, FALSE, sizeof(pending_match));
            }
            pg->m = sc_pe_m[n_pe];
            pg->v = sc_pe_v[n_pe];
            pg->p = sc_pe_p[n_pe];
            g_array_set_size(pg->m, 0);
            g_array_set_size(pg->v, 0);
            g_array_set_size(pg->p, 0);
            dests[n_pe + 1].pe = pe;
            dests[n_pe + 1].out = pg->m;
            dests[n_pe + 1].var_out = pg->v;
            dests[n_pe + 1].pending_out = pg->p;
            n_pe++;
        }
        g_ancestor_filter_subject = node;
        gather_matches_multi(ua, NS_CSS_ORIGIN_UA, 0, node, dests,
                             (guint)n_pe + 1,
                             layer_ranks);
        for (gsize i = 0; i < n_author; i++)
            gather_matches_multi(author[i], NS_CSS_ORIGIN_AUTHOR,
                                 (int)(i + 1), node, dests,
                                 (guint)n_pe + 1, layer_ranks);

        char *pres_css = ns_css_presentational_hints(node);
        const ns_css_stylesheet *pres_sheet = NULL;
        if (pres_css) {
            pres_sheet = ns_css_cached_decl_sheet(pres_css);
            g_free(pres_css);
        }
        if (pres_sheet) {
            for (guint ri = 0; ri < pres_sheet->rules->len; ri++) {
                ns_css_rule *r = g_ptr_array_index(pres_sheet->rules, ri);
                for (guint di = 0; di < r->decls->len; di++) {
                    ns_css_decl *d = &g_array_index(r->decls, ns_css_decl, di);
                    match_entry e = {
                        .origin = NS_CSS_ORIGIN_PRESENTATIONAL,
                        .spec_a = 0, .spec_b = 0, .spec_c = 0,
                        .layer_order = NS_CSS_LAYER_NONE,
                        .source_order = INT_MIN,
                        .decl_order = css_decl_slot(di),
                        .important = d->important,
                        .rule = r,
                        .value = d->value,
                        .prop  = d->prop,
                    };
                    g_array_append_val(matches, e);
                }
                if (r->vars) {
                    GHashTableIter it; gpointer k, v; int di_v = 0;
                    g_hash_table_iter_init(&it, r->vars);
                    while (g_hash_table_iter_next(&it, &k, &v)) {
                        var_match vm = {
                            .origin = NS_CSS_ORIGIN_PRESENTATIONAL,
                            .spec_a = 0, .spec_b = 0, .spec_c = 0,
                            .sheet_index = 0,
                            .layer_order = NS_CSS_LAYER_NONE,
                            .source_order = INT_MIN,
                            .decl_order = di_v++,
                            .important = r->var_important &&
                                g_hash_table_contains(r->var_important, k),
                            .rule = r,
                            .name = (const char *)k,
                            .text = (const char *)v,
                        };
                        g_array_append_val(var_matches, vm);
                    }
                }
                if (r->pending) {
                    for (guint pi = 0; pi < r->pending->len; pi++) {
                        ns_css_pending_decl *pd =
                            &g_array_index(r->pending, ns_css_pending_decl, pi);
                        pending_match pm = {
                            .origin = NS_CSS_ORIGIN_PRESENTATIONAL,
                            .spec_a = 0, .spec_b = 0, .spec_c = 0,
                            .sheet_index = 0,
                            .layer_order = NS_CSS_LAYER_NONE,
                            .source_order = INT_MIN,
                            .decl_order_base = css_pending_decl_slot(pd),
                            .rule = r,
                            .pd = pd,
                        };
                        g_array_append_val(pending_matches, pm);
                    }
                }
            }
        }

        const char *inline_css = ns_element_get_attr(node, "style");
        const ns_css_stylesheet *inline_sheet = NULL;
        if (inline_css && *inline_css)
            inline_sheet = ns_css_cached_decl_sheet(inline_css);
        if (inline_sheet) {
            for (guint ri = 0; ri < inline_sheet->rules->len; ri++) {
                ns_css_rule *r = g_ptr_array_index(inline_sheet->rules, ri);
                for (guint di = 0; di < r->decls->len; di++) {
                    ns_css_decl *d = &g_array_index(r->decls, ns_css_decl, di);
                    match_entry e = {
                        .origin = NS_CSS_ORIGIN_AUTHOR,
                        .spec_a = 1000, .spec_b = 0, .spec_c = 0,
                        .layer_order = NS_CSS_LAYER_NONE,
                        .source_order = INT_MAX,
                        .decl_order = css_decl_slot(di),
                        .important = d->important,
                        .inline_style = TRUE,
                        .rule = r,
                        .value = d->value,
                        .prop  = d->prop,
                    };
                    g_array_append_val(matches, e);
                }
                if (r->vars) {
                    GHashTableIter it; gpointer k, v; int di_v = 0;
                    g_hash_table_iter_init(&it, r->vars);
                    while (g_hash_table_iter_next(&it, &k, &v)) {
                        var_match vm = {
                            .origin = NS_CSS_ORIGIN_AUTHOR,
                            .spec_a = 1000, .spec_b = 0, .spec_c = 0,
                            .sheet_index = 0,
                            .layer_order = NS_CSS_LAYER_NONE,
                            .source_order = INT_MAX,
                            .decl_order = di_v++,
                            .important = r->var_important &&
                                g_hash_table_contains(r->var_important, k),
                            .inline_style = TRUE,
                            .rule = r,
                            .name = (const char *)k,
                            .text = (const char *)v,
                        };
                        g_array_append_val(var_matches, vm);
                    }
                }
                if (r->pending) {
                    for (guint pi = 0; pi < r->pending->len; pi++) {
                        ns_css_pending_decl *pd =
                            &g_array_index(r->pending, ns_css_pending_decl, pi);
                        pending_match pm = {
                            .origin = NS_CSS_ORIGIN_AUTHOR,
                            .spec_a = 1000, .spec_b = 0, .spec_c = 0,
                            .sheet_index = 0,
                            .layer_order = NS_CSS_LAYER_NONE,
                            .source_order = INT_MAX,
                            .decl_order_base = css_pending_decl_slot(pd),
                            .inline_style = TRUE,
                            .rule = r,
                            .pd = pd,
                        };
                        g_array_append_val(pending_matches, pm);
                    }
                }
            }
        }

        share_key_t probe;
        gboolean have_key = FALSE;
        const ns_style *shared = NULL;
        if (g_style_share) {
            style_share_key(g_share_scratch, parent_style, *root_px, matches,
                            var_matches, pending_matches, pe_g, n_pe);
            probe.data = g_share_scratch->data;
            probe.len  = g_share_scratch->len;
            probe.hash = share_key_djb2(probe.data, probe.len);
            have_key = TRUE;
            shared = g_hash_table_lookup(g_style_share, &probe);
            gboolean uses_attr = pending_uses_attr(pending_matches);
            for (int i = 0; i < n_pe && !uses_attr; i++)
                uses_attr = pending_uses_attr(pe_g[i].p);
            if (uses_attr) {
                have_key = FALSE;
                shared = NULL;
                ns_css_incremental_exclude(node, TRUE);
            }
        }
        if (shared) {
            ns_style_free(s);
            s = ns_style_clone_shared(shared);
            display_contents_to_none(node, s);
            g_array_set_size(matches, 0);
            g_array_set_size(var_matches, 0);
            g_array_set_size(pending_matches, 0);
            g_ptr_array_set_size(owned_values, 0);
        } else {
            s->share_id = ++g_style_share_next_id;
            s->vars = build_vars_for_element(parent_style, var_matches);
            resolve_pending_into_matches(pending_matches, s->vars,
                                         matches, owned_values, node);

            cascade_for(matches, s, parent_style, layout_parent,
                    node->parent &&
                        node->parent->kind == NS_NODE_DOCUMENT, *root_px);
            compute_registered_vars(s, parent_style, *root_px);
            strip_native_widget_decorations(node, s);
            if (display_contents_to_none(node, s)) have_key = FALSE;
            g_array_set_size(matches, 0);
            g_array_set_size(var_matches, 0);
            g_array_set_size(pending_matches, 0);
            g_ptr_array_set_size(owned_values, 0);

            for (int gi = 0; gi < n_pe; gi++) {
                ns_css_pseudo_element pe = pe_g[gi].pe;
                GArray *pm = pe_g[gi].m;
                GArray *pe_vars = pe_g[gi].v;
                GArray *pe_pending = pe_g[gi].p;
                if (pm->len == 0 && pe_pending->len == 0) continue;
                GPtrArray *pe_owned =
                    g_ptr_array_new_with_free_func(
                        (GDestroyNotify)ns_css_value_free);
                ns_style *ps = ns_style_alloc();
                ps->vars = build_vars_for_element(s, pe_vars);
                resolve_pending_into_matches(pe_pending, ps->vars, pm, pe_owned, node);
                cascade_for(pm, ps, s,
                            pe == NS_CSS_PE_BEFORE || pe == NS_CSS_PE_AFTER
                                ? s : NULL,
                            FALSE, *root_px);
                compute_registered_vars(ps, s, *root_px);
                gboolean keep = TRUE;
                if (pe == NS_CSS_PE_BEFORE || pe == NS_CSS_PE_AFTER)
                    keep = ps->values[NS_CSS_CONTENT] != NULL;
                if (keep && (pe == NS_CSS_PE_BEFORE || pe == NS_CSS_PE_AFTER) &&
                    ns_display_is_none(ns_css_display_of(ps))) {
                    if (pe == NS_CSS_PE_BEFORE) s->hidden_before = ps;
                    else                        s->hidden_after  = ps;
                } else if (keep) {
                    if (pe == NS_CSS_PE_BEFORE)            s->before       = ps;
                    else if (pe == NS_CSS_PE_AFTER)        s->after        = ps;
                    else if (pe == NS_CSS_PE_FIRST_LETTER) s->first_letter = ps;
                    else if (pe == NS_CSS_PE_FIRST_LINE)   s->first_line   = ps;
                    else if (pe == NS_CSS_PE_SELECTION)    s->selection    = ps;
                    else if (pe == NS_CSS_PE_MARKER)       s->marker       = ps;
                    else if (pe == NS_CSS_PE_BACKDROP)     s->backdrop     = ps;
                    else if (pe == NS_CSS_PE_PLACEHOLDER)  s->placeholder  = ps;
                    else s->file_selector_button = ps;
                } else {
                    ns_style_free(ps);
                }
                g_ptr_array_free(pe_owned, TRUE);
            }
            if (have_key) {
                share_key_t *k = g_new(share_key_t, 1);
                k->len  = probe.len;
                k->hash = probe.hash;
                k->data = g_memdup2(probe.data, probe.len);
                g_hash_table_insert(g_style_share, k, s);
            }
        }
        }
        g_hash_table_insert(out, node, s);
        child_parent_style = s;
        child_layout_parent = ns_display_is_contents(ns_css_display_of(s))
            ? layout_parent : s;
        if (*root_px <= 0 &&
            s->values[NS_CSS_FONT_SIZE] &&
            s->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
            s->values[NS_CSS_FONT_SIZE]->u.length.unit == NS_CSS_UNIT_PX)
            *root_px = s->values[NS_CSS_FONT_SIZE]->u.length.v;
        nd_recurse_dirty = nd_node_dirty;
    }
    gboolean pushed = ns_css_container_stack_push(node);
    gboolean filter_element = g_ancestor_filter_active &&
                              node->kind == NS_NODE_ELEMENT && node->first_child;
    guint8 *outer_filter = NULL;
    if (g_ancestor_filter_active && node->kind == NS_NODE_DOCUMENT &&
        node->parent) {
        outer_filter = g_memdup2(g_ancestor_filter, sizeof g_ancestor_filter);
        memset(g_ancestor_filter, 0, sizeof g_ancestor_filter);
    }
    if (filter_element) css_ancestor_filter_update(node, 1);
    for (ns_node *c = node->first_child; c; c = c->next_sibling)
        cascade_walk(c, ua, author, n_author, child_parent_style,
                     child_layout_parent, root_px,
                     layer_ranks, out, nd_recurse_dirty);
    if (filter_element) css_ancestor_filter_update(node, -1);
    if (outer_filter) {
        memcpy(g_ancestor_filter, outer_filter, sizeof g_ancestor_filter);
        g_free(outer_filter);
    }
    if (pushed) ns_css_container_stack_pop();
    if (frame_viewport) {
        g_viewport_w = frame_vw;
        g_viewport_h = frame_vh;
    }
    depth--;
}

static void
append_text_children(const ns_node *n, GString *out, int depth)
{
    if (depth >= 512) return;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_TEXT && c->text)
            g_string_append(out, c->text);
        else if (c->kind == NS_NODE_ELEMENT)
            append_text_children(c, out, depth + 1);
    }
}

static int g_host_scope_counter;

static char *
shadow_root_host_scope_id(ns_node *root)
{
    if (!root || !root->parent) return NULL;
    ns_node *host = root->parent;
    const char *existing = ns_element_get_attr(host, NS_HOST_SCOPE_ATTR);
    if (existing) return g_strdup(existing);
    char buf[32];
    g_snprintf(buf, sizeof buf, "%d", ++g_host_scope_counter);
    ns_element_set_attr(host, NS_HOST_SCOPE_ATTR, buf);
    return g_strdup(buf);
}

static char *
style_host_scope_id(ns_node *style_el)
{
    for (ns_node *a = style_el; a; a = a->parent)
        if (a->kind == NS_NODE_ELEMENT &&
            ns_element_get_attr(a, NS_SHADOW_ATTR) != NULL)
            return shadow_root_host_scope_id(a);
    return NULL;
}

static char *
style_iframe_scope_id(ns_node *style_el)
{
    ns_node *root = NULL;
    for (ns_node *a = style_el; a; a = a->parent) {
        if (a->kind == NS_NODE_ELEMENT && a->parent &&
            a->parent->kind == NS_NODE_DOCUMENT && a->parent->parent) {
            root = a;
            break;
        }
    }
    if (!root) return NULL;
    const char *existing = ns_element_get_attr(root, NS_HOST_SCOPE_ATTR);
    if (existing) return g_strdup(existing);
    char buf[32];
    g_snprintf(buf, sizeof buf, "%d", ++g_host_scope_counter);
    ns_element_set_attr(root, NS_HOST_SCOPE_ATTR, buf);
    return g_strdup(buf);
}

static char *
style_element_final_css(ns_node *style)
{
    if (!ns_node_is_element_named(style, "style")) return NULL;
    const char *media = ns_element_get_attr(style, "media");
    if (media && *media && !ns_css_media_query_matches(media)) return NULL;
    GString *buf = g_string_new(NULL);
    append_text_children(style, buf, 0);
    if (buf->len == 0) {
        g_string_free(buf, TRUE);
        return NULL;
    }
    gboolean frame_scope = FALSE;
    char *host_id = style_host_scope_id(style);
    if (!host_id) {
        host_id = style_iframe_scope_id(style);
        frame_scope = host_id != NULL;
    }
    if (host_id) {
        char *rewritten = ns_css_scoped_css(buf->str, buf->len, host_id,
                                            frame_scope);
        g_free(host_id);
        g_string_free(buf, TRUE);
        return rewritten;
    }
    return g_string_free(buf, FALSE);
}

char *
ns_css_style_element_text(ns_node *style)
{
    return style_element_final_css(style);
}

char *
ns_css_shadow_adopted_css(ns_node *root)
{
    const char *css = ns_element_get_attr(root, NS_ADOPTED_CSS_ATTR);
    if (!css || !*css) return NULL;
    char *host_id = shadow_root_host_scope_id(root);
    if (!host_id) return NULL;
    char *scoped = ns_css_scoped_css(css, strlen(css), host_id, FALSE);
    g_free(host_id);
    return scoped;
}

typedef struct {
    char *css;
    ns_css_stylesheet *sheet;
    double vw;
    double vh;
} ns_style_el_cached;

typedef struct {
    ns_css_stylesheet *sheet;
    guint64 stamp;
} ns_merged_style_cached;

typedef struct {
    double      vw;
    double      vh;
    const char *base;
    const char *css;
    gsize       len;
    guint       hash;
} ns_merged_style_key;

typedef struct {
    GBytes *bytes;
    ns_css_stylesheet *sheet;
} ns_import_sheet_cached;

static GHashTable *g_style_el_cache;
static GHashTable *g_merged_style_cache;
static GHashTable *g_link_sheet_cache;
static GHashTable *g_import_sheet_cache;
static guint64 g_merged_style_cache_clock;

static void
ns_style_el_cached_free(gpointer data)
{
    ns_style_el_cached *e = data;
    if (!e) return;
    g_free(e->css);
    if (e->sheet) {
        e->sheet->cached = FALSE;
        ns_css_stylesheet_free(e->sheet);
    }
    g_free(e);
}

static ns_merged_style_key
ns_merged_style_key_make(const char *css, gsize len, const char *base)
{
    ns_merged_style_key k = {
        .vw = rint(ns_css_media_viewport_current_w()),
        .vh = rint(ns_css_media_viewport_current_h()),
        .base = base,
        .css = css,
        .len = len,
    };
    guint h = 2166136261u;
    for (gsize i = 0; i < len; i++) {
        h ^= (guchar)css[i];
        h *= 16777619u;
    }
    h ^= base ? g_str_hash(base) * 31u : 0;
    h ^= (guint)k.vw * 7919u ^ (guint)k.vh * 104729u;
    k.hash = h;
    return k;
}

static guint
ns_merged_style_key_hash(gconstpointer p)
{
    return ((const ns_merged_style_key *)p)->hash;
}

static gboolean
ns_merged_style_key_equal(gconstpointer pa, gconstpointer pb)
{
    const ns_merged_style_key *a = pa, *b = pb;
    return a->hash == b->hash && a->len == b->len &&
           a->vw == b->vw && a->vh == b->vh &&
           g_strcmp0(a->base, b->base) == 0 &&
           memcmp(a->css, b->css, a->len) == 0;
}

static void
ns_merged_style_key_free(gpointer data)
{
    ns_merged_style_key *k = data;
    g_free((char *)k->base);
    g_free((char *)k->css);
    g_free(k);
}

static guint64 g_merged_style_pass_start;

static void
ns_merged_style_cache_trim(guint64 keep_after)
{
    if (!g_merged_style_cache ||
        g_hash_table_size(g_merged_style_cache) <= 64)
        return;
    while (g_hash_table_size(g_merged_style_cache) > 48) {
        GHashTableIter it;
        gpointer key, value, victim = NULL;
        guint64 oldest = G_MAXUINT64;
        g_hash_table_iter_init(&it, g_merged_style_cache);
        while (g_hash_table_iter_next(&it, &key, &value)) {
            ns_merged_style_cached *entry = value;
            if (entry->stamp > keep_after) continue;
            if (entry->stamp < oldest) {
                oldest = entry->stamp;
                victim = key;
            }
        }
        if (!victim) break;
        g_hash_table_remove(g_merged_style_cache, victim);
    }
}

static int g_css_relayout_depth;

void
ns_css_relayout_enter(void)
{
    g_css_relayout_depth++;
}

void
ns_css_relayout_leave(void)
{
    if (g_css_relayout_depth > 0) g_css_relayout_depth--;
}

void
ns_css_stylesheet_cache_drop(void)
{
    if (g_style_el_cache) g_hash_table_remove_all(g_style_el_cache);
    if (g_merged_style_cache) g_hash_table_remove_all(g_merged_style_cache);
    if (g_link_sheet_cache) g_hash_table_remove_all(g_link_sheet_cache);
    if (g_import_sheet_cache) g_hash_table_remove_all(g_import_sheet_cache);
}

void
ns_css_style_element_cache_begin(void)
{
    if (g_css_relayout_depth > 1) return;
    if (g_style_el_cache && g_hash_table_size(g_style_el_cache) > 2048)
        g_hash_table_remove_all(g_style_el_cache);
    ns_merged_style_cache_trim(G_MAXUINT64);
    g_merged_style_pass_start = g_merged_style_cache_clock;
    if (g_link_sheet_cache && g_hash_table_size(g_link_sheet_cache) > 256)
        g_hash_table_remove_all(g_link_sheet_cache);
    if (g_import_sheet_cache && g_hash_table_size(g_import_sheet_cache) > 256)
        g_hash_table_remove_all(g_import_sheet_cache);
}

static void
ns_merged_style_cached_free(gpointer data)
{
    ns_merged_style_cached *entry = data;
    if (!entry) return;
    if (entry->sheet) {
        entry->sheet->cached = FALSE;
        ns_css_stylesheet_free(entry->sheet);
    }
    g_free(entry);
}

static void
ns_cached_stylesheet_free(gpointer data)
{
    ns_css_stylesheet *sheet = data;
    if (!sheet) return;
    sheet->cached = FALSE;
    ns_css_stylesheet_free(sheet);
}

static void
ns_import_sheet_cached_free(gpointer data)
{
    ns_import_sheet_cached *e = data;
    if (!e) return;
    g_bytes_unref(e->bytes);
    ns_cached_stylesheet_free(e->sheet);
    g_free(e);
}

void
ns_css_style_element_cache_end(void)
{
    if (g_css_relayout_depth > 1) return;
    ns_merged_style_cache_trim(g_merged_style_pass_start);
}

ns_css_stylesheet *
ns_css_merged_styles_cached(const char *css, gssize len, const char *base_url)
{
    if (!css || len == 0) return NULL;
    if (len < 0) len = (gssize)strlen(css);
    if (!g_merged_style_cache)
        g_merged_style_cache =
            g_hash_table_new_full(ns_merged_style_key_hash,
                                  ns_merged_style_key_equal,
                                  ns_merged_style_key_free,
                                  ns_merged_style_cached_free);
    ns_merged_style_key probe =
        ns_merged_style_key_make(css, (gsize)len, base_url);
    ns_merged_style_cached *hit =
        g_hash_table_lookup(g_merged_style_cache, &probe);
    if (hit) {
        hit->stamp = ++g_merged_style_cache_clock;
        return hit->sheet;
    }
    ns_css_stylesheet *sh = ns_css_stylesheet_parse(css, len);
    if (!sh) return NULL;
    sh->cached = TRUE;
    ns_merged_style_cached *entry = g_new0(ns_merged_style_cached, 1);
    entry->sheet = sh;
    entry->stamp = ++g_merged_style_cache_clock;
    ns_merged_style_key *key = g_new(ns_merged_style_key, 1);
    *key = probe;
    key->base = g_strdup(base_url);
    char *copy = g_malloc((gsize)len + 1);
    memcpy(copy, css, (gsize)len);
    copy[len] = '\0';
    key->css = copy;
    g_hash_table_replace(g_merged_style_cache, key, entry);
    return sh;
}

ns_css_stylesheet *
ns_css_stylesheet_parse_url_cached(const char *url, const char *css, gssize len)
{
    if (!css) return NULL;
    if (!url || !*url) return ns_css_stylesheet_parse(css, len);
    if (!g_link_sheet_cache)
        g_link_sheet_cache =
            g_hash_table_new_full(g_str_hash, g_str_equal,
                                  g_free, ns_cached_stylesheet_free);
    char *key = g_strdup_printf("%.0fx%.0f|%s",
                                ns_css_media_viewport_current_w(),
                                ns_css_media_viewport_current_h(), url);
    ns_css_stylesheet *hit = g_hash_table_lookup(g_link_sheet_cache, key);
    if (hit) {
        g_free(key);
        return hit;
    }
    ns_css_stylesheet *sh = ns_css_stylesheet_parse(css, len);
    if (!sh) {
        g_free(key);
        return NULL;
    }
    sh->cached = TRUE;
    g_hash_table_replace(g_link_sheet_cache, key, sh);
    return sh;
}

static ns_css_stylesheet *
ns_css_stylesheet_parse_in_layer(GBytes *bytes, const char *layer_name)
{
    gsize len = 0;
    const char *data = g_bytes_get_data(bytes, &len);
    ns_css_stylesheet *sh = ns_css_stylesheet_parse(data, (gssize)len);
    if (sh && layer_name)
        ns_css_stylesheet_force_layer(sh, layer_name);
    return sh;
}

ns_css_stylesheet *
ns_css_stylesheet_parse_import_cached(const char *url, const char *layer_name,
                                      GBytes *bytes)
{
    if (!bytes) return NULL;
    if (!url || !*url) return ns_css_stylesheet_parse_in_layer(bytes, layer_name);
    if (!g_import_sheet_cache)
        g_import_sheet_cache =
            g_hash_table_new_full(g_str_hash, g_str_equal,
                                  g_free, ns_import_sheet_cached_free);
    char *key = g_strdup_printf("%.0fx%.0f|%c%zu:%s|%s",
                                ns_css_media_viewport_current_w(),
                                ns_css_media_viewport_current_h(),
                                layer_name ? 'L' : '-',
                                layer_name ? strlen(layer_name) : (gsize)0,
                                layer_name ? layer_name : "", url);
    ns_import_sheet_cached *hit = g_hash_table_lookup(g_import_sheet_cache, key);
    if (hit && (hit->bytes == bytes || g_bytes_equal(hit->bytes, bytes))) {
        g_free(key);
        return hit->sheet;
    }
    ns_css_stylesheet *sh = ns_css_stylesheet_parse_in_layer(bytes, layer_name);
    if (!sh) {
        g_free(key);
        return NULL;
    }
    sh->cached = TRUE;
    ns_import_sheet_cached *entry = g_new0(ns_import_sheet_cached, 1);
    entry->bytes = g_bytes_ref(bytes);
    entry->sheet = sh;
    g_hash_table_replace(g_import_sheet_cache, key, entry);
    return sh;
}

ns_css_stylesheet *
ns_css_stylesheet_from_style_element_cached(ns_node *style)
{
    char *css = style_element_final_css(style);
    if (!css) return NULL;
    if (!g_style_el_cache)
        g_style_el_cache = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                                 NULL, ns_style_el_cached_free);
    ns_style_el_cached *e = g_hash_table_lookup(g_style_el_cache, style);
    if (e && strcmp(e->css, css) == 0 &&
        e->vw == ns_css_media_viewport_current_w() &&
        e->vh == ns_css_media_viewport_current_h()) {
        g_free(css);
        return e->sheet;
    }
    ns_css_stylesheet *sh = ns_css_stylesheet_parse(css, -1);
    if (!sh) {
        g_free(css);
        return NULL;
    }
    sh->cached = TRUE;
    ns_style_el_cached *ne = g_new0(ns_style_el_cached, 1);
    ne->css = css;
    ne->sheet = sh;
    ne->vw = ns_css_media_viewport_current_w();
    ne->vh = ns_css_media_viewport_current_h();
    g_hash_table_replace(g_style_el_cache, style, ne);
    return sh;
}

GHashTable *
ns_css_compute(ns_node *doc,
               const ns_css_stylesheet *const *author_sheets,
               const ns_node *const *sheet_docs,
               gsize n_sheets)
{
    GHashTable *out = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                            NULL, (GDestroyNotify)ns_style_free);

    ns_css_language_cache_reset();

    const ns_css_stylesheet *cached_ua = ua_sheet_for(doc);

    gboolean profile = g_getenv("NS_PROFILE") != NULL;
    gint64 t0 = profile ? g_get_monotonic_time() : 0;
    (void)ns_css_rule_index_ensure(cached_ua);
    for (gsize i = 0; i < n_sheets; i++)
        (void)ns_css_rule_index_ensure(author_sheets[i]);
    gint64 t_idx = profile ? g_get_monotonic_time() : 0;

    GHashTable *layer_ranks = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                    g_free, NULL);
    css_layer_rank_add_sheet(layer_ranks, cached_ua);
    for (gsize i = 0; i < n_sheets; i++)
        css_layer_rank_add_sheet(layer_ranks, author_sheets[i]);
    css_layer_ranks_finalize(layer_ranks);

    g_registered_props = g_hash_table_new(g_str_hash, g_str_equal);
    css_collect_property_rules(g_registered_props, cached_ua);
    for (gsize i = 0; i < n_sheets; i++)
        css_collect_property_rules(g_registered_props, author_sheets[i]);
    if (g_js_registered_props) {
        GHashTableIter it;
        gpointer k, v;
        g_hash_table_iter_init(&it, g_js_registered_props);
        while (g_hash_table_iter_next(&it, &k, &v))
            g_hash_table_replace(g_registered_props, k, v);
    }

    double root_px = 0;
    if (g_decl_sheet_cache && g_hash_table_size(g_decl_sheet_cache) >= 8192)
        g_hash_table_remove_all(g_decl_sheet_cache);
    ns_css_container_stack_reset();
    if (!g_share_scratch)
        g_share_scratch = g_byte_array_sized_new(512);
    g_style_share = g_hash_table_new_full(share_key_hash, share_key_equal,
                                          share_key_free, NULL);
    g_var_adjust_cache = g_hash_table_new_full(
        g_direct_hash, g_direct_equal,
        (GDestroyNotify)ns_var_map_unref, (GDestroyNotify)ns_var_map_unref);
    ns_css_has_memo_begin();
    ns_css_selector_batch_begin();

    guint64 sig = incr_sheet_sig(cached_ua, author_sheets, n_sheets);
    if (sig != g_incr_has_sig) {
        if (g_has_anchors) g_ptr_array_set_size(g_has_anchors, 0);
        else g_has_anchors =
            g_ptr_array_new_with_free_func(incr_has_anchor_free);
        g_has_cq_loose = FALSE;
        incr_collect_has_cq_keys(cached_ua);
        for (gsize i = 0; i < n_sheets; i++)
            incr_collect_has_cq_keys(author_sheets[i]);
        g_incr_eligible = !g_has_cq_loose;
        g_incr_has_sig = sig;
    }
    gboolean incr_usable = g_getenv("NS_NO_INCR_RESTYLE") == NULL
        && g_incr_eligible
        && fabs(g_incr_zoom - 1.0) <= 0.001;
    gboolean incr_want = incr_usable;
    guint64 cq_sig = ns_css_container_map_signature();
    g_incr_pass_active = incr_want
        && g_incr_prev_styles != NULL
        && g_incr_prev_doc == doc
        && g_incr_prev_sig == sig
        && g_incr_prev_cq_sig == cq_sig
        && ns_css_focus_node() == g_incr_prev_focus
        && ns_css_hover_node() == g_incr_prev_hover
        && ns_css_active_node() == g_incr_prev_active
        && ns_css_fullscreen_node() == g_incr_prev_fullscreen;
    g_incr_reused = 0;
    g_incr_recomputed = 0;

    incr_ensure_struct_keys(cached_ua, author_sheets, n_sheets, sig);

    memset(g_ancestor_filter, 0, sizeof g_ancestor_filter);
    g_ancestor_filter_active = TRUE;
    g_ancestor_filter_attrs = ns_css_selector_attr_ancestor_hashes();
    GHashTable *outer_doc_sheets = g_doc_sheets;
    g_doc_sheets = doc_sheets_new(author_sheets, sheet_docs, n_sheets);
    cascade_walk(doc, cached_ua, author_sheets, n_sheets, NULL, NULL,
                 &root_px, layer_ranks, out, FALSE);
    g_clear_pointer(&g_doc_sheets, g_hash_table_destroy);
    g_doc_sheets = outer_doc_sheets;
    g_ancestor_filter_active = FALSE;
    g_ancestor_filter_subject = NULL;

    if (incr_want) {
        GHashTable *new_prev = g_hash_table_new_full(
            g_direct_hash, g_direct_equal, NULL, (GDestroyNotify)ns_style_free);
        GHashTableIter pit; gpointer pk, pv;
        g_hash_table_iter_init(&pit, out);
        while (g_hash_table_iter_next(&pit, &pk, &pv)) {
            ((ns_style *)pv)->ref++;
            g_hash_table_insert(new_prev, pk, pv);
        }
        if (g_incr_before_styles) g_hash_table_destroy(g_incr_before_styles);
        g_incr_before_styles = g_incr_prev_styles;
        g_incr_prev_styles = new_prev;
        g_incr_prev_doc = doc;
        g_incr_prev_sig = sig;
        g_incr_prev_cq_sig = cq_sig;
        g_incr_prev_focus = ns_css_focus_node();
        g_incr_prev_hover = ns_css_hover_node();
        g_incr_prev_active = ns_css_active_node();
        g_incr_prev_fullscreen = ns_css_fullscreen_node();
        if (g_getenv("NS_PROFILE"))
            g_printerr("[incr] active=%d reused=%u recomputed=%u\n",
                       g_incr_pass_active, g_incr_reused, g_incr_recomputed);
    } else if (g_incr_prev_styles && !incr_usable) {
        g_hash_table_destroy(g_incr_prev_styles);
        g_incr_prev_styles = NULL;
        g_incr_prev_doc = NULL;
        if (g_incr_before_styles) g_hash_table_destroy(g_incr_before_styles);
        g_incr_before_styles = NULL;
    }
    if (g_incr_dirty) g_hash_table_remove_all(g_incr_dirty);

    ns_css_has_memo_end();
    ns_css_selector_batch_end();
    g_hash_table_destroy(g_style_share);
    g_style_share = NULL;
    g_hash_table_destroy(g_var_adjust_cache);
    g_var_adjust_cache = NULL;
    g_hash_table_destroy(layer_ranks);
    g_hash_table_destroy(g_registered_props);
    g_registered_props = NULL;
    gint64 t_cascade = profile ? g_get_monotonic_time() : 0;
    if (profile)
        g_printerr("[profile]   css.idx=%.1fms css.cascade=%.1fms\n",
                   (t_idx - t0) / 1000.0,
                   (t_cascade - t_idx) / 1000.0);
    return out;
}

void
ns_css_style_scale_font_size(ns_style *s, double factor)
{
    if (!s || !s->values[NS_CSS_FONT_SIZE] ||
        s->values[NS_CSS_FONT_SIZE]->kind != NS_CSS_V_LENGTH)
        return;
    ns_css_value_cow(s, NS_CSS_FONT_SIZE)->u.length.v *= factor;
}
