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

static GHashTable *g_defined_elements;

void
ns_css_register_defined_element(const char *tag)
{
    if (!tag || !*tag) return;
    if (!g_defined_elements)
        g_defined_elements = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                   g_free, NULL);
    char *lower = g_ascii_strdown(tag, -1);
    if (g_hash_table_contains(g_defined_elements, lower)) g_free(lower);
    else g_hash_table_add(g_defined_elements, lower);
}

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
ns_css_clear_defined_elements(void)
{
    if (g_defined_elements) {
        g_hash_table_destroy(g_defined_elements);
        g_defined_elements = NULL;
    }
}

static gboolean
ns_css_is_defined_element(const char *tag)
{
    if (!g_defined_elements || !tag) return FALSE;
    char *lower = g_ascii_strdown(tag, -1);
    gboolean ok = g_hash_table_contains(g_defined_elements, lower);
    g_free(lower);
    return ok;
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

static char *g_target_fragment = NULL;

void
ns_css_set_target_fragment(const char *fragment)
{
    g_free(g_target_fragment);
    g_target_fragment = (fragment && *fragment) ? g_strdup(fragment) : NULL;
}

static const ns_node *g_css_focus_node = NULL;

const ns_node *
ns_css_set_focus_node(const ns_node *node)
{
    const ns_node *prev = g_css_focus_node;
    g_css_focus_node = node;
    return prev;
}

static const ns_node *g_css_focus_visible_node = NULL;

void
ns_css_set_focus_visible_node(const ns_node *node)
{
    g_css_focus_visible_node = node;
}

static const ns_node *g_css_hover_node = NULL;

const ns_node *
ns_css_set_hover_node(const ns_node *node)
{
    const ns_node *prev = g_css_hover_node;
    g_css_hover_node = node;
    return prev;
}

static const ns_node *g_css_active_node = NULL;

const ns_node *
ns_css_set_active_node(const ns_node *node)
{
    const ns_node *prev = g_css_active_node;
    g_css_active_node = node;
    return prev;
}

static const ns_node *g_css_fullscreen_node = NULL;

const ns_node *
ns_css_set_fullscreen_node(const ns_node *node)
{
    const ns_node *prev = g_css_fullscreen_node;
    g_css_fullscreen_node = node;
    return prev;
}

void
ns_css_forget_node(const ns_node *node)
{
    if (g_css_focus_node == node) g_css_focus_node = NULL;
    if (g_css_focus_visible_node == node) g_css_focus_visible_node = NULL;
    if (g_css_hover_node == node) g_css_hover_node = NULL;
    if (g_css_active_node == node) g_css_active_node = NULL;
    if (g_css_fullscreen_node == node) g_css_fullscreen_node = NULL;
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
static const char *css_scan_segment(const char *p, const char *end,
                                    char *terminator);
static const char *css_scan_declaration_value(const char *p, const char *end,
                                              char *terminator);
static gboolean css_declaration_value_syntax_valid(const char *text);
static const char *css_skip_to_block_end(const char *p, const char *end);
static const char *css_block_body_end(const char *body_start,
                                      const char *block_end);
static const char *css_find_top_level_char(const char *p, const char *end,
                                           char needle);
static const char *css_find_function(const char *p, const char *end,
                                     const char *name);
static const char *css_skip_comment(const char *p, const char *end);
static void css_strip_important(char *text, gboolean *important);
static char *css_trim_dup_range(const char *start, const char *end);
static int split_ws_limit(const char *s, char *out[], int max);
static const char *match_close_paren(const char *p, const char *end);

static char *
ascii_lower(const char *s, gsize len)
{
    if (len == G_MAXSIZE) return g_strdup("");
    char *r = g_malloc(len + 1);
    for (gsize i = 0; i < len; i++) {
        char c = s[i];
        if (c >= 'A' && c <= 'Z') c = (char)(c - 'A' + 'a');
        r[i] = c;
    }
    r[len] = '\0';
    return r;
}

static gboolean
css_wide_keyword_is(const char *kw)
{
    return strcmp(kw, "inherit") == 0 ||
           strcmp(kw, "initial") == 0 ||
           strcmp(kw, "unset") == 0 ||
           strcmp(kw, "revert") == 0 ||
           strcmp(kw, "revert-layer") == 0 ||
           strcmp(kw, "revert-rule") == 0;
}

static ns_css_value *
parse_css_wide_keyword(const char *text)
{
    while (*text && is_ws(*text)) text++;
    gsize len = strlen(text);
    while (len > 0 && is_ws(text[len - 1])) len--;
    char *kw = ascii_lower(text, len);
    if (!css_wide_keyword_is(kw)) {
        g_free(kw);
        return NULL;
    }
    ns_css_value *v = g_new0(ns_css_value, 1);
    v->kind = NS_CSS_V_KEYWORD;
    v->u.keyword = kw;
    return v;
}

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

static void
ns_attr_pred_clear(gpointer p)
{
    ns_css_attr_pred *a = p;
    g_free(a->name);
    g_free(a->value);
}

static void
matches_any_group_free(gpointer data)
{
    g_ptr_array_free((GPtrArray *)data, TRUE);
}

static void
ns_pseudo_pred_clear(gpointer p)
{
    ns_css_pseudo_pred *pc = p;
    g_free(pc->arg);
    if (pc->of_group) g_ptr_array_free(pc->of_group, TRUE);
}

static ns_css_simple *
ns_css_simple_new(void)
{
    ns_css_simple *s = g_new0(ns_css_simple, 1);
    s->classes = g_ptr_array_new_with_free_func(g_free);
    s->class_lens = g_array_new(FALSE, FALSE, sizeof(gsize));
    s->attrs   = g_array_new(FALSE, FALSE, sizeof(ns_css_attr_pred));
    g_array_set_clear_func(s->attrs, ns_attr_pred_clear);
    s->pseudos = g_array_new(FALSE, FALSE, sizeof(ns_css_pseudo_pred));
    g_array_set_clear_func(s->pseudos, ns_pseudo_pred_clear);
    return s;
}

static void
ns_css_simple_free(ns_css_simple *s)
{
    if (!s) return;
    g_free(s->type);
    g_free(s->id);
    g_ptr_array_free(s->classes, TRUE);
    g_array_free(s->class_lens, TRUE);
    if (s->attrs)   g_array_free(s->attrs,   TRUE);
    if (s->pseudos) g_array_free(s->pseudos, TRUE);
    if (s->matches_any)  g_ptr_array_free(s->matches_any,  TRUE);
    if (s->matches_none) g_ptr_array_free(s->matches_none, TRUE);
    if (s->has_groups)   g_ptr_array_free(s->has_groups,   TRUE);
    g_free(s);
}

static void
ns_css_selector_free(ns_css_selector *sel)
{
    if (!sel) return;
    for (guint i = 0; i < sel->compounds->len; i++)
        ns_css_simple_free(g_ptr_array_index(sel->compounds, i));
    g_ptr_array_free(sel->compounds, TRUE);
    g_array_free(sel->combinators, TRUE);
    g_free(sel);
}

typedef struct ns_css_scope {
    GPtrArray *roots;
    GPtrArray *limits;
} ns_css_scope;

typedef struct ns_css_scope_text {
    char *start;
    char *end;
} ns_css_scope_text;

#define NS_CSS_MAX_SELECTOR_NESTING 48
#define NS_CSS_MAX_AT_NESTING 32

static gboolean g_sel_parse_error;
static gboolean g_sel_ns_prefix;
static gboolean g_sel_has_hover;
static gboolean g_sel_has_active;
static gboolean g_sel_strict;
static int g_sel_has_depth;

static ns_css_selector *parse_one_selector_rel(const char **pp, const char *end,
                                               int depth, gboolean relative);
static ns_css_selector *parse_one_selector(const char **pp, const char *end,
                                           int depth);

static GPtrArray *
parse_selector_group_rel(const char *arg, gsize arg_n, int depth,
                         gboolean relative)
{
    GPtrArray *group = g_ptr_array_new_with_free_func(
        (GDestroyNotify)ns_css_selector_free);
    if (depth > NS_CSS_MAX_SELECTOR_NESTING)
        return group;
    const char *p = arg;
    const char *end = arg + arg_n;
    while (p < end) {
        const char *loop_start = p;
        p = css_skip_ws_comments(p, end);
        if (p >= end) break;
        ns_css_selector *sub = parse_one_selector_rel(&p, end, depth, relative);
        if (sub) g_ptr_array_add(group, sub);
        else if (g_sel_strict) g_sel_parse_error = TRUE;
        p = css_skip_ws_comments(p, end);
        if (p < end && *p == ',') { p++; continue; }
        if (p == loop_start) p++;
    }
    return group;
}

static GPtrArray *
parse_selector_group(const char *arg, gsize arg_n, int depth)
{
    return parse_selector_group_rel(arg, arg_n, depth, FALSE);
}

static const char *
css_find_nth_of(const char *s, const char *end)
{
    char quote = 0;
    int paren = 0, bracket = 0;
    const char *p = s;
    while (p < end) {
        char c = *p;
        if (quote) {
            if (c == '\\' && p + 1 < end) p += 2;
            else {
                if (c == quote) quote = 0;
                p++;
            }
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
        if (c == '[') bracket++;
        else if (c == ']' && bracket > 0) bracket--;
        else if (c == '(') paren++;
        else if (c == ')' && paren > 0) paren--;
        if (paren == 0 && bracket == 0 &&
            p + 2 <= end &&
            g_ascii_strncasecmp(p, "of", 2) == 0 &&
            (p == s || is_ws(p[-1])) &&
            (p + 2 == end || is_ws(p[2])))
            return p;
        p++;
    }
    return NULL;
}

static gboolean
anb_int_strict(const char *str, int *out)
{
    const char *p = str;
    if (*p == '+' || *p == '-') p++;
    if (!g_ascii_isdigit(*p)) return FALSE;
    for (const char *q = p; *q; q++)
        if (!g_ascii_isdigit(*q)) return FALSE;
    *out = ns_parse_int(str, 0, -1000000, 1000000);
    return TRUE;
}

static gboolean
parse_anb(const char *arg, gsize alen, int *out_a, int *out_b)
{
    char *raw = g_strndup(arg, alen);
    char *trimmed = g_strstrip(raw);
    char *s = g_malloc(strlen(trimmed) + 1);
    char *w = s;
    for (const char *r = trimmed; *r; r++)
        if (!is_ws(*r)) *w++ = *r;
    *w = '\0';
    int a = 0, b = 0;
    gboolean ok = TRUE;
    if (g_ascii_strcasecmp(s, "odd") == 0) {
        a = 2;
        b = 1;
    } else if (g_ascii_strcasecmp(s, "even") == 0) {
        a = 2;
        b = 0;
    } else {
        char *n_pos = strchr(s, 'n');
        if (!n_pos) n_pos = strchr(s, 'N');
        if (n_pos) {
            *n_pos = '\0';
            const char *a_str = s;
            if (!*a_str || strcmp(a_str, "+") == 0) a = 1;
            else if (strcmp(a_str, "-") == 0) a = -1;
            else ok = anb_int_strict(a_str, &a);
            const char *b_str = n_pos + 1;
            if (*b_str) {
                if (*b_str != '+' && *b_str != '-') ok = FALSE;
                else ok = ok && anb_int_strict(b_str, &b);
            }
        } else {
            a = 0;
            ok = anb_int_strict(s, &b);
        }
    }
    g_free(s);
    g_free(raw);
    if (!ok) return FALSE;
    *out_a = a;
    *out_b = b;
    return TRUE;
}

static gboolean
css_pseudo_class_is_standard(const char *name, gsize n)
{
    static const char *known[] = {
        "default", "indeterminate", "in-range", "out-of-range",
        "fullscreen", "modal", "autofill", "blank",
        "user-valid", "user-invalid", "target-within", "focus-visible",
        "local-link", "current", "past", "future",
        "playing", "paused", "muted", "seeking", "buffering", "stalled",
        "picture-in-picture", "volume-locked",
        "host", "host-context", "nth-col", "nth-last-col", "state",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(known); i++)
        if (strlen(known[i]) == n && g_ascii_strncasecmp(name, known[i], n) == 0)
            return TRUE;
    return FALSE;
}

static gboolean
css_pseudo_element_is_standard(const char *name, gsize n)
{
    static const char *known[] = {
        "part", "slotted", "cue", "cue-region", "highlight",
        "target-text", "spelling-error", "grammar-error",
        "file-selector-button", "details-content",
        "view-transition", "view-transition-group",
        "view-transition-image-pair", "view-transition-old",
        "view-transition-new",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(known); i++)
        if (strlen(known[i]) == n && g_ascii_strncasecmp(name, known[i], n) == 0)
            return TRUE;
    return FALSE;
}

static gboolean
parse_pseudo_keyword(const char *name, gsize n,
                     const char *arg, gsize alen,
                     ns_css_pseudo_pred *out, int depth)
{
    struct { const char *k; ns_css_pseudo v; } table[] = {
        { "first-child",   NS_CSS_PC_FIRST_CHILD },
        { "last-child",    NS_CSS_PC_LAST_CHILD },
        { "only-child",    NS_CSS_PC_ONLY_CHILD },
        { "first-of-type", NS_CSS_PC_FIRST_OF_TYPE },
        { "last-of-type",  NS_CSS_PC_LAST_OF_TYPE },
        { "only-of-type",  NS_CSS_PC_ONLY_OF_TYPE },
        { "empty",         NS_CSS_PC_EMPTY },
        { "root",          NS_CSS_PC_ROOT },
        { "checked",       NS_CSS_PC_CHECKED },
        { "disabled",      NS_CSS_PC_DISABLED },
        { "enabled",       NS_CSS_PC_ENABLED },
        { "required",      NS_CSS_PC_REQUIRED },
        { "optional",      NS_CSS_PC_OPTIONAL },
        { "valid",         NS_CSS_PC_VALID },
        { "invalid",       NS_CSS_PC_INVALID },
        { "in-range",      NS_CSS_PC_IN_RANGE },
        { "out-of-range",  NS_CSS_PC_OUT_OF_RANGE },
        { "default",       NS_CSS_PC_DEFAULT },
        { "indeterminate", NS_CSS_PC_INDETERMINATE },
        { "link",          NS_CSS_PC_LINK },
        { "visited",       NS_CSS_PC_VISITED },
        { "any-link",      NS_CSS_PC_ANY_LINK },
        { "hover",         NS_CSS_PC_HOVER },
        { "active",        NS_CSS_PC_ACTIVE },
        { "focus",         NS_CSS_PC_FOCUS },
        { "focus-visible", NS_CSS_PC_FOCUS_VISIBLE },
        { "focus-within",  NS_CSS_PC_FOCUS_WITHIN },
        { "target",        NS_CSS_PC_TARGET },
        { "target-within", NS_CSS_PC_TARGET_WITHIN },
        { "defined",       NS_CSS_PC_DEFINED },
        { "scope",         NS_CSS_PC_SCOPE },
        { "placeholder-shown", NS_CSS_PC_PLACEHOLDER_SHOWN },
        { "read-only",     NS_CSS_PC_READ_ONLY },
        { "read-write",    NS_CSS_PC_READ_WRITE },
        { "blank",         NS_CSS_PC_BLANK },
        { "open",          NS_CSS_PC_OPEN },
        { "popover-open",  NS_CSS_PC_POPOVER_OPEN },
        { "modal",         NS_CSS_PC_MODAL },
        { "fullscreen",    NS_CSS_PC_FULLSCREEN },
        { "user-valid",    NS_CSS_PC_USER_VALID },
        { "user-invalid",  NS_CSS_PC_USER_INVALID },
        { "autofill",      NS_CSS_PC_AUTOFILL },
        { "-webkit-autofill", NS_CSS_PC_AUTOFILL },
        { "playing",       NS_CSS_PC_PLAYING },
        { "paused",        NS_CSS_PC_PAUSED },
        { "muted",         NS_CSS_PC_MUTED },
        { "seeking",       NS_CSS_PC_SEEKING },
        { "buffering",     NS_CSS_PC_BUFFERING },
        { "stalled",       NS_CSS_PC_STALLED },
    };
    for (gsize i = 0; i < G_N_ELEMENTS(table); i++) {
        gsize klen = strlen(table[i].k);
        if (klen == n && g_ascii_strncasecmp(name, table[i].k, n) == 0) {
            out->kind = table[i].v;
            out->a = 0;
            out->b = 0;
            return TRUE;
        }
    }
    if (n == 7 && g_ascii_strncasecmp(name, "heading", 7) == 0) {
        out->kind = NS_CSS_PC_HEADING;
        out->a = 0;
        out->b = 0;
        if (!arg) {
            out->arg = NULL;
            return TRUE;
        }
        char *raw = g_strndup(arg, alen);
        char **items = g_strsplit(raw, ",", -1);
        gboolean ok = items[0] != NULL;
        for (int i = 0; ok && items[i]; i++) {
            int v = 0;
            if (!anb_int_strict(g_strstrip(items[i]), &v)) ok = FALSE;
        }
        g_strfreev(items);
        if (!ok) {
            g_free(raw);
            return FALSE;
        }
        out->arg = raw;
        return TRUE;
    }
    if (arg && ((n == 9 && g_ascii_strncasecmp(name, "nth-child", 9) == 0) ||
                (n == 14 && g_ascii_strncasecmp(name, "nth-last-child", 14) == 0) ||
                (n == 11 && g_ascii_strncasecmp(name, "nth-of-type", 11) == 0) ||
                (n == 16 && g_ascii_strncasecmp(name, "nth-last-of-type", 16) == 0))) {
        const char *as = arg;
        const char *ae = arg + alen;
        const char *of = (n == 9 || n == 14) ? css_find_nth_of(as, ae) : NULL;
        const char *anb_end = of ? of : ae;
        int a = 0, b = 0;
        if (!parse_anb(as, (gsize)(anb_end - as), &a, &b)) return FALSE;
        if (of) {
            const char *fs = css_skip_ws_comments(of + 2, ae);
            GPtrArray *group = parse_selector_group(fs, (gsize)(ae - fs),
                                                    depth + 1);
            if (!group || group->len == 0) {
                if (group) g_ptr_array_free(group, TRUE);
                return FALSE;
            }
            out->of_group = group;
        }
        if (n == 9) out->kind = NS_CSS_PC_NTH_CHILD;
        else if (n == 14) out->kind = NS_CSS_PC_NTH_LAST_CHILD;
        else if (n == 11) out->kind = NS_CSS_PC_NTH_OF_TYPE;
        else out->kind = NS_CSS_PC_NTH_LAST_OF_TYPE;
        out->a = a;
        out->b = b;
        return TRUE;
    }
    if (arg && n == 4 && g_ascii_strncasecmp(name, "lang", 4) == 0) {
        char *lang = css_trim_dup_range(arg, arg + alen);
        if (!lang || !*lang) {
            g_free(lang);
            return FALSE;
        }
        out->kind = NS_CSS_PC_LANG;
        out->arg = lang;
        return TRUE;
    }
    if (arg && n == 3 && g_ascii_strncasecmp(name, "dir", 3) == 0) {
        char *dir = css_trim_dup_range(arg, arg + alen);
        char *lo = g_ascii_strdown(dir ? dir : "", -1);
        g_free(dir);
        if (strcmp(lo, "ltr") != 0 && strcmp(lo, "rtl") != 0) {
            g_free(lo);
            return FALSE;
        }
        out->kind = NS_CSS_PC_DIR;
        out->arg = lo;
        return TRUE;
    }
    return FALSE;
}

static void
selector_group_max_specificity(const GPtrArray *group, int *a, int *b, int *c)
{
    for (guint i = 0; group && i < group->len; i++) {
        const ns_css_selector *sub = g_ptr_array_index(group, i);
        if (sub->spec_a > *a ||
            (sub->spec_a == *a && sub->spec_b > *b) ||
            (sub->spec_a == *a && sub->spec_b == *b && sub->spec_c > *c)) {
            *a = sub->spec_a;
            *b = sub->spec_b;
            *c = sub->spec_c;
        }
    }
}

static ns_css_selector *
parse_one_selector(const char **pp, const char *end, int depth)
{
    return parse_one_selector_rel(pp, end, depth, FALSE);
}

static guint32
css_identifier_hash(char kind, const char *name, gsize len)
{
    guint32 h = 2166136261u;
    h = (h ^ (guchar)kind) * 16777619u;
    for (gsize i = 0; i < len; i++)
        h = (h ^ (guchar)g_ascii_tolower(name[i])) * 16777619u;
    return h;
}

static void
css_selector_add_ancestor_hash(ns_css_selector *sel, guint32 hash)
{
    if (sel->n_ancestor_hashes < G_N_ELEMENTS(sel->ancestor_hashes))
        sel->ancestor_hashes[sel->n_ancestor_hashes++] = hash;
}

static gboolean g_css_attr_ancestor_hashes;

static guint32
css_attr_value_hash(const char *name, const char *value, gsize value_len)
{
    guint32 h = css_identifier_hash('[', name, strlen(name));
    h = (h ^ (guchar)'=') * 16777619u;
    for (gsize i = 0; i < value_len; i++)
        h = (h ^ (guchar)value[i]) * 16777619u;
    return h;
}

static gboolean
css_attr_pred_filterable(const ns_css_attr_pred *a)
{
    return a->op == NS_CSS_ATTR_EQ && a->name && a->value &&
           !a->case_insensitive && !a->html_ci && !strchr(a->name, '|');
}

static void
css_selector_collect_attr_ancestor_hashes(ns_css_selector *sel)
{
    for (int k = (int)sel->compounds->len - 2; k >= 0; k--) {
        ns_css_comb right = g_array_index(sel->combinators, ns_css_comb, k + 1);
        if (right != NS_CSS_COMB_DESCENDANT && right != NS_CSS_COMB_CHILD)
            continue;
        const ns_css_simple *c = g_ptr_array_index(sel->compounds, k);
        for (guint i = 0; c->attrs && i < c->attrs->len; i++) {
            const ns_css_attr_pred *a =
                &g_array_index(c->attrs, ns_css_attr_pred, i);
            if (!css_attr_pred_filterable(a)) continue;
            css_selector_add_ancestor_hash(
                sel, css_attr_value_hash(a->name, a->value, strlen(a->value)));
            sel->n_ancestor_attr_hashes = sel->n_ancestor_hashes;
            g_css_attr_ancestor_hashes = TRUE;
        }
    }
}

static void
css_selector_collect_ancestor_hashes(ns_css_selector *sel)
{
    css_selector_collect_attr_ancestor_hashes(sel);
    for (int k = (int)sel->compounds->len - 2; k >= 0; k--) {
        ns_css_comb right = g_array_index(sel->combinators, ns_css_comb, k + 1);
        if (right != NS_CSS_COMB_DESCENDANT && right != NS_CSS_COMB_CHILD)
            continue;
        const ns_css_simple *c = g_ptr_array_index(sel->compounds, k);
        if (c->id)
            css_selector_add_ancestor_hash(
                sel, css_identifier_hash('#', c->id, strlen(c->id)));
        for (guint i = 0; i < c->classes->len; i++) {
            const char *cls = g_ptr_array_index(c->classes, i);
            css_selector_add_ancestor_hash(
                sel, css_identifier_hash('.', cls, strlen(cls)));
        }
        if (c->type && strcmp(c->type, "*") != 0)
            css_selector_add_ancestor_hash(
                sel, css_identifier_hash('%', c->type, strlen(c->type)));
    }
}

static gboolean ns_css_html_ci_attr(const char *name);

static ns_css_selector *
parse_one_selector_rel(const char **pp, const char *end, int depth,
                       gboolean relative)
{
    ns_css_selector *sel = g_new0(ns_css_selector, 1);
    sel->compounds   = g_ptr_array_new();
    sel->combinators = g_array_new(FALSE, FALSE, sizeof(ns_css_comb));

    ns_css_comb pending = NS_CSS_COMB_NONE;
    gboolean expect_compound = TRUE;
    gboolean leading_comb_used = FALSE;
    const char *p = *pp;

    while (p < end) {

        gboolean had_ws = FALSE;
        const char *before_ws = p;
        p = css_skip_ws_comments(p, end);
        had_ws = p > before_ws;
        if (p >= end) break;
        char c = *p;

        if (c == ',' || c == '{') break;

        if (c == '>' || c == '+' || c == '~') {
            if (relative && sel->compounds->len == 0 && !leading_comb_used)
                leading_comb_used = TRUE;
            else if (expect_compound || sel->compounds->len == 0)
                g_sel_parse_error = TRUE;
            pending = c == '>' ? NS_CSS_COMB_CHILD
                    : c == '+' ? NS_CSS_COMB_ADJACENT
                    : NS_CSS_COMB_SIBLING;
            expect_compound = TRUE;
            p++;
            continue;
        }

        if (had_ws && !expect_compound)
            pending = NS_CSS_COMB_DESCENDANT;

        ns_css_simple *cmp = ns_css_simple_new();
        gboolean any = FALSE;
        while (p < end) {
            const char *tok_start = p;
            char cc = *p;
            if (cc == '*' || (cc == '|' && !(p + 1 < end && p[1] == '='))) {
                if (any) {
                    g_sel_parse_error = TRUE;
                    cmp->never_match = TRUE;
                }
                if (cc == '*') {
                    p++;
                }
                if (p < end && *p == '|' && !(p + 1 < end && p[1] == '=')) {
                    if (cc == '|')
                        cmp->ns_none = TRUE;
                    p++;
                    if (p < end && *p == '*') {
                        p++;
                        g_free(cmp->type);
                        cmp->type = g_strdup("*");
                    }
                    else {
                        char *type = ns_css_read_ident(&p, end);
                        if (type && *type) {
                            if (!cmp->type) {
                                cmp->type = ascii_lower(type, strlen(type));
                                sel->spec_c += 1;
                            }
                        }
                        else {
                            g_sel_parse_error = TRUE;
                        }
                        g_free(type);
                    }
                }
                else {
                    if (cmp->type) {
                        g_sel_parse_error = TRUE;
                        cmp->never_match = TRUE;
                    }
                    g_free(cmp->type);
                    cmp->type = g_strdup("*");
                }
                any = TRUE;
            } else if (cc == '#') {
                p++;
                char *id_str = ns_css_read_ident(&p, end);
                if (id_str && *id_str) {
                    g_free(cmp->id);
                    cmp->id = id_str;
                    sel->spec_a += 1;
                } else {
                    g_sel_parse_error = TRUE;
                    cmp->never_match = TRUE;
                    g_free(id_str);
                }
                any = TRUE;
            } else if (cc == '.') {
                p++;
                gboolean bad_start = FALSE;
                if (p < end) {
                    unsigned char nc = (unsigned char)*p;
                    if (g_ascii_isdigit(nc))
                        bad_start = TRUE;
                    else if (nc == '-' && p + 1 < end &&
                             g_ascii_isdigit((unsigned char)p[1]))
                        bad_start = TRUE;
                }
                char *cls = ns_css_read_ident(&p, end);
                if (!bad_start && cls && *cls) {
                    gsize cls_len = strlen(cls);
                    g_ptr_array_add(cmp->classes, cls);
                    g_array_append_val(cmp->class_lens, cls_len);
                    sel->spec_b += 1;
                } else {
                    g_sel_parse_error = TRUE;
                    cmp->never_match = TRUE;
                    g_free(cls);
                }
                any = TRUE;
            } else if (is_ident_start(cc) || cc == '\\') {
                if (any) {
                    g_sel_parse_error = TRUE;
                    cmp->never_match = TRUE;
                }
                char *type = ns_css_read_ident(&p, end);
                if (p < end && *p == '|' && !(p + 1 < end && p[1] == '=')) {
                    g_sel_ns_prefix = TRUE;
                    cmp->never_match = TRUE;
                    p++;
                    if (p < end && *p == '*') {
                        p++;
                    }
                    else {
                        char *unused = ns_css_read_ident(&p, end);
                        g_free(unused);
                    }
                }
                else if (!cmp->type) {
                    cmp->type = ascii_lower(type, strlen(type));
                    sel->spec_c += 1;
                }
                else {
                    g_sel_parse_error = TRUE;
                    cmp->never_match = TRUE;
                }
                g_free(type);
                any = TRUE;
            } else if (cc == ':') {
                p++;
                gboolean is_element = (p < end && *p == ':');
                if (is_element) p++;
                char *pseudo_name = ns_css_read_ident(&p, end);
                const char *name_s = pseudo_name;
                gsize name_n = strlen(pseudo_name);
                if (name_n == 0) {
                    g_sel_parse_error = TRUE;
                    cmp->never_match = TRUE;
                    g_free(pseudo_name);
                    any = TRUE;
                    continue;
                }
                const char *arg_s = NULL;
                gsize arg_n = 0;
                if (p < end && *p == '(') {
                    p++;
                    arg_s = p;
                    char term = 0;
                    const char *arg_end = css_scan_until(p, end, ")", &term);
                    arg_n = (gsize)(arg_end - arg_s);
                    p = term == ')' ? arg_end + 1 : arg_end;
                }
                if (is_element ||
                    (name_n == 6 && g_ascii_strncasecmp(name_s, "before", 6) == 0) ||
                    (name_n == 5 && g_ascii_strncasecmp(name_s, "after",  5) == 0) ||
                    (name_n == 10 && g_ascii_strncasecmp(name_s, "first-line", 10) == 0) ||
                    (name_n == 12 && g_ascii_strncasecmp(name_s, "first-letter", 12) == 0)) {
                    if (name_n == 6 && g_ascii_strncasecmp(name_s, "before", 6) == 0) {
                        sel->pseudo_element = NS_CSS_PE_BEFORE;
                        sel->spec_c += 1;
                    } else if (name_n == 5 && g_ascii_strncasecmp(name_s, "after", 5) == 0) {
                        sel->pseudo_element = NS_CSS_PE_AFTER;
                        sel->spec_c += 1;
                    } else if (name_n == 12 && g_ascii_strncasecmp(name_s, "first-letter", 12) == 0) {
                        sel->pseudo_element = NS_CSS_PE_FIRST_LETTER;
                        sel->spec_c += 1;
                    } else if (name_n == 10 && g_ascii_strncasecmp(name_s, "first-line", 10) == 0) {
                        sel->pseudo_element = NS_CSS_PE_FIRST_LINE;
                        sel->spec_c += 1;
                    } else if (name_n == 9 && g_ascii_strncasecmp(name_s, "selection", 9) == 0) {
                        sel->pseudo_element = NS_CSS_PE_SELECTION;
                        sel->spec_c += 1;
                    } else if (name_n == 6 && g_ascii_strncasecmp(name_s, "marker", 6) == 0) {
                        sel->pseudo_element = NS_CSS_PE_MARKER;
                        sel->spec_c += 1;
                    } else if (name_n == 8 && g_ascii_strncasecmp(name_s, "backdrop", 8) == 0) {
                        sel->pseudo_element = NS_CSS_PE_BACKDROP;
                        sel->spec_c += 1;
                    } else if (name_n == 20 &&
                               g_ascii_strncasecmp(name_s,
                                                   "file-selector-button",
                                                   20) == 0) {
                        sel->pseudo_element = NS_CSS_PE_FILE_SELECTOR_BUTTON;
                        sel->spec_c += 1;
                    } else if ((name_n == 11 &&
                                g_ascii_strncasecmp(name_s, "placeholder", 11) == 0) ||
                               (name_n == 25 &&
                                g_ascii_strncasecmp(name_s, "-webkit-input-placeholder", 25) == 0) ||
                               (name_n == 21 &&
                                g_ascii_strncasecmp(name_s, "-ms-input-placeholder", 21) == 0) ||
                               (name_n == 16 &&
                                g_ascii_strncasecmp(name_s, "-moz-placeholder", 16) == 0)) {
                        sel->pseudo_element = NS_CSS_PE_PLACEHOLDER;
                        sel->spec_c += 1;
                    } else {
                        cmp->never_match = TRUE;
                        if (name_s[0] != '-'
                            && !css_pseudo_element_is_standard(name_s, name_n))
                            g_sel_parse_error = TRUE;
                    }
                } else if (name_n == 3 && arg_s &&
                           g_ascii_strncasecmp(name_s, "has", 3) == 0 &&
                           g_sel_has_depth > 0) {
                    cmp->never_match = TRUE;
                    g_sel_parse_error = TRUE;
                } else if (name_n == 3 && arg_s &&
                           g_ascii_strncasecmp(name_s, "has", 3) == 0) {
                    g_sel_has_depth++;
                    GPtrArray *group = parse_selector_group_rel(arg_s, arg_n,
                                                                depth + 1, TRUE);
                    g_sel_has_depth--;
                    if (group->len == 0) {
                        g_ptr_array_free(group, TRUE);
                        cmp->never_match = TRUE;
                    } else {
                        if (!cmp->has_groups)
                            cmp->has_groups = g_ptr_array_new_with_free_func(
                                matches_any_group_free);
                        g_ptr_array_add(cmp->has_groups, group);
                        int ma = 0, mb = 0, mc = 0;
                        for (guint gi = 0; gi < group->len; gi++) {
                            const ns_css_selector *sub =
                                g_ptr_array_index(group, gi);
                            if (sub->spec_a > ma ||
                                (sub->spec_a == ma && sub->spec_b > mb) ||
                                (sub->spec_a == ma && sub->spec_b == mb &&
                                 sub->spec_c > mc)) {
                                ma = sub->spec_a;
                                mb = sub->spec_b;
                                mc = sub->spec_c;
                            }
                        }
                        sel->spec_a += ma;
                        sel->spec_b += mb;
                        sel->spec_c += mc;
                    }
                } else if (name_n > 0 && arg_s &&
                           ((name_n == 2 && g_ascii_strncasecmp(name_s, "is",    2) == 0) ||
                            (name_n == 5 && g_ascii_strncasecmp(name_s, "where", 5) == 0))) {
                    gboolean is_where = (name_n == 5);
                    gboolean saved_err = g_sel_parse_error;
                    gboolean saved_ns = g_sel_ns_prefix;
                    GPtrArray *group = parse_selector_group(arg_s, arg_n, depth + 1);
                    if (!g_sel_strict) g_sel_parse_error = saved_err;
                    g_sel_ns_prefix = saved_ns;
                    if (group->len == 0) {
                        g_ptr_array_free(group, TRUE);
                        cmp->never_match = TRUE;
                    } else {
                        if (!cmp->matches_any)
                            cmp->matches_any = g_ptr_array_new_with_free_func(
                                matches_any_group_free);
                        g_ptr_array_add(cmp->matches_any, group);
                        if (!is_where) {
                            int ma = 0, mb = 0, mc = 0;
                            for (guint gi = 0; gi < group->len; gi++) {
                                const ns_css_selector *sub =
                                    g_ptr_array_index(group, gi);
                                if (sub->spec_a > ma ||
                                    (sub->spec_a == ma && sub->spec_b > mb) ||
                                    (sub->spec_a == ma && sub->spec_b == mb &&
                                     sub->spec_c > mc)) {
                                    ma = sub->spec_a;
                                    mb = sub->spec_b;
                                    mc = sub->spec_c;
                                }
                            }
                            sel->spec_a += ma;
                            sel->spec_b += mb;
                            sel->spec_c += mc;
                        }
                    }
                } else if (name_n == 3 && arg_s &&
                           g_ascii_strncasecmp(name_s, "not", 3) == 0) {
                    GPtrArray *group = parse_selector_group(arg_s, arg_n, depth + 1);
                    if (group->len == 0) {
                        g_ptr_array_free(group, TRUE);
                    } else {
                        if (!cmp->matches_none)
                            cmp->matches_none = g_ptr_array_new_with_free_func(
                                matches_any_group_free);
                        g_ptr_array_add(cmp->matches_none, group);
                        int ma = 0, mb = 0, mc = 0;
                        for (guint gi = 0; gi < group->len; gi++) {
                            const ns_css_selector *sub =
                                g_ptr_array_index(group, gi);
                            if (sub->spec_a > ma ||
                                (sub->spec_a == ma && sub->spec_b > mb) ||
                                (sub->spec_a == ma && sub->spec_b == mb &&
                                 sub->spec_c > mc)) {
                                ma = sub->spec_a;
                                mb = sub->spec_b;
                                mc = sub->spec_c;
                            }
                        }
                        sel->spec_a += ma;
                        sel->spec_b += mb;
                        sel->spec_c += mc;
                    }
                } else if (name_n > 0) {
                    ns_css_pseudo_pred pc = {0};
                    if (parse_pseudo_keyword(name_s, name_n, arg_s, arg_n, &pc,
                                             depth)) {
                        g_array_append_val(cmp->pseudos, pc);
                        if (pc.kind == NS_CSS_PC_HOVER)
                            g_sel_has_hover = TRUE;
                        if (pc.kind == NS_CSS_PC_ACTIVE)
                            g_sel_has_active = TRUE;
                        sel->spec_b += 1;
                        int ma = 0, mb = 0, mc = 0;
                        selector_group_max_specificity(pc.of_group, &ma, &mb, &mc);
                        sel->spec_a += ma;
                        sel->spec_b += mb;
                        sel->spec_c += mc;
                    } else {
                        cmp->never_match = TRUE;
                        if (!css_pseudo_class_is_standard(name_s, name_n))
                            g_sel_parse_error = TRUE;
                    }
                } else {
                    cmp->never_match = TRUE;
                    g_sel_parse_error = TRUE;
                }
                g_free(pseudo_name);
                any = TRUE;
            } else if (cc == '[') {
                p++;
                p = css_skip_ws_comments(p, end);
                if (p + 1 < end && *p == '*' && p[1] == '|') {
                    p += 2;
                }
                else if (p < end && *p == '|' && !(p + 1 < end && p[1] == '=')) {
                    p++;
                }
                char *attr_name = ns_css_read_ident(&p, end);
                if (attr_name && *attr_name && p < end && *p == '|'
                    && !(p + 1 < end && p[1] == '='))
                {
                    g_sel_ns_prefix = TRUE;
                    g_free(attr_name);
                    p++;
                    attr_name = ns_css_read_ident(&p, end);
                    cmp->never_match = TRUE;
                }
                if (!attr_name || !*attr_name) {
                    g_free(attr_name);
                    char term = 0;
                    const char *close = css_scan_until(p, end, "]", &term);
                    p = term == ']' ? close + 1 : close;
                    continue;
                }
                ns_css_attr_pred ap = {0};
                ap.name = ascii_lower(attr_name, strlen(attr_name));
                ap.name_bit = ns_attr_name_bloom_bit(ap.name);
                ap.html_ci = ns_css_html_ci_attr(ap.name);
                g_free(attr_name);
                ap.op   = NS_CSS_ATTR_PRESENT;
                p = css_skip_ws_comments(p, end);
                if (p < end && (*p == '=' || *p == '^' || *p == '$' ||
                                *p == '*' || *p == '~' || *p == '|')) {
                    char op_c = *p;
                    if (op_c == '=')      ap.op = NS_CSS_ATTR_EQ;
                    else if (op_c == '^') { p++; if (p < end && *p == '=') ap.op = NS_CSS_ATTR_PREFIX; }
                    else if (op_c == '$') { p++; if (p < end && *p == '=') ap.op = NS_CSS_ATTR_SUFFIX; }
                    else if (op_c == '*') { p++; if (p < end && *p == '=') ap.op = NS_CSS_ATTR_SUBSTR; }
                    else if (op_c == '~') { p++; if (p < end && *p == '=') ap.op = NS_CSS_ATTR_WORD;   }
                    else if (op_c == '|') { p++; if (p < end && *p == '=') ap.op = NS_CSS_ATTR_HYPHEN; }
                    if (p < end && *p == '=') p++;
                    p = css_skip_ws_comments(p, end);
                    char q = (p < end) ? *p : 0;
                    if (q == '"' || q == '\'') {
                        ap.value = ns_css_read_string(&p, end);
                    } else {
                        ap.value = ns_css_read_ident(&p, end);
                    }
                }
                p = css_skip_ws_comments(p, end);
                if (p < end && *p != ']') {
                    const char *flag_start = p;
                    char *flag = ns_css_read_ident(&p, end);
                    if (flag && g_ascii_strcasecmp(flag, "i") == 0) {
                        if (ap.op == NS_CSS_ATTR_PRESENT) g_sel_parse_error = TRUE;
                        ap.case_insensitive = TRUE;
                    } else if (flag && g_ascii_strcasecmp(flag, "s") == 0) {
                        if (ap.op == NS_CSS_ATTR_PRESENT) g_sel_parse_error = TRUE;
                        ap.case_sensitive = TRUE;
                    } else {
                        p = flag_start;
                        g_sel_parse_error = TRUE;
                    }
                    g_free(flag);
                }
                p = css_skip_ws_comments(p, end);
                if (p < end && *p != ']')
                    g_sel_parse_error = TRUE;
                char term = 0;
                const char *close = css_scan_until(p, end, "]", &term);
                p = term == ']' ? close + 1 : close;
                g_array_append_val(cmp->attrs, ap);
                sel->spec_b += 1;
                any = TRUE;
            } else {
                break;
            }
            if (p == tok_start) break;
        }
        if (!any) { ns_css_simple_free(cmp); break; }
        g_ptr_array_add(sel->compounds, cmp);
        g_array_append_val(sel->combinators, pending);
        pending = NS_CSS_COMB_NONE;
        expect_compound = FALSE;
    }
    *pp = p;
    if (pending != NS_CSS_COMB_NONE)
        g_sel_parse_error = TRUE;
    if (sel->compounds->len == 0) {
        ns_css_selector_free(sel);
        return NULL;
    }
    if (!relative) css_selector_collect_ancestor_hashes(sel);
    return sel;
}

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


static int split_ws(const char *s, char *out[4]);


static ns_css_value *
font_shorthand_size_value(const char *size_only)
{
    double kw = ns_css_font_size_keyword_px(size_only);
    ns_css_value *v = NULL;
    if (kw > 0 || g_ascii_strcasecmp(size_only, "larger") == 0 ||
        g_ascii_strcasecmp(size_only, "smaller") == 0) {
        v = g_new0(ns_css_value, 1);
        v->kind = NS_CSS_V_LENGTH;
        if (kw > 0) {
            v->u.length.v = kw;
            v->u.length.unit = NS_CSS_UNIT_PX;
        } else {
            v->u.length.v = g_ascii_strcasecmp(size_only, "larger") == 0
                ? 1.2 : 0.833333333333;
            v->u.length.unit = NS_CSS_UNIT_EM;
        }
        return v;
    }
    return ns_css_parse_value_for(NS_CSS_FONT_SIZE, size_only);
}

static gboolean text_is_ident(const char *t);
static char *bg_position_zip(const char *xs, const char *ys);
static gboolean bg_token_is_box(const char *tok);
static gboolean inline_css_wide_value(const char *value);


static gboolean attr_functions_syntax_valid(const char *text);


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


static gboolean list_style_split(const char *text, char **out_type,
                                 char **out_position, char **out_image);
static char *css_inline_value_canonical(const char *prop, char *value);
static int prop_id(const char *name);


static gboolean
list_style_split(const char *text, char **out_type, char **out_position,
                 char **out_image)
{
    char *tokens[8] = {0};
    int n = ns_css_split_ws_paren(text, tokens, 8);
    char *type = NULL, *position = NULL, *image = NULL;
    int nones = 0;
    gboolean ok = n >= 1 && n <= 3;
    for (int i = 0; i < n && ok; i++) {
        const char *tok = tokens[i];
        if (!position && (g_ascii_strcasecmp(tok, "inside") == 0 ||
                          g_ascii_strcasecmp(tok, "outside") == 0)) {
            position = g_ascii_strdown(tok, -1);
            continue;
        }
        if (g_ascii_strcasecmp(tok, "none") == 0) { nones++; continue; }
        if (!image && (g_ascii_strncasecmp(tok, "url(", 4) == 0 ||
                       ns_css_text_starts_gradient(tok) || ns_css_text_starts_image_set(tok))) {
            ns_css_value *iv = ns_css_parse_value_for(NS_CSS_LIST_STYLE_IMAGE, tok);
            if (!iv) { ok = FALSE; break; }
            ns_css_value_free(iv);
            image = css_inline_value_canonical("list-style-image", g_strdup(tok));
            continue;
        }
        if (!type) {
            type = ns_css_list_style_type_canonical(tok);
            if (type) continue;
        }
        ok = FALSE;
    }
    if (ok && nones > 2) ok = FALSE;
    if (ok && nones == 2 && (type || image)) ok = FALSE;
    if (ok && nones == 1 && type && image) ok = FALSE;
    for (int i = 0; i < n; i++) g_free(tokens[i]);
    if (!ok) {
        g_free(type);
        g_free(position);
        g_free(image);
        return FALSE;
    }
    if (nones == 2 || (nones == 1 && !type && !image)) {
        type = g_strdup("none");
        image = g_strdup("none");
    } else if (nones == 1 && !type) {
        type = g_strdup("none");
    } else if (nones == 1 && !image) {
        image = g_strdup("none");
    }
    *out_type = type ? type : g_strdup("disc");
    *out_position = position ? position : g_strdup("outside");
    *out_image = image ? image : g_strdup("none");
    return TRUE;
}

ns_display
ns_css_display_of(const ns_style *s)
{
    ns_display d = { 0 };
    return s ? s->display : d;
}


static gboolean
prop_name_is_color(const char *prop)
{
    static const char *const names[] = {
        "color", "background-color", "border-top-color", "border-right-color",
        "border-bottom-color", "border-left-color", "border-color",
        "outline-color", "text-decoration-color", "column-rule-color",
        "caret-color", "accent-color", "fill", "stroke", "stop-color",
        "flood-color", "lighting-color", "text-emphasis-color",
        "border-block-start-color", "border-block-end-color",
        "border-inline-start-color", "border-inline-end-color",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(names); i++)
        if (strcmp(prop, names[i]) == 0) return TRUE;
    return FALSE;
}

static char *
quad_text_collapse(char *const v[4])
{
    if (strcmp(v[0], v[1]) == 0 && strcmp(v[1], v[2]) == 0 &&
        strcmp(v[2], v[3]) == 0)
        return g_strdup(v[0]);
    if (strcmp(v[0], v[2]) == 0 && strcmp(v[1], v[3]) == 0)
        return g_strdup_printf("%s %s", v[0], v[1]);
    if (strcmp(v[1], v[3]) == 0)
        return g_strdup_printf("%s %s %s", v[0], v[1], v[2]);
    return g_strdup_printf("%s %s %s %s", v[0], v[1], v[2], v[3]);
}

static char *
border_radius_half_canonical(const char *text)
{
    char *tokens[5] = {0};
    int n = split_ws_limit(text, tokens, G_N_ELEMENTS(tokens));
    char *vals[5] = { NULL };
    gboolean ok = n >= 1 && n <= 4;
    for (int i = 0; ok && i < n; i++) {
        double num;
        ns_css_unit unit;
        if (ns_css_parse_length(tokens[i], &num, &unit)) {
            ok = num >= 0 && (unit != NS_CSS_UNIT_NUMBER || num == 0);
            vals[i] = ok ? ns_css_add_leading_zeros(g_strdup(tokens[i])) : NULL;
        } else {
            ns_css_value *c = ns_css_parse_calc(tokens[i]);
            ok = c != NULL;
            ns_css_value_free(c);
            if (ok) {
                vals[i] = ns_css_math_canonical(tokens[i]);
                if (!vals[i]) vals[i] = ns_css_add_leading_zeros(g_strdup(tokens[i]));
            }
        }
    }
    char *r = NULL;
    if (ok) {
        char *quad[4] = {
            vals[0],
            n >= 2 ? vals[1] : vals[0],
            n >= 3 ? vals[2] : vals[0],
            n >= 4 ? vals[3] : (n >= 2 ? vals[1] : vals[0]),
        };
        r = quad_text_collapse(quad);
    }
    for (int i = 0; i < n; i++) {
        g_free(tokens[i]);
        g_free(vals[i]);
    }
    return r;
}

static char *
border_radius_canonical(const char *value)
{
    if (!value || strstr(value, "var(")) return NULL;
    const char *slash = strchr(value, '/');
    if (slash && strchr(slash + 1, '/')) return NULL;
    char *first = slash ? g_strndup(value, (gsize)(slash - value)) : g_strdup(value);
    char *h = border_radius_half_canonical(g_strstrip(first));
    char *v = NULL;
    gboolean ok = h != NULL;
    if (ok && slash) {
        char *second = g_strdup(slash + 1);
        v = border_radius_half_canonical(g_strstrip(second));
        g_free(second);
        ok = v != NULL;
    }
    char *r = NULL;
    if (ok) r = v && strcmp(h, v) != 0 ? g_strdup_printf("%s / %s", h, v)
                                       : g_strdup(h);
    g_free(first);
    g_free(h);
    g_free(v);
    return r;
}

char *
ns_css_specified_canonical(const char *prop, const char *value)
{
    if (prop && strcmp(prop, "display") == 0) {
        char *d = ns_css_display_canonical(value);
        if (d) return d;
    }
    if (prop && strcmp(prop, "transform") == 0) {
        char *t = ns_css_transform_list_canonical(value);
        if (t) return t;
        t = ns_css_transform_canonical(value);
        if (t) return t;
    }
    if (prop && (strcmp(prop, "scale") == 0 || strcmp(prop, "rotate") == 0 ||
                 strcmp(prop, "translate") == 0)) {
        char *t = ns_css_individual_transform_canonical(value,
            prop[0] == 's' ? NS_CSS_SCALE : prop[0] == 'r' ? NS_CSS_ROTATE
                                                          : NS_CSS_TRANSLATE);
        if (t) return t;
    }
    if (prop && (strcmp(prop, "transform-origin") == 0 ||
                 strcmp(prop, "perspective-origin") == 0)) {
        char *t = ns_css_transform_origin_canonical(value, prop[0] == 'p');
        if (t) return t;
    }
    if (prop && (strcmp(prop, "border-radius") == 0 ||
                 strcmp(prop, "-webkit-border-radius") == 0)) {
        char *r = border_radius_canonical(value);
        if (r) return r;
    }
    if (prop && (strcmp(prop, "animation") == 0 || strcmp(prop, "transition") == 0)) {
        char *a = ns_css_animation_shorthand_canonical(value, prop[0] == 'a');
        if (a) return a;
    }
    if (prop && (strcmp(prop, "animation-range-start") == 0 ||
                 strcmp(prop, "animation-range-end") == 0 ||
                 strcmp(prop, "animation-timeline") == 0 ||
                 strcmp(prop, "animation-name") == 0 ||
                 strcmp(prop, "transition-property") == 0 ||
                 strcmp(prop, "animation-timing-function") == 0 ||
                 strcmp(prop, "transition-timing-function") == 0 ||
                 strcmp(prop, "counter-reset") == 0 ||
                 strcmp(prop, "counter-increment") == 0 ||
                 strcmp(prop, "counter-set") == 0 ||
                 strcmp(prop, "list-style-type") == 0 ||
                 strcmp(prop, "overflow-clip-margin") == 0)) {
        int pid = prop_id(prop);
        ns_css_value *v = pid >= 0 ? ns_css_parse_value_for((ns_css_prop)pid, value) : NULL;
        if (v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword) {
            char *r = g_strdup(v->u.keyword);
            ns_css_value_free(v);
            return r;
        }
        ns_css_value_free(v);
    }
    if (prop && value && prop_name_is_color(prop)) {
        guint8 r, g, b, a;
        if (text_is_ident(value)) {
            if (ns_css_parse_color(value, &r, &g, &b, &a) ||
                g_ascii_strcasecmp(value, "currentcolor") == 0)
                return g_ascii_strdown(value, -1);
        } else if ((value[0] == '#' ||
                    g_ascii_strncasecmp(value, "rgb", 3) == 0 ||
                    g_ascii_strncasecmp(value, "hsl", 3) == 0 ||
                    g_ascii_strncasecmp(value, "hwb(", 4) == 0) &&
                   !strstr(value, "var(") && !strstr(value, "calc(") &&
                   !strstr(value, "none") &&
                   ns_css_parse_color(value, &r, &g, &b, &a)) {
            GString *out = g_string_new(NULL);
            ns_css_append_color(out, r, g, b, a);
            return g_string_free(out, FALSE);
        }
    }
    if (prop && (strcmp(prop, "background-clip") == 0 ||
                 strcmp(prop, "background-origin") == 0 ||
                 strcmp(prop, "background-attachment") == 0 ||
                 strcmp(prop, "background-repeat") == 0)) {
        int pid = prop_id(prop);
        ns_css_value *v = pid >= 0 ? ns_css_parse_value_for((ns_css_prop)pid, value) : NULL;
        if (v) {
            char *r = ns_css_value_serialize(v);
            ns_css_value_free(v);
            return r;
        }
    }
    if (prop && (strcmp(prop, "box-shadow") == 0 ||
                 strcmp(prop, "text-shadow") == 0)) {
        char *sh = ns_css_shadow_specified_canonical(value, prop[0] == 't');
        if (sh) return sh;
    }
    if (prop && strcmp(prop, "aspect-ratio") == 0) {
        ns_css_value *v = ns_css_parse_value_for(NS_CSS_ASPECT_RATIO, value);
        if (!v) return NULL;
        char *r = ns_css_value_serialize(v);
        ns_css_value_free(v);
        return r;
    }
    if (prop && strcmp(prop, "animation-range") == 0) {
        char *st = NULL, *en = NULL;
        if (ns_css_anim_range_shorthand_expand(value, &st, &en)) {
            char *r = ns_css_animation_range_serialize(st, en);
            g_free(st);
            g_free(en);
            return r;
        }
        return NULL;
    }
    if (prop && (strcmp(prop, "transition-delay") == 0 ||
                 strcmp(prop, "transition-duration") == 0 ||
                 strcmp(prop, "animation-delay") == 0 ||
                 strcmp(prop, "animation-duration") == 0)) {
        char *t = ns_css_time_specified(value);
        if (t) return t;
        return NULL;
    }
    return ns_css_math_canonical(value);
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

static gboolean
text_is_ident(const char *t)
{
    if (!t || !*t || g_ascii_isdigit((guchar)*t)) return FALSE;
    for (const char *p = t; *p; p++)
        if (!g_ascii_isalnum((guchar)*p) && *p != '-' && *p != '_')
            return FALSE;
    return TRUE;
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

gboolean
ns_css_declaration_valid(int prop, const char *text)
{
    if (prop < 0 || !text || !*text) return TRUE;
    if (strstr(text, "attr(") && !attr_functions_syntax_valid(text)) return FALSE;
    if (strstr(text, "var(") || strstr(text, "attr(")) return TRUE;
    ns_css_value *v = ns_css_parse_value_for((ns_css_prop)prop, text);
    if (!v) return FALSE;
    ns_css_value_free(v);
    return TRUE;
}

static void expand_border_image(GArray *decls_out, const char *vtext,
                                gboolean important);

gboolean
ns_css_named_declaration_valid(const char *name, const char *text)
{
    if (!name || !text || !*text) return TRUE;
    if (!ns_css_named_property_supported(name)) return FALSE;
    if (name[0] == '-' && name[1] == '-')
        return css_declaration_value_syntax_valid(text);
    if (g_ascii_strcasecmp(name, "border-image") == 0 ||
        g_ascii_strcasecmp(name, "-webkit-border-image") == 0) {
        if (strstr(text, "var(")) return TRUE;
        GArray *decls = g_array_new(FALSE, FALSE, sizeof(ns_css_decl));
        expand_border_image(decls, text, FALSE);
        gboolean valid = decls->len > 0;
        for (guint i = 0; i < decls->len; i++)
            ns_css_value_free(g_array_index(decls, ns_css_decl, i).value);
        g_array_free(decls, TRUE);
        return valid;
    }
    if (g_ascii_strcasecmp(name, "unicode-range") == 0) {
        char *canon = ns_css_unicode_range_canonical(text);
        gboolean ok = canon != NULL;
        g_free(canon);
        return ok;
    }
    if (g_ascii_strcasecmp(name, "animation-range") == 0 && !strstr(text, "var(")) {
        ns_css_value *wide = parse_css_wide_keyword(text);
        if (wide) {
            ns_css_value_free(wide);
            return TRUE;
        }
        char *st = NULL, *en = NULL;
        if (!ns_css_anim_range_shorthand_expand(text, &st, &en)) return FALSE;
        g_free(st);
        g_free(en);
        return TRUE;
    }
    if (g_ascii_strcasecmp(name, "all") != 0) {
        int prop = prop_id(name);
        if (prop >= 0 && ns_css_declaration_valid(prop, text)) return TRUE;
        return ns_css_supports_declaration(name, text);
    }
    if (strstr(text, "var(")) return TRUE;
    ns_css_value *wide = parse_css_wide_keyword(text);
    if (!wide) return FALSE;
    ns_css_value_free(wide);
    return TRUE;
}

static void
emit_quad(GArray *decls, ns_css_prop t, ns_css_prop r,
          ns_css_prop b, ns_css_prop l,
          char *vals[4], int n, gboolean important)
{
    const char *top    = vals[0];
    const char *right  = n >= 2 ? vals[1] : top;
    const char *bottom = n >= 3 ? vals[2] : top;
    const char *left   = n >= 4 ? vals[3] : right;
    const struct { ns_css_prop p; const char *v; } map[] = {
        { t, top }, { r, right }, { b, bottom }, { l, left },
    };
    for (int i = 0; i < 4; i++) {
        ns_css_value *vv = ns_css_parse_value_for(map[i].p, map[i].v);
        if (!vv) continue;
        ns_css_decl d = { .prop = map[i].p, .value = vv, .important = important };
        g_array_append_val(decls, d);
    }
}

static int
split_ws_limit(const char *s, char *out[], int max)
{
    int n = 0;
    const char *p = s;
    const char *end = s + strlen(s);
    while (p < end && n < max) {
        while (p < end && is_ws(*p)) p++;
        if (p >= end) break;
        const char *start = p;
        char term = 0;
        p = css_scan_until(p, end, " \t\n\r\f", &term);
        out[n++] = g_strndup(start, (gsize)(p - start));
    }
    return n;
}

static int
split_ws(const char *s, char *out[4])
{
    return split_ws_limit(s, out, 4);
}

static void
position_split_specified(const char *canon, char **out_x, char **out_y)
{
    char *tok[4] = {0};
    int n = split_ws_limit(canon, tok, 4);
    if (n == 4) {
        *out_x = g_strdup_printf("%s %s", tok[0], tok[1]);
        *out_y = g_strdup_printf("%s %s", tok[2], tok[3]);
    } else if (n == 3) {
        gboolean off_after_first = !ns_css_position_is_keyword(tok[1]);
        *out_x = off_after_first ? g_strdup_printf("%s %s", tok[0], tok[1])
                                 : g_strdup(tok[0]);
        *out_y = off_after_first ? g_strdup(tok[2])
                                 : g_strdup_printf("%s %s", tok[1], tok[2]);
    } else if (n == 2) {
        *out_x = g_strdup(tok[0]);
        *out_y = g_strdup(tok[1]);
    } else {
        *out_x = g_strdup(n == 1 ? tok[0] : "center");
        *out_y = g_strdup("center");
    }
    for (int i = 0; i < n; i++) g_free(tok[i]);
}

static char *
substitute_var_fallbacks(const char *vtext, int depth)
{
    if (!vtext) return NULL;
    if (depth > 16) return g_strdup(vtext);
    GString *out = g_string_new(NULL);
    const char *p = vtext;
    const char *end = vtext + strlen(vtext);
    while (p < end) {
        const char *fn = css_find_function(p, end, "var");
        if (!fn) {
            g_string_append_len(out, p, (gssize)(end - p));
            break;
        }
        g_string_append_len(out, p, (gssize)(fn - p));
        const char *args_start = fn + 4;
        char term = 0;
        const char *args_end = css_scan_until(args_start, end, ")", &term);
        if (term != ')') {
            p = end;
            break;
        }
        char comma_term = 0;
        const char *comma = css_scan_until(args_start, args_end, ",",
                                           &comma_term);
        if (comma_term == ',') {
            char *nested = css_trim_dup_range(comma + 1, args_end);
            char *sub = substitute_var_fallbacks(nested, depth + 1);
            if (sub) g_string_append(out, sub);
            g_free(nested);
            g_free(sub);
        }
        p = args_end + 1;
    }
    return g_string_free(out, FALSE);
}

static void pending_decl_clear(gpointer data);

typedef enum ns_custom_prop_wide {
    NS_CUSTOM_WIDE_NONE,
    NS_CUSTOM_WIDE_INHERIT,
    NS_CUSTOM_WIDE_INITIAL,
    NS_CUSTOM_WIDE_UNSET,
    NS_CUSTOM_WIDE_REVERT,
    NS_CUSTOM_WIDE_REVERT_LAYER,
    NS_CUSTOM_WIDE_REVERT_RULE,
} ns_custom_prop_wide;

static ns_custom_prop_wide
custom_prop_wide_kind(const char *text)
{
    if (!text) return NS_CUSTOM_WIDE_NONE;
    const char *start = text;
    while (*start && is_ws(*start)) start++;
    const char *end = text + strlen(text);
    while (end > start && is_ws(end[-1])) end--;
    gsize len = (gsize)(end - start);
    if (len == 7 && g_ascii_strncasecmp(start, "inherit", len) == 0)
        return NS_CUSTOM_WIDE_INHERIT;
    if (len == 7 && g_ascii_strncasecmp(start, "initial", len) == 0)
        return NS_CUSTOM_WIDE_INITIAL;
    if (len == 5 && g_ascii_strncasecmp(start, "unset", len) == 0)
        return NS_CUSTOM_WIDE_UNSET;
    if (len == 6 && g_ascii_strncasecmp(start, "revert", len) == 0)
        return NS_CUSTOM_WIDE_REVERT;
    if (len == 12 && g_ascii_strncasecmp(start, "revert-layer", len) == 0)
        return NS_CUSTOM_WIDE_REVERT_LAYER;
    if (len == 11 && g_ascii_strncasecmp(start, "revert-rule", len) == 0)
        return NS_CUSTOM_WIDE_REVERT_RULE;
    return NS_CUSTOM_WIDE_NONE;
}

static gboolean
custom_prop_value_invalid(const char *text)
{
    return !text || custom_prop_wide_kind(text) != NS_CUSTOM_WIDE_NONE;
}

typedef struct ns_var_map {
    int ref;
    GHashTable *own;
    struct ns_var_map *parent;
    GPtrArray *names;
} ns_var_map;

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

const char *
ns_var_map_lookup(const ns_var_map *m, const char *name)
{
    for (; m; m = m->parent) {
        if (m->own) {
            const char *v = g_hash_table_lookup(m->own, name);
            if (v) return v;
        }
    }
    return NULL;
}

static gint
ns_var_name_compare(gconstpointer a, gconstpointer b)
{
    const char *left = *(const char *const *)a;
    const char *right = *(const char *const *)b;
    return strcmp(left, right);
}

GPtrArray *
ns_var_map_names(const ns_var_map *m)
{
    if (m && m->names) return g_ptr_array_ref(m->names);
    GPtrArray *names = g_ptr_array_new_with_free_func(g_free);
    GHashTable *seen = g_hash_table_new(g_str_hash, g_str_equal);
    for (const ns_var_map *current = m; current; current = current->parent) {
        if (!current->own) continue;
        GHashTableIter iter;
        gpointer key, value;
        g_hash_table_iter_init(&iter, current->own);
        while (g_hash_table_iter_next(&iter, &key, &value)) {
            if (g_hash_table_contains(seen, key)) continue;
            g_hash_table_add(seen, key);
            if (value && g_ascii_strcasecmp(value, "initial") != 0)
                g_ptr_array_add(names, g_strdup(key));
        }
    }
    g_hash_table_destroy(seen);
    g_ptr_array_sort(names, ns_var_name_compare);
    if (!m) return names;
    ((ns_var_map *)m)->names = names;
    return g_ptr_array_ref(names);
}

#define NS_CSS_VAR_EXPAND_MAX   ((gsize)1024 * 1024)
#define NS_CSS_VAR_EXPAND_CALLS ((guint)100000)

typedef struct {
    gsize    out_bytes;
    guint    calls;
    gboolean overflow;
} ns_var_budget;

static gboolean
var_budget_take(ns_var_budget *b, gsize n, gboolean *valid)
{
    if (b->overflow) return FALSE;
    if (n > NS_CSS_VAR_EXPAND_MAX - b->out_bytes) {
        b->overflow = TRUE;
        if (valid) *valid = FALSE;
        return FALSE;
    }
    b->out_bytes += n;
    return TRUE;
}

static char *
substitute_vars_with_valid(const char *vtext, const ns_var_map *map, int depth,
                           gboolean *valid, ns_var_budget *b)
{
    if (!vtext) return NULL;
    if (depth > 16) return g_strdup(vtext);
    if (b->overflow || ++b->calls > NS_CSS_VAR_EXPAND_CALLS) {
        b->overflow = TRUE;
        if (valid) *valid = FALSE;
        return g_strdup("");
    }
    GString *out = g_string_new(NULL);
    const char *p = vtext;
    const char *end = vtext + strlen(vtext);
    while (p < end) {
        const char *fn = css_find_function(p, end, "var");
        if (!fn) {
            if (!var_budget_take(b, (gsize)(end - p), valid)) break;
            g_string_append_len(out, p, (gssize)(end - p));
            break;
        }
        if (!var_budget_take(b, (gsize)(fn - p), valid)) break;
        g_string_append_len(out, p, (gssize)(fn - p));
        const char *args_start = fn + 4;
        char term = 0;
        const char *args_end = css_scan_until(args_start, end, ")", &term);
        if (term != ')') {
            p = end;
            break;
        }
        char comma_term = 0;
        const char *comma = css_scan_until(args_start, args_end, ",",
                                           &comma_term);
        const char *name_end = comma_term == ',' ? comma : args_end;
        char *name = css_trim_dup_range(args_start, name_end);
        const char *replacement = NULL;
        if (map && name[0] == '-' && name[1] == '-')
            replacement = ns_var_map_lookup(map, name);
        if (replacement && *replacement &&
            !custom_prop_value_invalid(replacement)) {
            gboolean sub_valid = TRUE;
            char *sub = substitute_vars_with_valid(replacement, map,
                                                   depth + 1, &sub_valid, b);
            if (sub_valid && custom_prop_value_invalid(sub)) {
                ns_css_property_rule *pr = g_registered_props
                    ? g_hash_table_lookup(g_registered_props, name) : NULL;
                if (pr && pr->has_initial) {
                    g_free(sub);
                    sub = substitute_vars_with_valid(pr->initial_value, map,
                                                     depth + 1, &sub_valid, b);
                } else {
                    sub_valid = FALSE;
                }
            }
            if (sub_valid) {
                if (sub) g_string_append(out, sub);
            } else if (comma_term == ',') {
                char *nested = css_trim_dup_range(comma + 1, args_end);
                gboolean nested_valid = TRUE;
                char *fallback = substitute_vars_with_valid(nested, map,
                                                            depth + 1,
                                                            &nested_valid, b);
                if (nested_valid && fallback)
                    g_string_append(out, fallback);
                else if (valid)
                    *valid = FALSE;
                g_free(nested);
                g_free(fallback);
            } else if (valid) {
                *valid = FALSE;
            }
            g_free(sub);
        } else if (comma_term == ',') {
            char *nested = css_trim_dup_range(comma + 1, args_end);
            gboolean nested_valid = TRUE;
            char *sub = substitute_vars_with_valid(nested, map, depth + 1,
                                                   &nested_valid, b);
            if (nested_valid) {
                if (sub) g_string_append(out, sub);
            } else if (valid) {
                *valid = FALSE;
            }
            g_free(nested);
            g_free(sub);
        } else if (valid) {
            *valid = FALSE;
        }
        g_free(name);
        p = args_end + 1;
    }
    return g_string_free(out, FALSE);
}

static char *
substitute_vars_with(const char *vtext, const ns_var_map *map, int depth)
{
    gboolean valid = TRUE;
    ns_var_budget budget = { 0, 0, FALSE };
    char *out = substitute_vars_with_valid(vtext, map, depth, &valid, &budget);
    if (!valid) {
        g_free(out);
        return NULL;
    }
    return out;
}

char *
ns_css_resolve_style_vars(const char *text, const ns_style *style)
{
    return substitute_vars_with(text, style ? style->vars : NULL, 0);
}

static gboolean
is_color_keyword(const char *s)
{
    return s && (g_ascii_strcasecmp(s, "currentcolor") == 0 ||
                 g_ascii_strcasecmp(s, "transparent") == 0);
}

static gboolean
css_value_has_container_unit(const char *text)
{
    static const char *const units[] = {
        "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax",
    };
    if (!text) return FALSE;
    for (const char *p = text; *p; p++) {
        if (p == text || (!g_ascii_isdigit(p[-1]) && p[-1] != '.')) continue;
        gsize remaining = strlen(p);
        for (gsize i = 0; i < G_N_ELEMENTS(units); i++) {
            gsize len = strlen(units[i]);
            if (remaining >= len &&
                g_ascii_strncasecmp(p, units[i], len) == 0 &&
                !is_ident(p[len]))
                return TRUE;
        }
    }
    return FALSE;
}

static void
emit_longhand(GArray *decls_out, ns_css_prop prop, const char *text,
              gboolean important)
{
    ns_css_value *v = ns_css_parse_value_for(prop, text);
    if (!v) return;
    ns_css_decl d = { .prop = prop, .value = v, .important = important };
    g_array_append_val(decls_out, d);
}

static void
emit_border_image_initial(GArray *decls_out, gboolean important)
{
    emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_SOURCE, "none", important);
    emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_SLICE, "100%", important);
    emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_WIDTH, "1", important);
    emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_OUTSET, "0", important);
    emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_REPEAT, "stretch", important);
}

static void
expand_border_image(GArray *decls_out, const char *vtext, gboolean important)
{
    ns_css_value *wide = parse_css_wide_keyword(vtext);
    if (wide) {
        static const ns_css_prop longhands[] = {
            NS_CSS_BORDER_IMAGE_SOURCE, NS_CSS_BORDER_IMAGE_SLICE,
            NS_CSS_BORDER_IMAGE_WIDTH, NS_CSS_BORDER_IMAGE_OUTSET,
            NS_CSS_BORDER_IMAGE_REPEAT,
        };
        for (gsize i = 0; i < G_N_ELEMENTS(longhands); i++) {
            ns_css_decl d = { .prop = longhands[i],
                              .value = ns_css_value_dup(wide),
                              .important = important };
            g_array_append_val(decls_out, d);
        }
        ns_css_value_free(wide);
        return;
    }
    GPtrArray *toks = ns_css_border_image_tokens(vtext);
    GString *slice = g_string_new(NULL);
    GString *width = g_string_new(NULL);
    GString *outset = g_string_new(NULL);
    GString *repeat = g_string_new(NULL);
    char *source = NULL;
    int slash = 0, repeats = 0;
    gboolean slice_closed = FALSE;
    gboolean ok = toks->len > 0;
    for (guint i = 0; ok && i < toks->len; i++) {
        const char *tok = g_ptr_array_index(toks, i);
        if (strcmp(tok, "/") == 0) {
            if (slice_closed || slash >= 2 ||
                (slash == 0 && slice->len == 0)) {
                ok = FALSE;
                break;
            }
            slash++;
            continue;
        }
        if (slash > 0) {
            char *comp = ns_css_border_image_length_serialize(tok, slash == 1,
                                                       slash == 1);
            if (comp) {
                GString *target = slash == 1 ? width : outset;
                if (target->len) g_string_append_c(target, ' ');
                g_string_append(target, comp);
                g_free(comp);
                continue;
            }
            if (slash == 1 ? width->len == 0 : outset->len == 0) {
                ok = FALSE;
                break;
            }
            slice_closed = TRUE;
            slash = 0;
        }
        if (ns_css_border_image_tile_keyword(tok)) {
            if (repeats >= 2) { ok = FALSE; break; }
            if (repeat->len) g_string_append_c(repeat, ' ');
            g_string_append(repeat, tok);
            repeats++;
            continue;
        }
        ns_css_value *keyword = parse_css_wide_keyword(tok);
        if (keyword) {
            ns_css_value_free(keyword);
            ok = FALSE;
            break;
        }
        ns_css_value *img = ns_css_parse_value_for(NS_CSS_BORDER_IMAGE_SOURCE, tok);
        if (img) {
            ns_css_value_free(img);
            if (source) { ok = FALSE; break; }
            source = g_strdup(tok);
            continue;
        }
        if (slice_closed) { ok = FALSE; break; }
        if (slice->len) g_string_append_c(slice, ' ');
        g_string_append(slice, tok);
    }
    if (ok && slice->len) {
        ns_css_value *sv = ns_css_parse_value_for(NS_CSS_BORDER_IMAGE_SLICE,
                                           slice->str);
        if (sv) ns_css_value_free(sv);
        else ok = FALSE;
    }
    if (ok && (width->len || outset->len)) {
        ns_css_value *wv = width->len
            ? ns_css_parse_value_for(NS_CSS_BORDER_IMAGE_WIDTH, width->str) : NULL;
        ns_css_value *ov = outset->len
            ? ns_css_parse_value_for(NS_CSS_BORDER_IMAGE_OUTSET, outset->str) : NULL;
        if (width->len && !wv) ok = FALSE;
        if (outset->len && !ov) ok = FALSE;
        ns_css_value_free(wv);
        ns_css_value_free(ov);
    }
    if (ok) {
        emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_SOURCE,
                      source ? source : "none", important);
        emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_SLICE,
                      slice->len ? slice->str : "100%", important);
        emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_WIDTH,
                      width->len ? width->str : "1", important);
        emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_OUTSET,
                      outset->len ? outset->str : "0", important);
        emit_longhand(decls_out, NS_CSS_BORDER_IMAGE_REPEAT,
                      repeat->len ? repeat->str : "stretch", important);
    }
    g_free(source);
    g_string_free(slice, TRUE);
    g_string_free(width, TRUE);
    g_string_free(outset, TRUE);
    g_string_free(repeat, TRUE);
    g_ptr_array_free(toks, TRUE);
}

static const ns_css_prop kAnimationLonghands[] = {
    NS_CSS_ANIMATION_NAME, NS_CSS_ANIMATION_DURATION, NS_CSS_ANIMATION_DELAY,
    NS_CSS_ANIMATION_TIMING_FUNCTION, NS_CSS_ANIMATION_ITERATION_COUNT,
    NS_CSS_ANIMATION_DIRECTION, NS_CSS_ANIMATION_FILL_MODE,
    NS_CSS_ANIMATION_PLAY_STATE, NS_CSS_ANIMATION_TIMELINE,
    NS_CSS_ANIMATION_RANGE_START, NS_CSS_ANIMATION_RANGE_END,
};
static const ns_css_prop kTransitionLonghands[] = {
    NS_CSS_TRANSITION_PROPERTY, NS_CSS_TRANSITION_DURATION,
    NS_CSS_TRANSITION_DELAY, NS_CSS_TRANSITION_TIMING_FUNCTION,
    NS_CSS_TRANSITION_BEHAVIOR,
};

static void
anim_shorthand_emit_longhands(GArray *decls, ns_css_value *shorthand,
                              gboolean is_animation, gboolean important)
{
    const ns_css_prop *lh = is_animation ? kAnimationLonghands : kTransitionLonghands;
    gsize n = is_animation ? G_N_ELEMENTS(kAnimationLonghands)
                           : G_N_ELEMENTS(kTransitionLonghands);
    if (shorthand->kind != NS_CSS_V_ANIM) {
        for (gsize i = 0; i < n; i++) {
            ns_css_decl d = { .prop = lh[i], .value = ns_css_value_dup(shorthand),
                              .important = important };
            g_array_append_val(decls, d);
        }
        return;
    }
    const ns_css_anim_list *list = &shorthand->u.anim;
    for (gsize i = 0; i < n; i++) {
        GString *text = g_string_new(NULL);
        gboolean single = lh[i] == NS_CSS_ANIMATION_TIMELINE ||
                          lh[i] == NS_CSS_ANIMATION_RANGE_START ||
                          lh[i] == NS_CSS_ANIMATION_RANGE_END;
        for (int k = 0; k < (single ? MIN(list->n, 1) : list->n); k++) {
            char *t = ns_css_anim_entry_longhand_text(&list->entries[k], lh[i]);
            if (k) g_string_append(text, ", ");
            g_string_append(text, t ? t : "");
            g_free(t);
        }
        ns_css_value *v = list->n > 0 ? ns_css_parse_value_for(lh[i], text->str) : NULL;
        if (!v) {
            v = g_new0(ns_css_value, 1);
            v->kind = NS_CSS_V_KEYWORD;
            v->u.keyword = g_strdup("initial");
        }
        ns_css_decl d = { .prop = lh[i], .value = v, .important = important };
        g_array_append_val(decls, d);
        g_string_free(text, TRUE);
    }
}

char *
ns_css_background_position_join(const char *xs, const char *ys)
{
    return bg_position_zip(xs, ys);
}

static char *
bg_position_zip(const char *xs, const char *ys)
{
    char *x = g_strdup(xs), *y = g_strdup(ys);
    gboolean xi = ns_inline_value_strip_important(x);
    gboolean yi = ns_inline_value_strip_important(y);
    GPtrArray *xl = ns_css_split_top_level_commas(x);
    GPtrArray *yl = ns_css_split_top_level_commas(y);
    char *r = NULL;
    if (xi == yi && xl->len == 1 && yl->len == 1 &&
        inline_css_wide_value(g_ptr_array_index(xl, 0)) &&
        strcmp(g_ptr_array_index(xl, 0), g_ptr_array_index(yl, 0)) == 0) {
        r = g_strdup(g_ptr_array_index(xl, 0));
        if (xi) {
            char *with = g_strconcat(r, " !important", NULL);
            g_free(r);
            r = with;
        }
    } else if (xi == yi && xl->len == yl->len) {
        GString *out = g_string_new(NULL);
        for (guint i = 0; i < xl->len; i++) {
            if (i) g_string_append(out, ", ");
            g_string_append(out, g_ptr_array_index(xl, i));
            g_string_append_c(out, ' ');
            g_string_append(out, g_ptr_array_index(yl, i));
        }
        if (xi) g_string_append(out, " !important");
        r = g_string_free(out, FALSE);
    }
    g_ptr_array_free(xl, TRUE);
    g_ptr_array_free(yl, TRUE);
    g_free(x);
    g_free(y);
    return r;
}

char *
ns_css_background_shorthand_serialize(const char *image, const char *position,
                                      const char *size, const char *repeat,
                                      const char *attachment,
                                      const char *origin, const char *clip,
                                      const char *color)
{
    const char *texts[7] = { image, position, size, repeat, attachment,
                             origin, clip };
    GPtrArray *lists[7];
    guint n = 0;
    gboolean ok = TRUE;
    for (int k = 0; k < 7; k++) {
        lists[k] = ns_css_split_top_level_commas(texts[k] ? texts[k] : "");
        if (k == 0) n = lists[k]->len;
        else if (lists[k]->len != n) ok = FALSE;
    }
    GString *out = g_string_new(NULL);
    for (guint i = 0; ok && i < n; i++) {
        const char *img = g_ptr_array_index(lists[0], i);
        const char *pos = g_ptr_array_index(lists[1], i);
        const char *sz = g_ptr_array_index(lists[2], i);
        const char *rep = g_ptr_array_index(lists[3], i);
        const char *att = g_ptr_array_index(lists[4], i);
        const char *org = g_ptr_array_index(lists[5], i);
        const char *clp = g_ptr_array_index(lists[6], i);
        GString *layer = g_string_new(NULL);
        if (g_ascii_strcasecmp(img, "none") != 0) g_string_append(layer, img);
        gboolean size_set = g_ascii_strcasecmp(sz, "auto") != 0 &&
                            g_ascii_strcasecmp(sz, "auto auto") != 0;
        if (size_set || strcmp(pos, "0% 0%") != 0) {
            if (layer->len) g_string_append_c(layer, ' ');
            g_string_append(layer, pos);
            if (size_set) {
                g_string_append(layer, " / ");
                g_string_append(layer, sz);
                if (!strchr(sz, ' ') && g_ascii_strcasecmp(sz, "cover") != 0 &&
                    g_ascii_strcasecmp(sz, "contain") != 0)
                    g_string_append(layer, " auto");
            }
        }
        if (g_ascii_strcasecmp(rep, "repeat") != 0) {
            if (layer->len) g_string_append_c(layer, ' ');
            g_string_append(layer, rep);
        }
        if (g_ascii_strcasecmp(att, "scroll") != 0) {
            if (layer->len) g_string_append_c(layer, ' ');
            g_string_append(layer, att);
        }
        gboolean clip_only = !bg_token_is_box(clp);
        if (clip_only) {
            if (layer->len) g_string_append_c(layer, ' ');
            if (g_ascii_strcasecmp(org, "border-box") != 0) {
                g_string_append(layer, org);
                g_string_append_c(layer, ' ');
            }
            g_string_append(layer, clp);
        } else if (g_ascii_strcasecmp(org, clp) == 0) {
            if (layer->len) g_string_append_c(layer, ' ');
            g_string_append(layer, org);
        } else if (!(g_ascii_strcasecmp(org, "padding-box") == 0 &&
                     g_ascii_strcasecmp(clp, "border-box") == 0)) {
            if (layer->len) g_string_append_c(layer, ' ');
            g_string_append(layer, org);
            g_string_append_c(layer, ' ');
            g_string_append(layer, clp);
        }
        if (i + 1 == n && color && g_ascii_strcasecmp(color, "transparent") != 0) {
            if (layer->len) g_string_append_c(layer, ' ');
            g_string_append(layer, color);
        }
        if (layer->len == 0) g_string_append(layer, "none");
        if (out->len) g_string_append(out, ", ");
        g_string_append(out, layer->str);
        g_string_free(layer, TRUE);
    }
    for (int k = 0; k < 7; k++) g_ptr_array_free(lists[k], TRUE);
    if (!ok || n == 0) {
        g_string_free(out, TRUE);
        return NULL;
    }
    return g_string_free(out, FALSE);
}

typedef struct {
    char *image, *pos_x, *pos_y, *size, *repeat, *attachment, *origin, *clip;
} bg_layer_text;

static void
bg_layer_text_clear(bg_layer_text *l)
{
    g_free(l->image);
    g_free(l->pos_x);
    g_free(l->pos_y);
    g_free(l->size);
    g_free(l->repeat);
    g_free(l->attachment);
    g_free(l->origin);
    g_free(l->clip);
    memset(l, 0, sizeof *l);
}

static gboolean
bg_token_is_length_like(const char *tok, gboolean nonnegative)
{
    double num;
    ns_css_unit unit;
    if (ns_css_parse_length(tok, &num, &unit))
        return (unit != NS_CSS_UNIT_NUMBER || num == 0) &&
               (!nonnegative || num >= 0);
    ns_css_value *c = ns_css_parse_calc(tok);
    gboolean ok = c != NULL;
    ns_css_value_free(c);
    return ok;
}

static gboolean
bg_token_is_position(const char *tok)
{
    return g_ascii_strcasecmp(tok, "left") == 0 ||
           g_ascii_strcasecmp(tok, "center") == 0 ||
           g_ascii_strcasecmp(tok, "right") == 0 ||
           g_ascii_strcasecmp(tok, "top") == 0 ||
           g_ascii_strcasecmp(tok, "bottom") == 0 ||
           bg_token_is_length_like(tok, FALSE);
}

static gboolean
bg_token_is_box(const char *tok)
{
    return g_ascii_strcasecmp(tok, "border-box") == 0 ||
           g_ascii_strcasecmp(tok, "padding-box") == 0 ||
           g_ascii_strcasecmp(tok, "content-box") == 0;
}

static gboolean
bg_token_is_image(const char *tok)
{
    return g_ascii_strcasecmp(tok, "none") == 0 ||
           g_ascii_strncasecmp(tok, "url(", 4) == 0 ||
           ns_css_text_starts_gradient(tok) || ns_css_text_starts_image_set(tok);
}

static gboolean
bg_layer_parse(const char *text, gboolean final_layer, bg_layer_text *out,
               char **color_out)
{
    char *raw[24] = {0};
    int nraw = split_ws_limit(text, raw, G_N_ELEMENTS(raw));
    GPtrArray *toks = g_ptr_array_new_with_free_func(g_free);
    for (int i = 0; i < nraw; i++) {
        const char *t = raw[i];
        const char *slash = strchr(t, '(') ? NULL : strchr(t, '/');
        if (!slash) {
            g_ptr_array_add(toks, g_strdup(t));
        } else {
            if (slash > t) g_ptr_array_add(toks, g_strndup(t, (gsize)(slash - t)));
            g_ptr_array_add(toks, g_strdup("/"));
            if (slash[1]) g_ptr_array_add(toks, g_strdup(slash + 1));
        }
        g_free(raw[i]);
    }
    gboolean ok = toks->len > 0;
    const char *repeat_a = NULL, *repeat_b = NULL;
    int n_repeat = 0, n_box = 0, n_pos = 0;
    gboolean pos_closed = FALSE;
    GString *pos = NULL, *size = NULL;
    char *clip_only = NULL;
    for (guint i = 0; ok && i < toks->len; i++) {
        const char *tok = g_ptr_array_index(toks, i);
        guint8 r, g, b, a;
        if (strcmp(tok, "/") == 0) {
            if (!pos || pos_closed || size) { ok = FALSE; break; }
            pos_closed = TRUE;
            size = g_string_new(NULL);
            int n_size = 0;
            while (i + 1 < toks->len) {
                const char *nx = g_ptr_array_index(toks, i + 1);
                if (n_size == 0 && (g_ascii_strcasecmp(nx, "cover") == 0 ||
                                    g_ascii_strcasecmp(nx, "contain") == 0)) {
                    char *lower = g_ascii_strdown(nx, -1);
                    g_string_append(size, lower);
                    g_free(lower);
                    i++;
                    n_size = 2;
                    break;
                }
                if (n_size < 2 && (g_ascii_strcasecmp(nx, "auto") == 0 ||
                                   bg_token_is_length_like(nx, TRUE))) {
                    if (size->len) g_string_append_c(size, ' ');
                    g_string_append(size, nx);
                    i++;
                    n_size++;
                    continue;
                }
                break;
            }
            if (n_size == 0) ok = FALSE;
            continue;
        }
        if (bg_token_is_position(tok)) {
            if (pos_closed || n_pos >= 4) { ok = FALSE; break; }
            if (!pos) pos = g_string_new(NULL);
            if (pos->len) g_string_append_c(pos, ' ');
            g_string_append(pos, tok);
            n_pos++;
            continue;
        }
        if (pos) pos_closed = TRUE;
        if (bg_token_is_image(tok)) {
            if (out->image) { ok = FALSE; break; }
            out->image = g_strdup(tok);
        } else if (ns_css_bg_repeat_token(tok, TRUE)) {
            gboolean axis = g_ascii_strncasecmp(tok, "repeat-", 7) == 0;
            if (n_repeat >= 2 || (n_repeat == 1 && (axis ||
                g_ascii_strncasecmp(repeat_a, "repeat-", 7) == 0))) {
                ok = FALSE;
                break;
            }
            if (n_repeat == 0) repeat_a = tok;
            else repeat_b = tok;
            n_repeat++;
        } else if (g_ascii_strcasecmp(tok, "scroll") == 0 ||
                   g_ascii_strcasecmp(tok, "fixed") == 0 ||
                   g_ascii_strcasecmp(tok, "local") == 0) {
            if (out->attachment) { ok = FALSE; break; }
            out->attachment = g_ascii_strdown(tok, -1);
        } else if (bg_token_is_box(tok)) {
            if (n_box >= 2) { ok = FALSE; break; }
            if (n_box == 0) {
                out->origin = g_ascii_strdown(tok, -1);
                g_free(out->clip);
                out->clip = g_ascii_strdown(tok, -1);
            } else {
                g_free(out->clip);
                out->clip = g_ascii_strdown(tok, -1);
            }
            n_box++;
        } else if (g_ascii_strcasecmp(tok, "text") == 0 ||
                   g_ascii_strcasecmp(tok, "border-area") == 0) {
            if (n_box >= 2 || (clip_only && strstr(clip_only, " "))) {
                ok = FALSE;
                break;
            }
            char *lower = g_ascii_strdown(tok, -1);
            char *joined = clip_only
                ? g_strdup_printf("%s %s", clip_only, lower) : g_strdup(lower);
            g_free(clip_only);
            clip_only = ns_css_bg_clip_canonical(joined);
            g_free(joined);
            g_free(lower);
            if (!clip_only) { ok = FALSE; break; }
        } else if (ns_css_parse_color(tok, &r, &g, &b, &a) ||
                   g_ascii_strcasecmp(tok, "currentcolor") == 0) {
            if (!final_layer || *color_out) { ok = FALSE; break; }
            *color_out = g_strdup(tok);
        } else {
            ok = FALSE;
        }
    }
    if (ok && pos) {
        char *canon = ns_css_position_canonical_ex(pos->str, TRUE, TRUE);
        if (canon) {
            position_split_specified(canon, &out->pos_x, &out->pos_y);
            g_free(canon);
        } else {
            ok = FALSE;
        }
    }
    if (ok && size) {
        ns_css_value *sv = ns_css_parse_value_for(NS_CSS_BACKGROUND_SIZE, size->str);
        if (sv) out->size = ns_css_add_leading_zeros(g_strdup(size->str));
        else ok = FALSE;
        ns_css_value_free(sv);
    }
    if (ok && n_repeat) out->repeat = ns_css_bg_repeat_canonical(repeat_a, repeat_b);
    if (ok && clip_only) {
        g_free(out->clip);
        out->clip = clip_only;
        clip_only = NULL;
        if (!out->origin) out->origin = g_strdup("border-box");
    }
    g_free(clip_only);
    if (ok && out->image) {
        char *canon = ns_css_image_value_canonical(out->image);
        if (canon) {
            g_free(out->image);
            out->image = canon;
        } else if (g_ascii_strcasecmp(out->image, "none") != 0) {
            ok = FALSE;
        }
    }
    if (pos) g_string_free(pos, TRUE);
    if (size) g_string_free(size, TRUE);
    g_ptr_array_free(toks, TRUE);
    return ok;
}

static void
mask_layer_chain_append(ns_css_value **head, ns_css_value **tail,
                        ns_css_value *v)
{
    if (*tail) (*tail)->next_layer = v;
    else *head = v;
    *tail = v;
}

typedef struct mask_layer_parts {
    const char *image;
    const char *boxes[2];
    int n_boxes;
    const char *op;
} mask_layer_parts;

static void
mask_layer_classify(char **toks, int n, mask_layer_parts *out)
{
    for (int i = 0; i < n; i++) {
        const char *box = ns_css_mask_box_keyword(toks[i]);
        const char *comp = ns_css_mask_composite_keyword(toks[i]);
        if (box && out->n_boxes < 2) out->boxes[out->n_boxes++] = box;
        else if (comp && !out->op) out->op = comp;
        else if (!out->image && (strchr(toks[i], '(') ||
                                 g_ascii_strcasecmp(toks[i], "none") == 0))
            out->image = toks[i];
    }
}

static gboolean
mask_layer_append(const char *layer, ns_css_value **heads,
                  ns_css_value **tails)
{
    char *toks[24] = {0};
    int n = split_ws_limit(layer, toks, G_N_ELEMENTS(toks));
    mask_layer_parts parts = {0};
    mask_layer_classify(toks, n, &parts);
    ns_css_value *iv = ns_css_parse_value_for(NS_CSS_MASK_IMAGE,
                                       parts.image ? parts.image : "none");
    gboolean ok = iv != NULL;
    if (ok) {
        const char *clip = parts.n_boxes ? parts.boxes[parts.n_boxes - 1]
                                         : "border-box";
        mask_layer_chain_append(&heads[0], &tails[0], iv);
        mask_layer_chain_append(&heads[1], &tails[1], keyword_value_dup(clip));
        mask_layer_chain_append(&heads[2], &tails[2],
                                keyword_value_dup(parts.op ? parts.op : "add"));
    }
    for (int i = 0; i < n; i++) g_free(toks[i]);
    return ok;
}

static gboolean
parse_mask_shorthand(const char *vtext, gboolean important, GArray *decls_out)
{
    static const ns_css_prop props[] = {
        NS_CSS_MASK_IMAGE, NS_CSS_MASK_CLIP, NS_CSS_MASK_COMPOSITE,
    };
    ns_css_value *heads[3] = {0}, *tails[3] = {0};
    ns_css_value *wide = parse_css_wide_keyword(vtext);
    gboolean ok = TRUE;
    if (wide) {
        for (gsize f = 0; f < G_N_ELEMENTS(props); f++)
            heads[f] = f ? ns_css_value_dup(wide) : wide;
    } else {
        GPtrArray *layers = ns_css_split_top_level_commas(vtext);
        ok = layers->len > 0;
        for (guint li = 0; ok && li < layers->len; li++)
            ok = mask_layer_append(g_ptr_array_index(layers, li), heads, tails);
        g_ptr_array_free(layers, TRUE);
    }
    for (gsize f = 0; f < G_N_ELEMENTS(props); f++) {
        if (!ok) {
            ns_css_value_free(heads[f]);
            continue;
        }
        ns_css_decl d = { .prop = props[f], .value = heads[f],
                          .important = important };
        g_array_append_val(decls_out, d);
    }
    return ok;
}

static gboolean
parse_background_shorthand(const char *vtext, gboolean important,
                           GArray *decls_out)
{
    static const struct { ns_css_prop prop; const char *initial; } fields[] = {
        { NS_CSS_BACKGROUND_IMAGE, "none" },
        { NS_CSS_BACKGROUND_POSITION_X, "0%" },
        { NS_CSS_BACKGROUND_POSITION_Y, "0%" },
        { NS_CSS_BACKGROUND_SIZE, "auto" },
        { NS_CSS_BACKGROUND_REPEAT, "repeat" },
        { NS_CSS_BACKGROUND_ATTACHMENT, "scroll" },
        { NS_CSS_BACKGROUND_ORIGIN, "padding-box" },
        { NS_CSS_BACKGROUND_CLIP, "border-box" },
    };
    ns_css_value *wide = parse_css_wide_keyword(vtext);
    if (wide) {
        for (gsize f = 0; f <= G_N_ELEMENTS(fields); f++) {
            ns_css_prop prop = f < G_N_ELEMENTS(fields)
                ? fields[f].prop : NS_CSS_BACKGROUND_COLOR;
            ns_css_decl d = { .prop = prop,
                              .value = f ? ns_css_value_dup(wide) : wide,
                              .important = important };
            g_array_append_val(decls_out, d);
        }
        return TRUE;
    }
    GPtrArray *layers = ns_css_split_top_level_commas(vtext);
    GArray *parsed = g_array_new(FALSE, TRUE, sizeof(bg_layer_text));
    g_array_set_size(parsed, layers->len);
    char *color = NULL;
    gboolean ok = layers->len > 0;
    for (guint i = 0; ok && i < layers->len; i++)
        ok = bg_layer_parse(g_ptr_array_index(layers, i),
                            i + 1 == layers->len,
                            &g_array_index(parsed, bg_layer_text, i), &color);
    if (ok) {
        for (gsize f = 0; ok && f < G_N_ELEMENTS(fields); f++) {
            ns_css_value *head = NULL, *tail = NULL;
            for (guint i = 0; ok && i < parsed->len; i++) {
                bg_layer_text *l = &g_array_index(parsed, bg_layer_text, i);
                const char *layer_texts[8] = {
                    l->image, l->pos_x, l->pos_y, l->size, l->repeat,
                    l->attachment, l->origin, l->clip,
                };
                const char *text = layer_texts[f];
                if (!text) text = fields[f].initial;
                ns_css_value *v = ns_css_parse_value_for(fields[f].prop, text);
                if (!v) { ok = FALSE; break; }
                g_free(v->specified);
                v->specified = g_strdup(text);
                if (tail) tail->next_layer = v;
                else head = v;
                tail = v;
            }
            if (!ok) {
                ns_css_value_free(head);
                break;
            }
            ns_css_decl d = { .prop = fields[f].prop, .value = head,
                              .important = important };
            g_array_append_val(decls_out, d);
        }
        if (ok) {
            ns_css_value *cv = ns_css_parse_value_for(NS_CSS_BACKGROUND_COLOR,
                                               color ? color : "transparent");
            if (cv) {
                ns_css_decl d = { .prop = NS_CSS_BACKGROUND_COLOR, .value = cv,
                                  .important = important };
                g_array_append_val(decls_out, d);
            }
        }
    }
    for (guint i = 0; i < parsed->len; i++)
        bg_layer_text_clear(&g_array_index(parsed, bg_layer_text, i));
    g_array_free(parsed, TRUE);
    g_ptr_array_free(layers, TRUE);
    g_free(color);
    return ok;
}


static gboolean
border_shorthand_valid(const char *vtext, ns_css_prop style_prop)
{
    ns_css_value *wide = parse_css_wide_keyword(vtext);
    if (wide) {
        ns_css_value_free(wide);
        return TRUE;
    }
    char *tokens[5] = {0};
    int n = split_ws_limit(vtext, tokens, G_N_ELEMENTS(tokens));
    gboolean ok = n >= 1 && n <= 3;
    gboolean saw_color = FALSE, saw_width = FALSE, saw_style = FALSE;
    for (int i = 0; ok && i < n; i++) {
        const char *tok = tokens[i];
        guint8 r, g, b, a;
        double num;
        ns_css_unit unit;
        if (ns_css_parse_color(tok, &r, &g, &b, &a) || is_color_keyword(tok)) {
            ok = !saw_color;
            saw_color = TRUE;
        } else if (g_ascii_strcasecmp(tok, "thin") == 0 ||
                   g_ascii_strcasecmp(tok, "medium") == 0 ||
                   g_ascii_strcasecmp(tok, "thick") == 0) {
            ok = !saw_width;
            saw_width = TRUE;
        } else if (ns_css_parse_length(tok, &num, &unit)) {
            ok = !saw_width && num >= 0 && unit != NS_CSS_UNIT_PERCENT &&
                 (unit != NS_CSS_UNIT_NUMBER || num == 0);
            saw_width = TRUE;
        } else {
            ns_css_value *calc = ns_css_parse_calc(tok);
            if (calc) {
                ns_css_value_free(calc);
                ok = !saw_width;
                saw_width = TRUE;
                continue;
            }
            ns_css_value *style = ns_css_parse_value_for(style_prop, tok);
            ok = !saw_style && style != NULL;
            ns_css_value_free(style);
            saw_style = TRUE;
        }
    }
    for (int i = 0; i < n; i++) g_free(tokens[i]);
    return ok;
}

static void
parse_declaration_block(const char **pp, const char *end,
                        GArray *decls_out, ns_css_rule *capture)
{

    const char *p = *pp;
    while (p < end && *p != '}') {
        p = css_skip_ws_comments(p, end);
        while (p < end && *p == ';') {
            p++;
            p = css_skip_ws_comments(p, end);
        }
        if (p >= end || *p == '}') break;

        char *name = ns_css_read_ident(&p, end);
        if (!name || !*name) {
            g_free(name);
            const char *before = p;
            char term = 0;
            const char *skip_to = css_scan_segment(p, end, &term);
            if (term == '{') {
                p = css_skip_to_block_end(skip_to, end);
            } else {
                p = term == ';' ? skip_to + 1 : skip_to;
            }
            if (p <= before) p = before + 1;
            continue;
        }
        char *pname;
        if (name[0] == '-' && name[1] == '-') {
            pname = name;
        } else {
            pname = ascii_lower(name, strlen(name));
            g_free(name);
            if (g_str_has_prefix(pname, "-webkit-border-") &&
                g_str_has_suffix(pname, "-radius")) {
                char *plain = g_strdup(pname + 8);
                g_free(pname);
                pname = plain;
            }
        }
        p = css_skip_ws_comments(p, end);
        if (p >= end || *p != ':') { g_free(pname);
            char term = 0;
            const char *skip_to = css_scan_segment(p, end, &term);
            p = (term == ';') ? skip_to + 1 : skip_to;
            continue;
        }
        p++;

        const char *vstart = p;
        char term = 0;
        const char *vend = css_scan_declaration_value(p, end, &term);
        p = vend;
        char *raw_vtext = g_strndup(vstart, (gsize)(vend - vstart));

        if (!css_declaration_value_syntax_valid(raw_vtext)) {
            g_free(raw_vtext);
            g_free(pname);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (capture && pname[0] == '-' && pname[1] == '-' && pname[2]) {
            char *trimmed = g_strstrip(g_strdup(raw_vtext));
            gboolean is_important = FALSE;
            css_strip_important(trimmed, &is_important);
            if (!capture->vars)
                capture->vars = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                     g_free, g_free);
            g_hash_table_replace(capture->vars, g_strdup(pname), trimmed);
            if (is_important) {
                if (!capture->var_important)
                    capture->var_important = g_hash_table_new_full(
                        g_str_hash, g_str_equal, g_free, NULL);
                g_hash_table_add(capture->var_important, g_strdup(pname));
            } else if (capture->var_important) {
                g_hash_table_remove(capture->var_important, pname);
            }
            g_free(raw_vtext);
            g_free(pname);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strstr(raw_vtext, "attr(") && !attr_functions_syntax_valid(raw_vtext)) {
            g_free(raw_vtext);
            g_free(pname);
            if (p < end && *p == ';') p++;
            continue;
        }
        if (capture && (strstr(raw_vtext, "var(") || strstr(raw_vtext, "attr(") ||
                        css_value_has_container_unit(raw_vtext))) {
            if (!capture->pending) {
                capture->pending = g_array_new(FALSE, FALSE,
                                               sizeof(ns_css_pending_decl));
                g_array_set_clear_func(capture->pending, pending_decl_clear);
            }
            gboolean is_important = FALSE;
            css_strip_important(raw_vtext, &is_important);
            int decl_index = capture->decls ? (int)capture->decls->len : 0;
            int decl_rank = 0;
            if (capture->pending->len > 0) {
                const ns_css_pending_decl *prev =
                    &g_array_index(capture->pending, ns_css_pending_decl,
                                   capture->pending->len - 1);
                if (prev->decl_index == decl_index)
                    decl_rank = prev->decl_rank + 1;
            }
            ns_css_pending_decl pd = {
                .pname = pname,
                .raw_vtext = raw_vtext,
                .important = is_important,
                .decl_index = decl_index,
                .decl_rank = decl_rank,
            };
            g_array_append_val(capture->pending, pd);
            if (p < end && *p == ';') p++;
            continue;
        }

        char *vtext = substitute_var_fallbacks(raw_vtext, 0);
        g_free(raw_vtext);
        gboolean important = FALSE;
        css_strip_important(vtext, &important);

        if (strcmp(pname, "all") == 0) {
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            if (wide) {
                for (int prop = 0; prop < NS_CSS_PROP_COUNT; prop++) {
                    if (prop == NS_CSS_DIRECTION ||
                        prop == NS_CSS_UNICODE_BIDI)
                        continue;
                    ns_css_decl d = {
                        .prop = (ns_css_prop)prop,
                        .value = ns_css_value_dup(wide),
                        .important = important
                    };
                    g_array_append_val(decls_out, d);
                }
                ns_css_value_free(wide);
            }
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "border-image") == 0 ||
            strcmp(pname, "-webkit-border-image") == 0) {
            expand_border_image(decls_out, vtext, important);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        static const struct { const char *name; ns_css_prop t,r,b,l; } border_sides[] = {
            { "border-top",    NS_CSS_BORDER_TOP_WIDTH,    NS_CSS_BORDER_TOP_COLOR,
                               NS_CSS_BORDER_TOP_STYLE,    NS_CSS_PROP_COUNT },
            { "border-right",  NS_CSS_BORDER_RIGHT_WIDTH,  NS_CSS_BORDER_RIGHT_COLOR,
                               NS_CSS_BORDER_RIGHT_STYLE,  NS_CSS_PROP_COUNT },
            { "border-bottom", NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_BOTTOM_COLOR,
                               NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_PROP_COUNT },
            { "border-left",   NS_CSS_BORDER_LEFT_WIDTH,   NS_CSS_BORDER_LEFT_COLOR,
                               NS_CSS_BORDER_LEFT_STYLE,   NS_CSS_PROP_COUNT },
            { "border-inline-start", NS_CSS_BORDER_LEFT_WIDTH,   NS_CSS_BORDER_LEFT_COLOR,
                                      NS_CSS_BORDER_LEFT_STYLE,   NS_CSS_PROP_COUNT },
            { "border-inline-end",   NS_CSS_BORDER_RIGHT_WIDTH,  NS_CSS_BORDER_RIGHT_COLOR,
                                      NS_CSS_BORDER_RIGHT_STYLE,  NS_CSS_PROP_COUNT },
            { "border-block-start",  NS_CSS_BORDER_TOP_WIDTH,    NS_CSS_BORDER_TOP_COLOR,
                                      NS_CSS_BORDER_TOP_STYLE,    NS_CSS_PROP_COUNT },
            { "border-block-end",    NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_BOTTOM_COLOR,
                                      NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_PROP_COUNT },
            { NULL, 0, 0, 0, 0 },
        };

        gboolean is_border_side = FALSE;
        int side_idx = -1;
        for (int i = 0; border_sides[i].name; i++) {
            if (strcmp(pname, border_sides[i].name) == 0) {
                is_border_side = TRUE; side_idx = i; break;
            }
        }
        if ((strcmp(pname, "border") == 0 || is_border_side) &&
            !strstr(vtext, "var(") &&
            !border_shorthand_valid(vtext, NS_CSS_BORDER_TOP_STYLE)) {
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }
        if (strcmp(pname, "border") == 0 || is_border_side) {
            char *tokens[4] = {0};
            int n = split_ws_limit(vtext, tokens, G_N_ELEMENTS(tokens));
            gboolean saw_color = FALSE, saw_width = FALSE, saw_style = FALSE;
            for (int i = 0; i < n; i++) {
                guint8 r, g, b, a;
                double num; ns_css_unit u;
                if (ns_css_parse_color(tokens[i], &r, &g, &b, &a) ||
                    is_color_keyword(tokens[i])) {
                    saw_color = TRUE;
                    if (is_border_side) {
                        ns_css_value *v = ns_css_parse_value_for(border_sides[side_idx].r, tokens[i]);
                        if (v) {
                            ns_css_decl d = { .prop = border_sides[side_idx].r, .value = v, .important = important };
                            g_array_append_val(decls_out, d);
                        }
                    } else {
                        char *quad[4] = { tokens[i], tokens[i], tokens[i], tokens[i] };
                        emit_quad(decls_out,
                            NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
                            NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
                            quad, 4, important);
                    }
                } else if (ns_css_parse_length(tokens[i], &num, &u) ||
                           g_ascii_strcasecmp(tokens[i], "thin") == 0 ||
                           g_ascii_strcasecmp(tokens[i], "medium") == 0 ||
                           g_ascii_strcasecmp(tokens[i], "thick") == 0) {
                    saw_width = TRUE;
                    if (is_border_side) {
                        ns_css_value *v = ns_css_parse_value_for(border_sides[side_idx].t, tokens[i]);
                        if (v) {
                            ns_css_decl d = { .prop = border_sides[side_idx].t, .value = v, .important = important };
                            g_array_append_val(decls_out, d);
                        }
                    } else {
                        char *quad[4] = { tokens[i], tokens[i], tokens[i], tokens[i] };
                        emit_quad(decls_out,
                            NS_CSS_BORDER_TOP_WIDTH, NS_CSS_BORDER_RIGHT_WIDTH,
                            NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_LEFT_WIDTH,
                            quad, 4, important);
                    }
                } else {
                    saw_style = TRUE;
                    if (is_border_side) {
                        ns_css_value *v = ns_css_parse_value_for(border_sides[side_idx].b, tokens[i]);
                        if (v) {
                            ns_css_decl d = { .prop = border_sides[side_idx].b, .value = v, .important = important };
                            g_array_append_val(decls_out, d);
                        }
                    } else {
                        char *quad[4] = { tokens[i], tokens[i], tokens[i], tokens[i] };
                        emit_quad(decls_out,
                            NS_CSS_BORDER_TOP_STYLE, NS_CSS_BORDER_RIGHT_STYLE,
                            NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_BORDER_LEFT_STYLE,
                            quad, 4, important);
                    }
                }
            }
            if (n > 0 && !(saw_color && saw_width && saw_style)) {
                static const struct { ns_css_prop t, r, b, l; const char *def; }
                    border_initials[3] = {
                    { NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
                      NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
                      "currentcolor" },
                    { NS_CSS_BORDER_TOP_WIDTH, NS_CSS_BORDER_RIGHT_WIDTH,
                      NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_LEFT_WIDTH,
                      "medium" },
                    { NS_CSS_BORDER_TOP_STYLE, NS_CSS_BORDER_RIGHT_STYLE,
                      NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_BORDER_LEFT_STYLE,
                      "none" },
                };
                gboolean seen[3] = { saw_color, saw_width, saw_style };
                for (int k = 0; k < 3; k++) {
                    if (seen[k]) continue;
                    if (is_border_side) {
                        ns_css_prop sp = k == 0 ? border_sides[side_idx].r
                                       : k == 1 ? border_sides[side_idx].t
                                                : border_sides[side_idx].b;
                        ns_css_value *v = ns_css_parse_value_for(sp, border_initials[k].def);
                        if (v) {
                            ns_css_decl d = { .prop = sp, .value = v, .important = important };
                            g_array_append_val(decls_out, d);
                        }
                    } else {
                        char *q = (char *)border_initials[k].def;
                        char *quad[4] = { q, q, q, q };
                        emit_quad(decls_out, border_initials[k].t, border_initials[k].r,
                                  border_initials[k].b, border_initials[k].l,
                                  quad, 4, important);
                    }
                }
            }
            if (n > 0 && !is_border_side)
                emit_border_image_initial(decls_out, important);
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "border-block") == 0 ||
            strcmp(pname, "border-inline") == 0) {
            gboolean is_block = strcmp(pname, "border-block") == 0;
            ns_css_prop w1 = is_block ? NS_CSS_BORDER_TOP_WIDTH : NS_CSS_BORDER_LEFT_WIDTH;
            ns_css_prop w2 = is_block ? NS_CSS_BORDER_BOTTOM_WIDTH : NS_CSS_BORDER_RIGHT_WIDTH;
            ns_css_prop c1 = is_block ? NS_CSS_BORDER_TOP_COLOR : NS_CSS_BORDER_LEFT_COLOR;
            ns_css_prop c2 = is_block ? NS_CSS_BORDER_BOTTOM_COLOR : NS_CSS_BORDER_RIGHT_COLOR;
            ns_css_prop s1 = is_block ? NS_CSS_BORDER_TOP_STYLE : NS_CSS_BORDER_LEFT_STYLE;
            ns_css_prop s2 = is_block ? NS_CSS_BORDER_BOTTOM_STYLE : NS_CSS_BORDER_RIGHT_STYLE;
            char *tokens[4] = {0};
            int n = split_ws_limit(vtext, tokens, G_N_ELEMENTS(tokens));
            for (int i = 0; i < n; i++) {
                guint8 r, g, b, a;
                double num; ns_css_unit u;
                ns_css_prop p1, p2;
                if (ns_css_parse_color(tokens[i], &r, &g, &b, &a) ||
                    is_color_keyword(tokens[i])) {
                    p1 = c1; p2 = c2;
                } else if (ns_css_parse_length(tokens[i], &num, &u)) {
                    p1 = w1; p2 = w2;
                } else {
                    p1 = s1; p2 = s2;
                }
                ns_css_value *v1 = ns_css_parse_value_for(p1, tokens[i]);
                ns_css_value *v2 = ns_css_parse_value_for(p2, tokens[i]);
                if (v1) {
                    ns_css_decl d = { .prop = p1, .value = v1, .important = important };
                    g_array_append_val(decls_out, d);
                }
                if (v2) {
                    ns_css_decl d = { .prop = p2, .value = v2, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        static const struct { const char *name; ns_css_prop a,b; } border_pair_props[] = {
            { "border-block-width",  NS_CSS_BORDER_TOP_WIDTH,    NS_CSS_BORDER_BOTTOM_WIDTH },
            { "border-inline-width", NS_CSS_BORDER_LEFT_WIDTH,   NS_CSS_BORDER_RIGHT_WIDTH },
            { "border-block-style",  NS_CSS_BORDER_TOP_STYLE,    NS_CSS_BORDER_BOTTOM_STYLE },
            { "border-inline-style", NS_CSS_BORDER_LEFT_STYLE,   NS_CSS_BORDER_RIGHT_STYLE },
            { "border-block-color",  NS_CSS_BORDER_TOP_COLOR,    NS_CSS_BORDER_BOTTOM_COLOR },
            { "border-inline-color", NS_CSS_BORDER_LEFT_COLOR,   NS_CSS_BORDER_RIGHT_COLOR },
            { NULL, NS_CSS_PROP_COUNT, NS_CSS_PROP_COUNT },
        };
        gboolean border_pair_prop = FALSE;
        for (int i = 0; border_pair_props[i].name; i++) {
            if (strcmp(pname, border_pair_props[i].name) != 0) continue;
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            if (n > 0) {
                const char *a = tokens[0];
                const char *b = n >= 2 ? tokens[1] : a;
                ns_css_value *va = ns_css_parse_value_for(border_pair_props[i].a, a);
                ns_css_value *vb = ns_css_parse_value_for(border_pair_props[i].b, b);
                if (va) {
                    ns_css_decl d = { .prop = border_pair_props[i].a, .value = va, .important = important };
                    g_array_append_val(decls_out, d);
                }
                if (vb) {
                    ns_css_decl d = { .prop = border_pair_props[i].b, .value = vb, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            for (int j = 0; j < n; j++) g_free(tokens[j]);
            border_pair_prop = TRUE;
            break;
        }
        if (border_pair_prop) {
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "overflow") == 0) {
            char *tokens[3] = {0};
            int n = split_ws_limit(vtext, tokens, G_N_ELEMENTS(tokens));
            if (n == 2 || (n == 1 && !strchr(tokens[0], '('))) {
                ns_css_value *vx = ns_css_parse_value_for(NS_CSS_OVERFLOW_X, tokens[0]);
                ns_css_value *vy = ns_css_parse_value_for(NS_CSS_OVERFLOW_Y, tokens[n - 1]);
                if (vx) {
                    ns_css_decl d = { .prop = NS_CSS_OVERFLOW_X, .value = vx, .important = important };
                    g_array_append_val(decls_out, d);
                }
                if (vy) {
                    ns_css_decl d = { .prop = NS_CSS_OVERFLOW_Y, .value = vy, .important = important };
                    g_array_append_val(decls_out, d);
                }
                for (int i = 0; i < n; i++) g_free(tokens[i]);
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
        }

        static const struct { const char *name; ns_css_prop prop; } prop_aliases[] = {
            { "grid-row-gap", NS_CSS_ROW_GAP },
            { "grid-column-gap", NS_CSS_COLUMN_GAP },
            { "-webkit-user-select", NS_CSS_USER_SELECT },
            { "-moz-user-select", NS_CSS_USER_SELECT },
            { "inline-size", NS_CSS_WIDTH },
            { "block-size", NS_CSS_HEIGHT },
            { "min-inline-size", NS_CSS_MIN_WIDTH },
            { "max-inline-size", NS_CSS_MAX_WIDTH },
            { "min-block-size", NS_CSS_MIN_HEIGHT },
            { "max-block-size", NS_CSS_MAX_HEIGHT },
            { "margin-inline-start", NS_CSS_MARGIN_LEFT },
            { "margin-inline-end", NS_CSS_MARGIN_RIGHT },
            { "margin-block-start", NS_CSS_MARGIN_TOP },
            { "margin-block-end", NS_CSS_MARGIN_BOTTOM },
            { "padding-inline-start", NS_CSS_PADDING_LEFT },
            { "padding-inline-end", NS_CSS_PADDING_RIGHT },
            { "padding-block-start", NS_CSS_PADDING_TOP },
            { "padding-block-end", NS_CSS_PADDING_BOTTOM },
            { "inset-inline-start", NS_CSS_LEFT },
            { "inset-inline-end", NS_CSS_RIGHT },
            { "inset-block-start", NS_CSS_TOP },
            { "inset-block-end", NS_CSS_BOTTOM },
            { "border-inline-start-width", NS_CSS_BORDER_LEFT_WIDTH },
            { "border-inline-end-width", NS_CSS_BORDER_RIGHT_WIDTH },
            { "border-block-start-width", NS_CSS_BORDER_TOP_WIDTH },
            { "border-block-end-width", NS_CSS_BORDER_BOTTOM_WIDTH },
            { "border-inline-start-style", NS_CSS_BORDER_LEFT_STYLE },
            { "border-inline-end-style", NS_CSS_BORDER_RIGHT_STYLE },
            { "border-block-start-style", NS_CSS_BORDER_TOP_STYLE },
            { "border-block-end-style", NS_CSS_BORDER_BOTTOM_STYLE },
            { "border-inline-start-color", NS_CSS_BORDER_LEFT_COLOR },
            { "border-inline-end-color", NS_CSS_BORDER_RIGHT_COLOR },
            { "border-block-start-color", NS_CSS_BORDER_TOP_COLOR },
            { "border-block-end-color", NS_CSS_BORDER_BOTTOM_COLOR },
            { "border-start-start-radius", NS_CSS_BORDER_TOP_LEFT_RADIUS },
            { "border-start-end-radius", NS_CSS_BORDER_TOP_RIGHT_RADIUS },
            { "border-end-start-radius", NS_CSS_BORDER_BOTTOM_LEFT_RADIUS },
            { "border-end-end-radius", NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS },
            { "page-break-before", NS_CSS_BREAK_BEFORE },
            { "page-break-after", NS_CSS_BREAK_AFTER },
            { "page-break-inside", NS_CSS_BREAK_INSIDE },
            { NULL, NS_CSS_PROP_COUNT },
        };
        gboolean aliased_prop = FALSE;
        for (int i = 0; prop_aliases[i].name; i++) {
            if (strcmp(pname, prop_aliases[i].name) != 0) continue;
            const char *atext = vtext;
            if (g_str_has_prefix(prop_aliases[i].name, "page-break-") &&
                g_ascii_strcasecmp(vtext, "always") == 0)
                atext = "page";
            ns_css_value *vv = ns_css_parse_value_for(prop_aliases[i].prop, atext);
            if (vv) {
                ns_css_decl d = {
                    .prop = prop_aliases[i].prop,
                    .value = vv,
                    .important = important,
                };
                g_array_append_val(decls_out, d);
            }
            aliased_prop = TRUE;
            break;
        }
        if (aliased_prop) {
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "mask") == 0 || strcmp(pname, "-webkit-mask") == 0) {
            parse_mask_shorthand(vtext, important, decls_out);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "background") == 0) {
            parse_background_shorthand(vtext, important, decls_out);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "background-position") == 0 ||
            strcmp(pname, "object-position") == 0) {
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            if (wide) {
                gboolean bg = strcmp(pname, "background-position") == 0;
                ns_css_decl dx = { .prop = bg ? NS_CSS_BACKGROUND_POSITION_X
                                              : NS_CSS_OBJECT_POSITION_X,
                                   .value = wide, .important = important };
                ns_css_decl dy = { .prop = bg ? NS_CSS_BACKGROUND_POSITION_Y
                                              : NS_CSS_OBJECT_POSITION_Y,
                                   .value = ns_css_value_dup(wide),
                                   .important = important };
                g_array_append_val(decls_out, dx);
                g_array_append_val(decls_out, dy);
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
        }
        if (strcmp(pname, "background-position") == 0) {
            ns_css_value *vx_head = NULL, *vx_tail = NULL;
            ns_css_value *vy_head = NULL, *vy_tail = NULL;
            const char *sp = vtext;
            const char *send = vtext + strlen(vtext);
            while (sp < send) {
                char sterm = 0;
                const char *sseg = css_scan_until(sp, send, ",", &sterm);
                char *layer = css_trim_dup_range(sp, sseg);
                sp = sterm == ',' ? sseg + 1 : sseg;
                char *layer_canon = ns_css_position_canonical_ex(layer, TRUE, TRUE);
                if (!layer_canon) {
                    g_free(layer);
                    ns_css_value_free(vx_head);
                    ns_css_value_free(vy_head);
                    vx_head = vy_head = NULL;
                    break;
                }
                char *xs = NULL, *ys = NULL, *sx = NULL, *sy = NULL;
                ns_css_position_split(layer, &xs, &ys);
                position_split_specified(layer_canon, &sx, &sy);
                g_free(layer_canon);
                if (xs) {
                    ns_css_value *v = ns_css_parse_value_for(NS_CSS_BACKGROUND_POSITION_X, xs);
                    if (v) {
                        g_free(v->specified);
                        v->specified = sx;
                        sx = NULL;
                        if (vx_tail) vx_tail->next_layer = v;
                        else vx_head = v;
                        vx_tail = v;
                    }
                }
                if (ys) {
                    ns_css_value *v = ns_css_parse_value_for(NS_CSS_BACKGROUND_POSITION_Y, ys);
                    if (v) {
                        g_free(v->specified);
                        v->specified = sy;
                        sy = NULL;
                        if (vy_tail) vy_tail->next_layer = v;
                        else vy_head = v;
                        vy_tail = v;
                    }
                }
                g_free(xs);
                g_free(ys);
                g_free(sx);
                g_free(sy);
                g_free(layer);
            }
            if (vx_head) {
                ns_css_decl d = { .prop = NS_CSS_BACKGROUND_POSITION_X, .value = vx_head, .important = important };
                g_array_append_val(decls_out, d);
            }
            if (vy_head) {
                ns_css_decl d = { .prop = NS_CSS_BACKGROUND_POSITION_Y, .value = vy_head, .important = important };
                g_array_append_val(decls_out, d);
            }
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "object-position") == 0) {
            char *xs = NULL, *ys = NULL;
            char *canon = ns_css_position_canonical_ex(vtext, TRUE, FALSE);
            if (canon) ns_css_position_split(vtext, &xs, &ys);
            g_free(canon);
            if (xs) {
                ns_css_value *v = ns_css_parse_value_for(NS_CSS_OBJECT_POSITION_X, xs);
                if (v) {
                    ns_css_decl d = { .prop = NS_CSS_OBJECT_POSITION_X, .value = v, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            if (ys) {
                ns_css_value *v = ns_css_parse_value_for(NS_CSS_OBJECT_POSITION_Y, ys);
                if (v) {
                    ns_css_decl d = { .prop = NS_CSS_OBJECT_POSITION_Y, .value = v, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            g_free(xs);
            g_free(ys);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "grid-template") == 0 ||
            strcmp(pname, "grid") == 0) {
            static const ns_css_prop grid_props[6] = {
                NS_CSS_GRID_TEMPLATE_ROWS, NS_CSS_GRID_TEMPLATE_COLUMNS,
                NS_CSS_GRID_TEMPLATE_AREAS, NS_CSS_GRID_AUTO_FLOW,
                NS_CSS_GRID_AUTO_ROWS, NS_CSS_GRID_AUTO_COLUMNS,
            };
            int n = pname[4] == '\0' ? 6 : 3;
            char *parts[6] = {0};
            char *canon = NULL;
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            gboolean ok = TRUE;
            if (wide) {
                for (int i = 0; i < n; i++) parts[i] = g_strdup(vtext);
                ns_css_value_free(wide);
            } else if (n == 6) {
                ok = ns_css_grid_shorthand_parse(vtext, parts, &canon);
            } else {
                ok = ns_css_grid_template_parse(vtext, parts, &canon);
            }
            for (int i = 0; ok && i < n; i++) {
                ns_css_value *v = ns_css_parse_value_for(grid_props[i], parts[i]);
                if (!v) continue;
                ns_css_decl d = { .prop = grid_props[i], .value = v,
                                  .important = important };
                g_array_append_val(decls_out, d);
            }
            for (int i = 0; i < n; i++) g_free(parts[i]);
            g_free(canon);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "gap") == 0 || strcmp(pname, "grid-gap") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            const char *row = n >= 1 ? tokens[0] : NULL;
            const char *col = n >= 2 ? tokens[1] : row;
            if (row) {
                ns_css_value *v = ns_css_parse_value_for(NS_CSS_ROW_GAP, row);
                if (v) {
                    ns_css_decl d = { .prop = NS_CSS_ROW_GAP, .value = v, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            if (col) {
                ns_css_value *v = ns_css_parse_value_for(NS_CSS_COLUMN_GAP, col);
                if (v) {
                    ns_css_decl d = { .prop = NS_CSS_COLUMN_GAP, .value = v, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "grid-area") == 0 ||
            strcmp(pname, "grid-column") == 0 ||
            strcmp(pname, "grid-row") == 0) {
            static const ns_css_prop area_props[4] = {
                NS_CSS_GRID_ROW_START, NS_CSS_GRID_COLUMN_START,
                NS_CSS_GRID_ROW_END, NS_CSS_GRID_COLUMN_END,
            };
            static const ns_css_prop row_props[2] = {
                NS_CSS_GRID_ROW_START, NS_CSS_GRID_ROW_END,
            };
            static const ns_css_prop column_props[2] = {
                NS_CSS_GRID_COLUMN_START, NS_CSS_GRID_COLUMN_END,
            };
            gboolean area = pname[5] == 'a';
            const ns_css_prop *props = area ? area_props
                                     : pname[5] == 'c' ? column_props
                                                       : row_props;
            char *parts[4] = {0};
            gboolean ident_only[4] = {0};
            int n = 0;
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            if (wide) {
                n = area ? 4 : 2;
                for (int i = 0; i < n; i++) parts[i] = g_strdup(vtext);
                ns_css_value_free(wide);
            } else {
                n = ns_css_grid_placement_expand(vtext, area, parts, ident_only);
            }
            for (int i = 0; i < n; i++) {
                ns_css_value *v = ns_css_parse_value_for(props[i], parts[i]);
                if (!v) continue;
                ns_css_decl d = { .prop = props[i], .value = v,
                                  .important = important };
                g_array_append_val(decls_out, d);
            }
            for (int i = 0; i < n; i++) g_free(parts[i]);
            if (n == 0 || !area) {
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
        }

        if (strcmp(pname, "place-items") == 0 ||
            strcmp(pname, "place-self") == 0 ||
            strcmp(pname, "place-content") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            ns_css_prop ap = NS_CSS_ALIGN_CONTENT, jp = NS_CSS_JUSTIFY_CONTENT;
            if (strcmp(pname, "place-items") == 0) {
                ap = NS_CSS_ALIGN_ITEMS;
                jp = NS_CSS_JUSTIFY_ITEMS;
            } else if (strcmp(pname, "place-self") == 0) {
                ap = NS_CSS_ALIGN_SELF;
                jp = NS_CSS_JUSTIFY_SELF;
            }
            ns_css_value *av = NULL, *jv = NULL;
            static const int splits[5][2] = {
                {0, 0}, {1, 0}, {2, 1}, {1, 2}, {2, 0},
            };
            for (int si = 0; si < 2 && n >= 1 && n <= 4; si++) {
                int k = splits[n][si];
                if (k == 0) break;
                char *first = k == 2 ? g_strdup_printf("%s %s", tokens[0], tokens[1])
                                     : g_strdup(tokens[0]);
                char *second;
                if (k >= n) second = g_strdup(first);
                else if (n - k == 2) second = g_strdup_printf("%s %s", tokens[k], tokens[k + 1]);
                else second = g_strdup(tokens[k]);
                av = ns_css_parse_value_for(ap, first);
                jv = av ? ns_css_parse_value_for(jp, second) : NULL;
                g_free(first);
                g_free(second);
                if (av && jv) break;
                if (av) ns_css_value_free(av);
                av = NULL;
            }
            if (av && jv) {
                ns_css_decl d = { .prop = ap, .value = av, .important = important };
                g_array_append_val(decls_out, d);
                ns_css_decl d2 = { .prop = jp, .value = jv, .important = important };
                g_array_append_val(decls_out, d2);
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "columns") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            for (int i = 0; i < n; i++) {
                double num; ns_css_unit u;
                if (ns_css_parse_length(tokens[i], &num, &u)) {
                    ns_css_prop prop = (u == NS_CSS_UNIT_NUMBER)
                        ? NS_CSS_COLUMN_COUNT : NS_CSS_COLUMN_WIDTH;
                    ns_css_value *v = ns_css_parse_value_for(prop, tokens[i]);
                    if (v) {
                        ns_css_decl d = { .prop = prop, .value = v,
                                          .important = important };
                        g_array_append_val(decls_out, d);
                    }
                }
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "outline") == 0 ||
            strcmp(pname, "column-rule") == 0) {
            gboolean is_outline = (strcmp(pname, "outline") == 0);
            ns_css_prop p_w = is_outline ? NS_CSS_OUTLINE_WIDTH : NS_CSS_COLUMN_RULE_WIDTH;
            ns_css_prop p_s = is_outline ? NS_CSS_OUTLINE_STYLE : NS_CSS_COLUMN_RULE_STYLE;
            ns_css_prop p_c = is_outline ? NS_CSS_OUTLINE_COLOR : NS_CSS_COLUMN_RULE_COLOR;
            if (!strstr(vtext, "var(") && !border_shorthand_valid(vtext, p_s)) {
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
            char *tokens[8] = {0};
            int n = split_ws(vtext, tokens);
            gboolean saw_c = FALSE, saw_w = FALSE, saw_s = FALSE;
            for (int i = 0; i < n; i++) {
                guint8 r, g, b, a;
                double num; ns_css_unit u;
                if (ns_css_parse_color(tokens[i], &r, &g, &b, &a) ||
                    is_color_keyword(tokens[i])) {
                    ns_css_value *v = ns_css_parse_value_for(p_c, tokens[i]);
                    if (v) {
                        ns_css_decl d = { .prop = p_c, .value = v, .important = important };
                        g_array_append_val(decls_out, d);
                        saw_c = TRUE;
                    }
                } else if (ns_css_parse_length(tokens[i], &num, &u) ||
                           g_ascii_strcasecmp(tokens[i], "thin") == 0 ||
                           g_ascii_strcasecmp(tokens[i], "medium") == 0 ||
                           g_ascii_strcasecmp(tokens[i], "thick") == 0) {
                    ns_css_value *v = ns_css_parse_value_for(p_w, tokens[i]);
                    if (v) {
                        ns_css_decl d = { .prop = p_w, .value = v, .important = important };
                        g_array_append_val(decls_out, d);
                        saw_w = TRUE;
                    }
                } else {
                    ns_css_value *v = ns_css_parse_value_for(p_s, tokens[i]);
                    if (v) {
                        ns_css_decl d = { .prop = p_s, .value = v, .important = important };
                        g_array_append_val(decls_out, d);
                        saw_s = TRUE;
                    }
                }
            }
            ns_css_value *wide = n == 1 ? parse_css_wide_keyword(tokens[0]) : NULL;
            if (n > 0 && !wide) {
                const struct { ns_css_prop prop; const char *def; gboolean seen; } rest[3] = {
                    { p_c, "currentcolor", saw_c },
                    { p_w, "medium", saw_w },
                    { p_s, "none", saw_s },
                };
                for (int k = 0; k < 3; k++) {
                    if (rest[k].seen) continue;
                    ns_css_value *v = ns_css_parse_value_for(rest[k].prop, rest[k].def);
                    if (!v) continue;
                    ns_css_decl d = { .prop = rest[k].prop, .value = v, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            ns_css_value_free(wide);
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "text-decoration") == 0 ||
            strcmp(pname, "text-decoration-line") == 0) {
            gboolean line_only = strcmp(pname, "text-decoration-line") == 0;
            char *tokens[8] = {0};
            int n = split_ws(vtext, tokens);
            GString *lines = g_string_new(NULL);
            for (int i = 0; i < n; i++) {
                const char *tk = tokens[i];
                if (!tk) continue;
                guint8 cr, cg, cb, ca;
                if (g_ascii_strcasecmp(tk, "underline") == 0 ||
                    g_ascii_strcasecmp(tk, "overline")  == 0 ||
                    g_ascii_strcasecmp(tk, "line-through") == 0 ||
                    g_ascii_strcasecmp(tk, "none") == 0) {
                    if (lines->len > 0) g_string_append_c(lines, ' ');
                    char *low = g_ascii_strdown(tk, -1);
                    g_string_append(lines, low);
                    g_free(low);
                } else if (line_only) {
                    continue;
                } else if (g_ascii_strcasecmp(tk, "solid")  == 0 ||
                           g_ascii_strcasecmp(tk, "double") == 0 ||
                           g_ascii_strcasecmp(tk, "dotted") == 0 ||
                           g_ascii_strcasecmp(tk, "dashed") == 0 ||
                           g_ascii_strcasecmp(tk, "wavy")   == 0) {
                    ns_css_value *v = g_new0(ns_css_value, 1);
                    v->kind = NS_CSS_V_KEYWORD;
                    v->u.keyword = g_ascii_strdown(tk, -1);
                    ns_css_decl d = {
                        .prop = NS_CSS_TEXT_DECORATION_STYLE,
                        .value = v, .important = important
                    };
                    g_array_append_val(decls_out, d);
                } else if (ns_css_parse_color(tk, &cr, &cg, &cb, &ca)) {
                    ns_css_value *v = g_new0(ns_css_value, 1);
                    v->kind = NS_CSS_V_COLOR;
                    v->u.color.r = cr; v->u.color.g = cg;
                    v->u.color.b = cb; v->u.color.a = ca;
                    ns_css_decl d = {
                        .prop = NS_CSS_TEXT_DECORATION_COLOR,
                        .value = v, .important = important
                    };
                    g_array_append_val(decls_out, d);
                }
            }
            if (lines->len > 0) {
                ns_css_value *v = g_new0(ns_css_value, 1);
                v->kind = NS_CSS_V_KEYWORD;
                v->u.keyword = g_string_free(lines, FALSE);
                lines = NULL;
                ns_css_decl d = {
                    .prop = NS_CSS_TEXT_DECORATION,
                    .value = v, .important = important
                };
                g_array_append_val(decls_out, d);
            }
            if (lines) g_string_free(lines, TRUE);
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "font") == 0) {
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            if (wide) {
                const ns_css_prop props[] = {
                    NS_CSS_FONT_STYLE,
                    NS_CSS_FONT_VARIANT,
                    NS_CSS_FONT_WEIGHT,
                    NS_CSS_FONT_STRETCH,
                    NS_CSS_FONT_KERNING,
                    NS_CSS_FONT_VARIANT_LIGATURES,
                    NS_CSS_FONT_FEATURE_SETTINGS,
                    NS_CSS_FONT_VARIATION_SETTINGS,
                    NS_CSS_FONT_SIZE,
                    NS_CSS_LINE_HEIGHT,
                    NS_CSS_FONT_FAMILY,
                };
                for (gsize i = 0; i < G_N_ELEMENTS(props); i++) {
                    ns_css_decl d = {
                        .prop = props[i],
                        .value = ns_css_value_dup(wide),
                        .important = important
                    };
                    g_array_append_val(decls_out, d);
                }
                ns_css_value_free(wide);
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
            guint font_decls_before = decls_out->len;
            {
                char *lower = g_ascii_strdown(g_strstrip(vtext), -1);
                gboolean system_font =
                    strcmp(lower, "caption") == 0 || strcmp(lower, "icon") == 0 ||
                    strcmp(lower, "menu") == 0 || strcmp(lower, "message-box") == 0 ||
                    strcmp(lower, "small-caption") == 0 ||
                    strcmp(lower, "status-bar") == 0;
                g_free(lower);
                if (system_font) {
                    static const struct { ns_css_prop prop; const char *value; }
                    sys_props[] = {
                        { NS_CSS_FONT_STYLE, "normal" },
                        { NS_CSS_FONT_VARIANT, "normal" },
                        { NS_CSS_FONT_WEIGHT, "normal" },
                        { NS_CSS_FONT_STRETCH, "normal" },
                        { NS_CSS_LINE_HEIGHT, "normal" },
                        { NS_CSS_FONT_FAMILY, "system-ui" },
                    };
                    for (gsize j = 0; j < G_N_ELEMENTS(sys_props); j++) {
                        ns_css_value *sv = g_new0(ns_css_value, 1);
                        sv->kind = NS_CSS_V_KEYWORD;
                        sv->u.keyword = g_strdup(sys_props[j].value);
                        ns_css_decl sd = { .prop = sys_props[j].prop, .value = sv,
                                           .important = important };
                        g_array_append_val(decls_out, sd);
                    }
                    ns_css_value *sz = g_new0(ns_css_value, 1);
                    sz->kind = NS_CSS_V_LENGTH;
                    sz->u.length.v = 13.3333;
                    sz->u.length.unit = NS_CSS_UNIT_PX;
                    ns_css_decl szd = { .prop = NS_CSS_FONT_SIZE, .value = sz,
                                        .important = important };
                    g_array_append_val(decls_out, szd);
                    g_free(pname);
                    g_free(vtext);
                    if (p < end && *p == ';') p++;
                    continue;
                }
            }
            {
                char *canon = ns_css_font_shorthand_canonical(vtext);
                if (!canon) {
                    g_free(pname);
                    g_free(vtext);
                    if (p < end && *p == ';') p++;
                    continue;
                }
                g_free(canon);
            }
            char *tokens[24] = {0};
            int n = ns_css_split_ws_paren(vtext, tokens, (int)G_N_ELEMENTS(tokens));
            char *family_buf = NULL;
            int size_idx = -1;
            for (int i = 0; i < n; i++) {
                if (ns_css_font_shorthand_is_size_token(tokens[i])) {
                    size_idx = i;
                    break;
                }
            }
            static const ns_css_prop font_reset_props[] = {
                NS_CSS_FONT_STYLE, NS_CSS_FONT_VARIANT, NS_CSS_FONT_WEIGHT,
                NS_CSS_FONT_STRETCH, NS_CSS_LINE_HEIGHT,
            };
            for (gsize j = 0; j < G_N_ELEMENTS(font_reset_props); j++) {
                ns_css_value *rv = g_new0(ns_css_value, 1);
                rv->kind = NS_CSS_V_KEYWORD;
                rv->u.keyword = g_strdup("normal");
                ns_css_decl rd = { .prop = font_reset_props[j], .value = rv,
                                   .important = important };
                g_array_append_val(decls_out, rd);
            }
            int prefix_end = size_idx >= 0 ? size_idx : 0;
            for (int i = 0; i < prefix_end; i++) {
                const char *t = tokens[i];
                ns_css_prop prop = NS_CSS_PROP_COUNT;
                const char *kw = NULL;
                if (g_ascii_strcasecmp(t, "italic") == 0 ||
                    g_ascii_strcasecmp(t, "oblique") == 0) {
                    prop = NS_CSS_FONT_STYLE; kw = "italic";
                } else if (g_ascii_strcasecmp(t, "bold")    == 0 ||
                           g_ascii_strcasecmp(t, "bolder")  == 0 ||
                           g_ascii_strcasecmp(t, "lighter") == 0) {
                    prop = NS_CSS_FONT_WEIGHT; kw = t;
                } else if (g_ascii_isdigit(t[0])) {
                    double num; ns_css_unit u;
                    if (ns_css_parse_length(t, &num, &u) &&
                        u == NS_CSS_UNIT_NUMBER &&
                        num >= 1 && num <= 1000) {
                        prop = NS_CSS_FONT_WEIGHT; kw = t;
                    }
                } else if (g_ascii_strcasecmp(t, "small-caps") == 0) {
                    prop = NS_CSS_FONT_VARIANT; kw = "small-caps";
                } else if (ns_css_font_stretch_keyword(t)) {
                    prop = NS_CSS_FONT_STRETCH; kw = t;
                }
                if (prop != NS_CSS_PROP_COUNT) {
                    ns_css_value *v = g_new0(ns_css_value, 1);
                    v->kind = NS_CSS_V_KEYWORD;
                    v->u.keyword = g_ascii_strdown(kw, -1);
                    ns_css_decl d = {
                        .prop = prop, .value = v, .important = important
                    };
                    g_array_append_val(decls_out, d);
                }
            }
            if (size_idx >= 0) {
                char *size_tok = tokens[size_idx];
                const char *slash = ns_css_font_shorthand_slash(size_tok);
                char *size_only = slash
                    ? g_strndup(size_tok, (gsize)(slash - size_tok))
                    : g_strdup(size_tok);
                char *lh_text = NULL;
                int family_start = size_idx + 1;
                if (slash) {
                    if (slash[1]) lh_text = g_strdup(slash + 1);
                    else if (family_start < n) lh_text = g_strdup(tokens[family_start++]);
                } else if (family_start < n && tokens[family_start][0] == '/') {
                    if (tokens[family_start][1])
                        lh_text = g_strdup(tokens[family_start] + 1);
                    else if (family_start + 1 < n)
                        lh_text = g_strdup(tokens[++family_start]);
                    family_start++;
                }
                ns_css_value *v = font_shorthand_size_value(size_only);
                g_free(size_only);
                if (v) {
                    ns_css_decl d = {
                        .prop = NS_CSS_FONT_SIZE, .value = v,
                        .important = important
                    };
                    g_array_append_val(decls_out, d);
                } else {
                    size_idx = -1;
                }
                if (lh_text && g_ascii_strcasecmp(lh_text, "normal") != 0) {
                    ns_css_value *lv = ns_css_parse_value_for(NS_CSS_LINE_HEIGHT, lh_text);
                    if (lv) {
                        ns_css_decl lhd = {
                            .prop = NS_CSS_LINE_HEIGHT,
                            .value = lv,
                            .important = important
                        };
                        g_array_append_val(decls_out, lhd);
                    } else {
                        size_idx = -1;
                    }
                }
                g_free(lh_text);
                if (family_start < n) {
                    GString *fam = g_string_new(NULL);
                    for (int j = family_start; j < n; j++) {
                        if (j > family_start) g_string_append_c(fam, ' ');
                        g_string_append(fam, tokens[j]);
                    }
                    family_buf = g_string_free(fam, FALSE);
                }
            }
            if (family_buf) {
                static const struct {
                    ns_css_prop prop;
                    const char *value;
                } reset_props[] = {
                    { NS_CSS_FONT_KERNING, "auto" },
                    { NS_CSS_FONT_VARIANT_LIGATURES, "normal" },
                    { NS_CSS_FONT_FEATURE_SETTINGS, "normal" },
                    { NS_CSS_FONT_VARIATION_SETTINGS, "normal" },
                };
                for (gsize j = 0; j < G_N_ELEMENTS(reset_props); j++) {
                    ns_css_value *rv = g_new0(ns_css_value, 1);
                    rv->kind = NS_CSS_V_KEYWORD;
                    rv->u.keyword = g_strdup(reset_props[j].value);
                    ns_css_decl rd = {
                        .prop = reset_props[j].prop,
                        .value = rv,
                        .important = important
                    };
                    g_array_append_val(decls_out, rd);
                }
                char *canon_family = ns_css_font_family_canonical(family_buf);
                g_free(family_buf);
                family_buf = canon_family;
                if (family_buf) {
                    ns_css_value *fv = g_new0(ns_css_value, 1);
                    fv->kind = NS_CSS_V_KEYWORD;
                    fv->u.keyword = family_buf;
                    ns_css_decl fd = {
                        .prop = NS_CSS_FONT_FAMILY, .value = fv,
                        .important = important
                    };
                    g_array_append_val(decls_out, fd);
                }
            }
            if (!family_buf || size_idx < 0) {
                for (guint k = font_decls_before; k < decls_out->len; k++)
                    ns_css_value_free(g_array_index(decls_out, ns_css_decl, k).value);
                g_array_set_size(decls_out, font_decls_before);
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "flex") == 0) {
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            if (wide) {
                static const ns_css_prop flex_longhands[] = {
                    NS_CSS_FLEX_GROW, NS_CSS_FLEX_SHRINK, NS_CSS_FLEX_BASIS,
                };
                for (gsize i = 0; i < G_N_ELEMENTS(flex_longhands); i++) {
                    ns_css_decl d = {
                        .prop = flex_longhands[i],
                        .value = i == 0 ? wide : ns_css_value_dup(wide),
                        .important = important,
                    };
                    g_array_append_val(decls_out, d);
                }
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            double grow = 0, shrink = 1;
            char *basis = NULL;
            gboolean basis_set = FALSE;
            gboolean keyword_set = FALSE;
            int numerics = 0;
            for (int i = 0; i < n; i++) {
                char *t = tokens[i];
                double num; ns_css_unit u;
                if (g_ascii_strcasecmp(t, "none") == 0) {
                    grow = 0; shrink = 0; keyword_set = TRUE;
                    g_free(basis);
                    basis = g_strdup("auto"); basis_set = TRUE;
                    break;
                }
                if (g_ascii_strcasecmp(t, "auto") == 0) {
                    if (numerics == 0) {
                        grow = 1; shrink = 1;
                    }
                    g_free(basis);
                    basis = g_strdup("auto"); basis_set = TRUE;
                    continue;
                }
                if (g_ascii_strcasecmp(t, "initial") == 0) {
                    grow = 0; shrink = 1; keyword_set = TRUE;
                    g_free(basis);
                    basis = g_strdup("auto"); basis_set = TRUE;
                    continue;
                }
                if (g_ascii_strncasecmp(t, "calc(", 5) == 0 ||
                    g_ascii_strncasecmp(t, "min(", 4) == 0 ||
                    g_ascii_strncasecmp(t, "max(", 4) == 0 ||
                    g_ascii_strncasecmp(t, "clamp(", 6) == 0) {
                    g_free(basis);
                    basis = g_strdup(t);
                    basis_set = TRUE;
                    continue;
                }
                if (ns_css_parse_length(t, &num, &u) && u != NS_CSS_UNIT_NUMBER) {
                    g_free(basis);
                    basis = g_strdup(t);
                    basis_set = TRUE;
                    continue;
                }
                if (ns_css_parse_length(t, &num, &u) && u == NS_CSS_UNIT_NUMBER) {
                    if (numerics == 0)      grow = num;
                    else if (numerics == 1) shrink = num;
                    else if (numerics == 2) {
                        g_free(basis);
                        basis = g_strdup_printf("%g", num);
                        basis_set = TRUE;
                    }
                    numerics++;
                }
            }
            if (numerics >= 1 && !basis_set) {
                basis = g_strdup("0%");
                basis_set = TRUE;
            }
            if (numerics == 0 && basis_set && !keyword_set) grow = 1;
            char grow_buf[32];
            g_snprintf(grow_buf, sizeof grow_buf, "%g", grow);
            char shrink_buf[32];
            g_snprintf(shrink_buf, sizeof shrink_buf, "%g", shrink);
            ns_css_value *gv = ns_css_parse_value_for(NS_CSS_FLEX_GROW, grow_buf);
            if (gv) {
                ns_css_decl d = { .prop = NS_CSS_FLEX_GROW, .value = gv, .important = important };
                g_array_append_val(decls_out, d);
            }
            ns_css_value *sv = ns_css_parse_value_for(NS_CSS_FLEX_SHRINK, shrink_buf);
            if (sv) {
                ns_css_decl d = { .prop = NS_CSS_FLEX_SHRINK, .value = sv, .important = important };
                g_array_append_val(decls_out, d);
            }
            if (basis_set) {
                ns_css_value *bv = ns_css_parse_value_for(NS_CSS_FLEX_BASIS, basis);
                if (bv) {
                    ns_css_decl d = { .prop = NS_CSS_FLEX_BASIS, .value = bv, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            g_free(basis);
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "flex-flow") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            for (int i = 0; i < n; i++) {
                char *t = tokens[i];
                if (g_ascii_strcasecmp(t, "row") == 0 ||
                    g_ascii_strcasecmp(t, "row-reverse") == 0 ||
                    g_ascii_strcasecmp(t, "column") == 0 ||
                    g_ascii_strcasecmp(t, "column-reverse") == 0) {
                    ns_css_value *v = ns_css_parse_value_for(NS_CSS_FLEX_DIRECTION, t);
                    if (v) {
                        ns_css_decl d = { .prop = NS_CSS_FLEX_DIRECTION, .value = v, .important = important };
                        g_array_append_val(decls_out, d);
                    }
                } else if (g_ascii_strcasecmp(t, "wrap") == 0 ||
                           g_ascii_strcasecmp(t, "nowrap") == 0 ||
                           g_ascii_strcasecmp(t, "wrap-reverse") == 0) {
                    ns_css_value *v = ns_css_parse_value_for(NS_CSS_FLEX_WRAP, t);
                    if (v) {
                        ns_css_decl d = { .prop = NS_CSS_FLEX_WRAP, .value = v, .important = important };
                        g_array_append_val(decls_out, d);
                    }
                }
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "list-style") == 0) {
            char *type = NULL, *position = NULL, *image = NULL;
            if (list_style_split(vtext, &type, &position, &image)) {
                const struct { ns_css_prop prop; const char *text; } parts[] = {
                    { NS_CSS_LIST_STYLE_TYPE, type },
                    { NS_CSS_LIST_STYLE_POSITION, position },
                    { NS_CSS_LIST_STYLE_IMAGE, image },
                };
                for (gsize k = 0; k < G_N_ELEMENTS(parts); k++) {
                    ns_css_value *v = ns_css_parse_value_for(parts[k].prop, parts[k].text);
                    if (!v) continue;
                    ns_css_decl d = { .prop = parts[k].prop, .value = v, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            g_free(type);
            g_free(position);
            g_free(image);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "border-radius") == 0 && !strstr(vtext, "var(")) {
            ns_css_value *wide = parse_css_wide_keyword(vtext);
            char *radius_canon = wide ? g_strdup(vtext)
                                      : border_radius_canonical(vtext);
            ns_css_value_free(wide);
            if (!radius_canon) {
                g_free(pname);
                g_free(vtext);
                if (p < end && *p == ';') p++;
                continue;
            }
            g_free(radius_canon);
        }
        if (strcmp(pname, "border-radius") == 0) {
            char *vtext_main = vtext;
            char *slash = strchr(vtext_main, '/');
            if (slash) *slash = '\0';
            char *tokens[4] = {0};
            char *vtokens[4] = {0};
            int n = split_ws(vtext_main, tokens);
            int nv = slash ? split_ws(slash + 1, vtokens) : 0;
            if (n > 0) {
                const char *h[4] = {
                    tokens[0],
                    n >= 2 ? tokens[1] : tokens[0],
                    n >= 3 ? tokens[2] : tokens[0],
                    n >= 4 ? tokens[3] : (n >= 2 ? tokens[1] : tokens[0]),
                };
                const char *vert[4] = {
                    nv >= 1 ? vtokens[0] : NULL,
                    nv >= 2 ? vtokens[1] : (nv >= 1 ? vtokens[0] : NULL),
                    nv >= 3 ? vtokens[2] : (nv >= 1 ? vtokens[0] : NULL),
                    nv >= 4 ? vtokens[3]
                            : (nv >= 2 ? vtokens[1] : (nv >= 1 ? vtokens[0] : NULL)),
                };
                static const ns_css_prop corners[4] = {
                    NS_CSS_BORDER_TOP_LEFT_RADIUS, NS_CSS_BORDER_TOP_RIGHT_RADIUS,
                    NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS, NS_CSS_BORDER_BOTTOM_LEFT_RADIUS,
                };
                for (int i = 0; i < 4; i++) {
                    char *text = vert[i] ? g_strdup_printf("%s %s", h[i], vert[i])
                                         : g_strdup(h[i]);
                    ns_css_value *vv = ns_css_parse_value_for(corners[i], text);
                    g_free(text);
                    if (!vv) continue;
                    ns_css_decl d = { .prop = corners[i], .value = vv, .important = important };
                    g_array_append_val(decls_out, d);
                }
                ns_css_value *legacy = ns_css_parse_value_for(NS_CSS_BORDER_RADIUS, h[0]);
                if (legacy) {
                    ns_css_decl d = { .prop = NS_CSS_BORDER_RADIUS, .value = legacy, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            for (int i = 0; i < nv; i++) g_free(vtokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "margin-block") == 0 ||
            strcmp(pname, "margin-inline") == 0 ||
            strcmp(pname, "padding-block") == 0 ||
            strcmp(pname, "padding-inline") == 0 ||
            strcmp(pname, "inset-block") == 0 ||
            strcmp(pname, "inset-inline") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            if (n > 2) {
                for (int i = 2; i < n; i++) g_free(tokens[i]);
                n = 2;
            }
            if (n > 0) {
                const char *a = tokens[0];
                const char *b = n >= 2 ? tokens[1] : a;
                ns_css_prop pa = NS_CSS_MARGIN_TOP, pb = NS_CSS_MARGIN_BOTTOM;
                if (strcmp(pname, "margin-block") == 0) {
                    pa = NS_CSS_MARGIN_TOP; pb = NS_CSS_MARGIN_BOTTOM;
                } else if (strcmp(pname, "margin-inline") == 0) {
                    pa = NS_CSS_MARGIN_LEFT; pb = NS_CSS_MARGIN_RIGHT;
                } else if (strcmp(pname, "padding-block") == 0) {
                    pa = NS_CSS_PADDING_TOP; pb = NS_CSS_PADDING_BOTTOM;
                } else if (strcmp(pname, "padding-inline") == 0) {
                    pa = NS_CSS_PADDING_LEFT; pb = NS_CSS_PADDING_RIGHT;
                } else if (strcmp(pname, "inset-block") == 0) {
                    pa = NS_CSS_TOP; pb = NS_CSS_BOTTOM;
                } else {
                    pa = NS_CSS_LEFT; pb = NS_CSS_RIGHT;
                }
                ns_css_value *va = ns_css_parse_value_for(pa, a);
                ns_css_value *vb = ns_css_parse_value_for(pb, b);
                if (va) {
                    ns_css_decl d = { .prop = pa, .value = va, .important = important };
                    g_array_append_val(decls_out, d);
                }
                if (vb) {
                    ns_css_decl d = { .prop = pb, .value = vb, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "inset") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            if (n > 0) {
                emit_quad(decls_out,
                    NS_CSS_TOP, NS_CSS_RIGHT,
                    NS_CSS_BOTTOM, NS_CSS_LEFT,
                    tokens, n, important);
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "text-wrap") == 0 ||
            strcmp(pname, "text-wrap-mode") == 0) {
            const char *mapped = NULL;
            char *kw = g_ascii_strdown(vtext, -1);
            g_strstrip(kw);
            if (strcmp(kw, "nowrap") == 0)
                mapped = "nowrap";
            else if (strcmp(kw, "wrap") == 0 ||
                     strcmp(kw, "balance") == 0 ||
                     strcmp(kw, "pretty") == 0 ||
                     strcmp(kw, "stable") == 0)
                mapped = "normal";
            if (mapped) {
                ns_css_value *vv = ns_css_parse_value_for(NS_CSS_WHITE_SPACE, mapped);
                if (vv) {
                    ns_css_decl d = { .prop = NS_CSS_WHITE_SPACE, .value = vv, .important = important };
                    g_array_append_val(decls_out, d);
                }
            }
            g_free(kw);
            g_free(pname);
            g_free(vtext);
            if (p < end && *p == ';') p++;
            continue;
        }

        if (strcmp(pname, "margin") == 0 ||
            strcmp(pname, "padding") == 0 ||
            strcmp(pname, "scroll-margin") == 0 ||
            strcmp(pname, "scroll-padding") == 0 ||
            strcmp(pname, "border-width") == 0 ||
            strcmp(pname, "border-color") == 0 ||
            strcmp(pname, "border-style") == 0) {
            char *tokens[4] = {0};
            int n = split_ws(vtext, tokens);
            if (n > 0) {
                if (strcmp(pname, "margin") == 0)
                    emit_quad(decls_out,
                        NS_CSS_MARGIN_TOP, NS_CSS_MARGIN_RIGHT,
                        NS_CSS_MARGIN_BOTTOM, NS_CSS_MARGIN_LEFT,
                        tokens, n, important);
                else if (strcmp(pname, "padding") == 0)
                    emit_quad(decls_out,
                        NS_CSS_PADDING_TOP, NS_CSS_PADDING_RIGHT,
                        NS_CSS_PADDING_BOTTOM, NS_CSS_PADDING_LEFT,
                        tokens, n, important);
                else if (strcmp(pname, "scroll-margin") == 0)
                    emit_quad(decls_out,
                        NS_CSS_SCROLL_MARGIN_TOP, NS_CSS_SCROLL_MARGIN_RIGHT,
                        NS_CSS_SCROLL_MARGIN_BOTTOM, NS_CSS_SCROLL_MARGIN_LEFT,
                        tokens, n, important);
                else if (strcmp(pname, "scroll-padding") == 0)
                    emit_quad(decls_out,
                        NS_CSS_SCROLL_PADDING_TOP, NS_CSS_SCROLL_PADDING_RIGHT,
                        NS_CSS_SCROLL_PADDING_BOTTOM, NS_CSS_SCROLL_PADDING_LEFT,
                        tokens, n, important);
                else if (strcmp(pname, "border-width") == 0)
                    emit_quad(decls_out,
                        NS_CSS_BORDER_TOP_WIDTH, NS_CSS_BORDER_RIGHT_WIDTH,
                        NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_LEFT_WIDTH,
                        tokens, n, important);
                else if (strcmp(pname, "border-color") == 0)
                    emit_quad(decls_out,
                        NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
                        NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
                        tokens, n, important);
                else
                    emit_quad(decls_out,
                        NS_CSS_BORDER_TOP_STYLE, NS_CSS_BORDER_RIGHT_STYLE,
                        NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_BORDER_LEFT_STYLE,
                        tokens, n, important);
            }
            for (int i = 0; i < n; i++) g_free(tokens[i]);
        } else if (strcmp(pname, "container") == 0) {
            char *slash = strchr(vtext, '/');
            char *name_part = slash ? g_strndup(vtext, (gsize)(slash - vtext))
                                    : g_strdup(vtext);
            g_strstrip(name_part);
            gboolean type_ok = TRUE;
            if (slash) {
                char *type_part = g_strstrip(g_strdup(slash + 1));
                ns_css_value *tv = *type_part
                    ? ns_css_parse_value_for(NS_CSS_CONTAINER_TYPE, type_part) : NULL;
                type_ok = tv != NULL;
                ns_css_value_free(tv);
                g_free(type_part);
            }
            ns_css_value *nv = type_ok
                ? ns_css_parse_value_for(NS_CSS_CONTAINER_NAME, name_part) : NULL;
            if (nv && !slash) {
                ns_css_value *tv = ns_css_parse_value_for(NS_CSS_CONTAINER_TYPE, "normal");
                ns_css_decl d = { .prop = NS_CSS_CONTAINER_TYPE, .value = tv,
                                  .important = important };
                g_array_append_val(decls_out, d);
            }
            if (nv) {
                ns_css_decl d = { .prop = NS_CSS_CONTAINER_NAME, .value = nv,
                                  .important = important };
                g_array_append_val(decls_out, d);
            }
            g_free(name_part);
            if (slash && nv) {
                char *type_part = g_strstrip(g_strdup(slash + 1));
                ns_css_value *tv = *type_part
                    ? ns_css_parse_value_for(NS_CSS_CONTAINER_TYPE, type_part) : NULL;
                if (tv) {
                    ns_css_decl d = { .prop = NS_CSS_CONTAINER_TYPE, .value = tv,
                                      .important = important };
                    g_array_append_val(decls_out, d);
                }
                g_free(type_part);
            }
        } else if (strcmp(pname, "animation-range") == 0) {
            char *st = NULL, *en = NULL;
            if (ns_css_anim_range_shorthand_expand(vtext, &st, &en)) {
                ns_css_value *sv = ns_css_parse_value_for(NS_CSS_ANIMATION_RANGE_START, st);
                ns_css_value *ev = ns_css_parse_value_for(NS_CSS_ANIMATION_RANGE_END, en);
                if (sv) {
                    ns_css_decl d = { .prop = NS_CSS_ANIMATION_RANGE_START, .value = sv,
                                      .important = important };
                    g_array_append_val(decls_out, d);
                }
                if (ev) {
                    ns_css_decl d = { .prop = NS_CSS_ANIMATION_RANGE_END, .value = ev,
                                      .important = important };
                    g_array_append_val(decls_out, d);
                }
                g_free(st);
                g_free(en);
            }
        } else {
            int pid = prop_id(pname);
            if (pid >= 0) {
                ns_css_value *vv = ns_css_parse_value_for((ns_css_prop)pid, vtext);
                if (vv) {
                    ns_css_decl d = { .prop = (ns_css_prop)pid, .value = vv, .important = important };
                    g_array_append_val(decls_out, d);
                    if (pid == NS_CSS_ANIMATION || pid == NS_CSS_TRANSITION)
                        anim_shorthand_emit_longhands(decls_out, vv,
                                                      pid == NS_CSS_ANIMATION,
                                                      important);
                }
            }
        }
        g_free(pname);
        g_free(vtext);
        if (p < end && *p == ';') p++;
    }
    if (p < end && *p == '}') p++;
    *pp = p;
}

static void
pending_decl_clear(gpointer data)
{
    ns_css_pending_decl *pd = data;
    g_free(pd->pname);
    g_free(pd->raw_vtext);
}

static void
ns_css_scope_free(ns_css_scope *s)
{
    if (!s) return;
    if (s->roots) g_ptr_array_free(s->roots, TRUE);
    if (s->limits) g_ptr_array_free(s->limits, TRUE);
    g_free(s);
}


static void
ns_css_rule_free(ns_css_rule *r)
{
    if (!r) return;
    for (guint i = 0; i < r->selectors->len; i++)
        ns_css_selector_free(g_ptr_array_index(r->selectors, i));
    g_ptr_array_free(r->selectors, TRUE);
    for (guint i = 0; i < r->decls->len; i++) {
        ns_css_decl *d = &g_array_index(r->decls, ns_css_decl, i);
        ns_css_value_free(d->value);
    }
    g_array_free(r->decls, TRUE);
    if (r->vars) g_hash_table_destroy(r->vars);
    if (r->var_important) g_hash_table_destroy(r->var_important);
    if (r->pending) g_array_free(r->pending, TRUE);
    g_free(r->layer_name);
    g_free(r->container_condition);
    ns_css_container_query_free(r->container_query);
    if (r->scopes) g_ptr_array_free(r->scopes, TRUE);
    g_free(r);
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
css_scan_segment(const char *p, const char *end, char *terminator)
{
    return css_scan_until(p, end, "{;}", terminator);
}

static const char *
css_scan_declaration_value(const char *p, const char *end, char *terminator)
{
    return css_scan_until(p, end, ";}", terminator);
}

static gboolean
css_declaration_value_syntax_valid(const char *text)
{
    char *value = g_strdup(text ? text : "");
    gboolean important = FALSE;
    css_strip_important(value, &important);
    const char *p = value;
    const char *end = value + strlen(value);
    char quote = 0;
    int paren = 0, bracket = 0, brace = 0;
    gboolean valid = TRUE;
    while (p < end && valid) {
        char c = *p;
        if (quote) {
            if (c == '\\' && p + 1 < end) {
                p += 2;
                continue;
            }
            if (c == quote) quote = 0;
            else if (c == '\n' || c == '\r' || c == '\f') valid = FALSE;
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
        if (c == '(') paren++;
        else if (c == ')') { if (paren == 0) valid = FALSE; else paren--; }
        else if (c == '[') bracket++;
        else if (c == ']') { if (bracket == 0) valid = FALSE; else bracket--; }
        else if (c == '{') brace++;
        else if (c == '}') { if (brace == 0) valid = FALSE; else brace--; }
        else if (c == '!' && paren == 0 && bracket == 0 && brace == 0)
            valid = FALSE;
        else if (c == ';' && paren == 0 && bracket == 0 && brace == 0)
            valid = FALSE;
        p++;
    }
    valid = valid && quote == 0 && paren == 0 && bracket == 0 && brace == 0;
    g_free(value);
    return valid;
}

gboolean
ns_css_named_property_supported(const char *name)
{
    static const char *const cssom_properties[] = {
        "alignment-baseline", "background-attachment", "baseline-shift",
        "baseline-source", "background", "border", "column-rule",
        "columns", "empty-cells", "flex", "flex-flow", "font", "grid",
        "grid-template", "list-style", "outline", "page-break-after",
        "page-break-before", "page-break-inside", "place-content",
        "place-items", "place-self", "src", "unicode-range",
        "animation-range",
    };
    if (!name || !*name) return FALSE;
    if (name[0] == '-' && name[1] == '-' && name[2]) return TRUE;
    if (g_ascii_strcasecmp(name, "all") == 0 || prop_id(name) >= 0 ||
        g_ascii_strcasecmp(name, "unicode-range") == 0)
        return TRUE;
    for (gsize i = 0; i < G_N_ELEMENTS(cssom_properties); i++)
        if (g_ascii_strcasecmp(name, cssom_properties[i]) == 0)
            return TRUE;
    char *declaration = g_strdup_printf("%s: initial;", name);
    const char *p = declaration;
    const char *end = declaration + strlen(declaration);
    GArray *decls = g_array_new(FALSE, FALSE, sizeof(ns_css_decl));
    parse_declaration_block(&p, end, decls, NULL);
    gboolean supported = decls->len > 0;
    for (guint i = 0; i < decls->len; i++) {
        ns_css_decl *decl = &g_array_index(decls, ns_css_decl, i);
        ns_css_value_free(decl->value);
    }
    g_array_free(decls, TRUE);
    g_free(declaration);
    return supported;
}

static const char *
css_skip_to_block_end(const char *p, const char *end)
{
    int depth = 0;
    char quote = 0;
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
        if (c == '{') depth++;
        else if (c == '}') {
            depth--;
            if (depth <= 0) return p + 1;
        }
        p++;
    }
    return end;
}

static const char *
css_block_body_end(const char *body_start, const char *block_end)
{
    return block_end > body_start && block_end[-1] == '}' ? block_end - 1
                                                          : block_end;
}

static const char *
css_find_top_level_char(const char *p, const char *end, char needle)
{
    char terms[2] = { needle, 0 };
    char term = 0;
    const char *q = css_scan_until(p, end, terms, &term);
    return term == needle ? q : NULL;
}

static const char *
css_find_function(const char *p, const char *end, const char *name)
{
    gsize n = strlen(name);
    const char *start = p;
    char quote = 0;
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
        if ((gsize)(end - p) > n && p[n] == '(' &&
            g_ascii_strncasecmp(p, name, n) == 0 &&
            (p == start || !is_ident(p[-1])))
            return p;
        p++;
    }
    return NULL;
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

static int
font_face_weight_descriptor(const char *val)
{
    if (g_ascii_strcasecmp(val, "normal") == 0) return 400;
    if (g_ascii_strcasecmp(val, "bold") == 0) return 700;
    char *end = NULL;
    double weight = g_ascii_strtod(val, &end);
    if (end == val || *end != '\0' || weight < 1 || weight > 1000) return 0;
    return (int)(weight + 0.5);
}

static ns_font_slant
font_face_style_descriptor(const char *val)
{
    if (g_ascii_strcasecmp(val, "normal") == 0) return NS_FONT_SLANT_ROMAN;
    if (g_ascii_strcasecmp(val, "italic") == 0) return NS_FONT_SLANT_ITALIC;
    if (g_ascii_strncasecmp(val, "oblique", 7) == 0) return NS_FONT_SLANT_OBLIQUE;
    return NS_FONT_SLANT_AUTO;
}

static void
font_face_clear(gpointer data)
{
    ns_css_font_face *ff = data;
    g_free(ff->family);
    g_free(ff->src_url);
    g_free(ff->unicode_range);
}

static void
property_rule_clear(gpointer data)
{
    ns_css_property_rule *pr = data;
    g_free(pr->name);
    g_free(pr->initial_value);
    g_free(pr->syntax_text);
    ns_css_syntax_def_free(pr->syntax);
}

static void
css_property_rule_free(gpointer data)
{
    property_rule_clear(data);
    g_free(data);
}

static char *
css_string_descriptor_dup(const char *value)
{
    if (!value) return NULL;
    gboolean valid = FALSE;
    GPtrArray *items = ns_css_component_values_parse(value, -1, &valid);
    char *out = NULL;
    if (valid) {
        const ns_css_component *only = NULL;
        for (guint i = 0; i < items->len; i++) {
            const ns_css_component *c = g_ptr_array_index(items, i);
            if (c->type == NS_CSS_COMPONENT_WHITESPACE) continue;
            if (only) { only = NULL; break; }
            only = c;
        }
        if (only && only->type == NS_CSS_COMPONENT_STRING)
            out = g_strdup(only->value ? only->value : "");
    }
    g_ptr_array_free(items, TRUE);
    return out;
}

static gboolean
font_url_suffix_eq(const char *url, const char *end, const char *suffix)
{
    gsize n = strlen(suffix);
    return (gsize)(end - url) >= n &&
           g_ascii_strncasecmp(end - n, suffix, n) == 0;
}

static int
font_src_score(const char *url)
{
    if (!url || !*url) return -1;
    if (g_str_has_prefix(url, "data:")) {
        if (strstr(url, "font/woff2")) return 80;
        if (strstr(url, "font/woff"))  return 70;
        if (strstr(url, "font/"))      return 40;
        return 20;
    }
    const char *end = url + strlen(url);
    const char *q = strchr(url, '?');
    const char *h = strchr(url, '#');
    if (q && q < end) end = q;
    if (h && h < end) end = h;
    if (font_url_suffix_eq(url, end, ".woff2")) return 80;
    if (font_url_suffix_eq(url, end, ".woff"))  return 70;
    if (font_url_suffix_eq(url, end, ".otf"))   return 60;
    if (font_url_suffix_eq(url, end, ".ttf"))   return 60;
    if (font_url_suffix_eq(url, end, ".ttc"))   return 60;
    if (font_url_suffix_eq(url, end, ".eot"))   return -1;
    if (font_url_suffix_eq(url, end, ".svg"))   return -1;
    return 10;
}

static void
font_src_consider(char **best, const char *start, gsize len)
{
    if (!best || !start || len == 0) return;
    char *candidate = g_strndup(start, len);
    int score = font_src_score(candidate);
    if (score < 0) {
        g_free(candidate);
        return;
    }
    int old_score = *best ? font_src_score(*best) : -1;
    if (!*best || score > old_score) {
        g_free(*best);
        *best = candidate;
    } else {
        g_free(candidate);
    }
}

static void
font_src_consider_urls(char **best, const char *value)
{
    const char *p = value;
    const char *end = value + strlen(value);
    while (p < end) {
        if (p + 4 <= end && g_ascii_strncasecmp(p, "url(", 4) == 0) {
            p += 4;
            p = css_skip_ws_comments(p, end);
            char quote = 0;
            if (p < end && (*p == '"' || *p == '\'')) {
                quote = *p;
                p++;
            }
            const char *start = p;
            if (quote) {
                while (p < end) {
                    if (*p == '\\' && p + 1 < end) p += 2;
                    else if (*p == quote) break;
                    else p++;
                }
            } else {
                while (p < end && *p != ')' && !is_ws(*p)) {
                    if (*p == '\\' && p + 1 < end) p += 2;
                    else p++;
                }
            }
            if (p > start) font_src_consider(best, start, (gsize)(p - start));
            while (p < end && *p != ')') p++;
            if (p < end) p++;
            continue;
        }
        if ((*p == '"' || *p == '\'')) {
            char q = *p++;
            while (p < end) {
                if (*p == '\\' && p + 1 < end) p += 2;
                else if (*p++ == q) break;
            }
        } else if (p + 1 < end && p[0] == '/' && p[1] == '*') {
            p = css_skip_comment(p, end);
        } else {
            p++;
        }
    }
}

static char *
css_keyframes_name_from_range(const char *start, const char *end)
{
    char *name = css_trim_dup_range(start, end);
    if (!name || !*name) return name;
    gsize n = strlen(name);
    if (n >= 2 && (name[0] == '"' || name[0] == '\'') && name[n - 1] == name[0]) {
        GString *out = g_string_new(NULL);
        for (gsize i = 1; i + 1 < n; i++) {
            if (name[i] == '\\' && i + 1 < n - 1) i++;
            g_string_append_c(out, name[i]);
        }
        g_free(name);
        name = g_string_free(out, FALSE);
    }
    return name;
}

static void
keyframes_clear(gpointer data)
{
    ns_css_keyframes *kf = data;
    g_free(kf->name);
    for (int i = 0; i < kf->n_stops; i++)
        g_free(kf->stops[i].raw_props);
    g_free(kf->stops);
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
        char *resolved = substitute_vars_with(rawp, vars, 0);
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

static int
keyframe_stop_cmp(gconstpointer a, gconstpointer b)
{
    double da = ((const ns_css_keyframe_stop *)a)->pct;
    double db = ((const ns_css_keyframe_stop *)b)->pct;
    if (da < db) return -1;
    if (da > db) return  1;
    return 0;
}

static gboolean
parse_keyframe_stop_pct(const char *sel, double *out_pct)
{
    while (*sel == ' ') sel++;
    if (g_ascii_strcasecmp(sel, "from") == 0) { *out_pct = 0;   return TRUE; }
    if (g_ascii_strcasecmp(sel, "to")   == 0) { *out_pct = 100; return TRUE; }
    char *end = NULL;
    double v = g_ascii_strtod(sel, &end);
    if (end == sel) return FALSE;
    while (*end == ' ') end++;
    if (*end != '%' && *end != '\0') return FALSE;
    *out_pct = v;
    return TRUE;
}

static void
skip_at_rule(const char **pp, const char *end)
{
    const char *p = *pp;
    char term = 0;
    const char *seg = css_scan_segment(p, end, &term);
    if (term == ';') *pp = seg + 1;
    else if (term == '{') *pp = css_skip_to_block_end(seg, end);
    else *pp = seg;
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

static gboolean
sizes_is_length_fn(const char *p)
{
    return g_ascii_strncasecmp(p, "calc(", 5) == 0 ||
           g_ascii_strncasecmp(p, "min(", 4) == 0 ||
           g_ascii_strncasecmp(p, "max(", 4) == 0 ||
           g_ascii_strncasecmp(p, "clamp(", 6) == 0;
}

static double
sizes_length_px(const char *len, gsize len_n)
{
    double px = 0, pct = 0;
    if (!ns_css_resolve_to_px_pct(len, len_n, &px, &pct)) return -1;
    return px + pct * 0.01 * g_viewport_w;
}

double
ns_css_sizes_resolve(const char *sizes)
{
    if (!sizes || !*sizes) return g_viewport_w;
    const char *p = sizes;
    const char *end = sizes + strlen(sizes);
    while (p < end) {
        while (p < end && (is_ws(*p) || *p == ',')) p++;
        if (p >= end) break;
        const char *entry = p;
        while (p < end && *p != ',') {
            if (*p == '(') {
                const char *cp = match_close_paren(p + 1, end);
                p = cp ? cp + 1 : end;
            } else {
                p++;
            }
        }
        const char *entry_end = p;
        const char *q = entry;
        const char *len_start = NULL;
        while (q < entry_end) {
            while (q < entry_end && is_ws(*q)) q++;
            if (q >= entry_end) break;
            if (sizes_is_length_fn(q) || g_ascii_isdigit(*q) ||
                *q == '.' || *q == '+' || *q == '-') {
                len_start = q;
                break;
            }
            if (*q == '(') {
                const char *cp = match_close_paren(q + 1, entry_end);
                q = cp ? cp + 1 : entry_end;
            } else {
                while (q < entry_end && !is_ws(*q)) q++;
            }
        }
        if (!len_start) continue;
        char *cond = g_strndup(entry, (gsize)(len_start - entry));
        g_strstrip(cond);
        gboolean cond_ok = (*cond == '\0') || ns_css_media_query_matches(cond);
        g_free(cond);
        if (!cond_ok) continue;
        double px = sizes_length_px(len_start, (gsize)(entry_end - len_start));
        if (px > 0) return px;
    }
    return g_viewport_w;
}

#define NS_CSS_LAYER_NONE INT_MAX

static gboolean supports_expr(const char **pp, const char *end, int depth);

static __thread int g_supports_parse_depth;

gboolean
ns_css_supports_declaration(const char *property, const char *value)
{
    if (!property || !value || g_supports_parse_depth >= NS_CSS_MAX_AT_NESTING)
        return FALSE;
    char *property_copy = g_strdup(property);
    char *value_copy = g_strdup(value);
    property = g_strstrip(property_copy);
    value = g_strstrip(value_copy);
    const char *property_end = property + strlen(property);
    const char *property_scan = property;
    char *property_name = ns_css_read_ident(&property_scan, property_end);
    gboolean property_valid = property_name && *property_name &&
                              property_scan == property_end;
    g_free(property_name);
    gboolean empty_custom = property[0] == '-' && property[1] == '-' &&
                            property[2] != '\0';
    char term = 0;
    const char *value_end = value + strlen(value);
    const char *value_scan = css_scan_declaration_value(value, value_end, &term);
    if (!*property || (!*value && !empty_custom) || !property_valid ||
        value_scan != value_end) {
        g_free(property_copy);
        g_free(value_copy);
        return FALSE;
    }
    char *css = g_strdup_printf("x{%s:%s}", property, value);
    g_supports_parse_depth++;
    ns_css_stylesheet *sh = ns_css_stylesheet_parse(css, -1);
    g_supports_parse_depth--;
    g_free(css);
    gboolean ok = FALSE;
    if (sh && sh->rules && sh->rules->len > 0) {
        ns_css_rule *r = g_ptr_array_index(sh->rules, 0);
        if (r && ((r->decls && r->decls->len > 0) ||
                  (r->vars && g_hash_table_size(r->vars) > 0) ||
                  (r->pending && r->pending->len > 0)))
            ok = TRUE;
    }
    if (sh) ns_css_stylesheet_free(sh);
    g_free(property_copy);
    g_free(value_copy);
    return ok;
}

static gboolean
supports_feature_matches(const char *src, gsize len)
{
    char *s = g_strndup(src, len);
    g_strstrip(s);
    char *colon = (char *)css_find_top_level_char(s, s + strlen(s), ':');
    if (!colon) { g_free(s); return FALSE; }
    *colon = '\0';
    gboolean ok = ns_css_supports_declaration(g_strstrip(s),
                                               g_strstrip(colon + 1));
    g_free(s);
    return ok;
}

static gboolean supports_selector_supported(const ns_css_selector *sel);

static gboolean
supports_simple_supported(const ns_css_simple *c)
{
    if (c->never_match) return FALSE;
    GPtrArray *groups[3] = { c->matches_any, c->matches_none, c->has_groups };
    for (int g = 0; g < 3; g++) {
        if (!groups[g]) continue;
        for (guint i = 0; i < groups[g]->len; i++) {
            const GPtrArray *grp = g_ptr_array_index(groups[g], i);
            for (guint j = 0; j < grp->len; j++)
                if (!supports_selector_supported(g_ptr_array_index(grp, j)))
                    return FALSE;
        }
    }
    return TRUE;
}

static gboolean
supports_selector_supported(const ns_css_selector *sel)
{
    if (!sel || !sel->compounds || sel->compounds->len == 0) return FALSE;
    for (guint i = 0; i < sel->compounds->len; i++)
        if (!supports_simple_supported(g_ptr_array_index(sel->compounds, i)))
            return FALSE;
    return TRUE;
}

static gboolean
supports_selector_matches(const char *src, gsize len)
{
    char *s = g_strndup(src, len);
    gboolean saved_strict = g_sel_strict;
    g_sel_strict = TRUE;
    gboolean valid = FALSE;
    GPtrArray *list = ns_css_parse_selector_list_checked(s, &valid);
    g_sel_strict = saved_strict;
    g_free(s);
    gboolean ok = valid && list->len == 1;
    for (guint i = 0; ok && i < list->len; i++)
        if (!supports_selector_supported(g_ptr_array_index(list, i)))
            ok = FALSE;
    g_ptr_array_free(list, TRUE);
    return ok;
}

static gboolean
match_kw(const char *p, const char *end, const char *kw)
{
    gsize n = strlen(kw);
    if ((gsize)(end - p) < n) return FALSE;
    if (g_ascii_strncasecmp(p, kw, n) != 0) return FALSE;
    if (p + n == end) return TRUE;
    char c = p[n];
    return is_ws(c) || (c == '/' && p + n + 1 < end && p[n + 1] == '*');
}

static gboolean
supports_function_start(const char *p, const char *end)
{
    const char *q = p;
    char *name = ns_css_read_ident(&q, end);
    gboolean result = name && *name && q < end && *q == '(';
    g_free(name);
    return result;
}

static gboolean
supports_term(const char **pp, const char *end, int depth)
{
    if (depth > NS_CSS_MAX_AT_NESTING) { *pp = end; return FALSE; }
    const char *p = *pp;
    p = css_skip_ws_comments(p, end);
    gboolean negate = FALSE;
    if (match_kw(p, end, "not")) {
        negate = TRUE;
        p += 3;
        p = css_skip_ws_comments(p, end);
    }
    if ((gsize)(end - p) > 9 && g_ascii_strncasecmp(p, "selector(", 9) == 0) {
        p += 9;
        const char *sel_start = p;
        char term = 0;
        const char *sel_end = css_scan_until(p, end, ")", &term);
        gsize sel_len = (gsize)(sel_end - sel_start);
        p = term == ')' ? sel_end + 1 : sel_end;
        gboolean result = supports_selector_matches(sel_start, sel_len);
        if (negate) result = !result;
        *pp = p;
        return result;
    }
    if (supports_function_start(p, end)) {
        const char *q = p;
        char *name = ns_css_read_ident(&q, end);
        g_free(name);
        q++;
        char term = 0;
        const char *close = css_scan_until(q, end, ")", &term);
        p = term == ')' ? close + 1 : close;
        gboolean result = FALSE;
        if (negate && term == ')') result = TRUE;
        *pp = p;
        return result;
    }
    if (p >= end || *p != '(') { *pp = p; return FALSE; }
    p++;
    p = css_skip_ws_comments(p, end);
    gboolean is_nested = (p < end && *p == '(') ||
                         match_kw(p, end, "not") ||
                         supports_function_start(p, end);
    gboolean result;
    if (is_nested) {
        result = supports_expr(&p, end, depth + 1);
        p = css_skip_ws_comments(p, end);
    } else {
        const char *fstart = p;
        char term = 0;
        const char *fend = css_scan_until(p, end, ")", &term);
        gsize flen = (gsize)(fend - fstart);
        p = fend;
        result = supports_feature_matches(fstart, flen);
    }
    if (p >= end || *p != ')') { *pp = p; return FALSE; }
    p++;
    if (negate) result = !result;
    *pp = p;
    return result;
}

static gboolean
supports_expr(const char **pp, const char *end, int depth)
{
    gboolean acc = supports_term(pp, end, depth);
    const char *p = *pp;
    int op = 0;
    while (1) {
        p = css_skip_ws_comments(p, end);
        if (match_kw(p, end, "and")) {
            if (op == 2) { *pp = p; return FALSE; }
            op = 1;
            p += 3;
            *pp = p;
            gboolean rhs = supports_term(pp, end, depth);
            p = *pp;
            acc = acc && rhs;
        } else if (match_kw(p, end, "or")) {
            if (op == 1) { *pp = p; return FALSE; }
            op = 2;
            p += 2;
            *pp = p;
            gboolean rhs = supports_term(pp, end, depth);
            p = *pp;
            acc = acc || rhs;
        } else {
            break;
        }
    }
    *pp = p;
    return acc;
}

gboolean
ns_css_supports_condition(const char *condition,
                          gboolean allow_bare_declaration)
{
    if (!condition) return FALSE;
    char *copy = g_strdup(condition);
    char *query = g_strstrip(copy);
    const char *end = query + strlen(query);
    if (allow_bare_declaration) {
        const char *colon = css_find_top_level_char(query, end, ':');
        if (colon) {
            char *property = g_strndup(query, (gsize)(colon - query));
            char *value = g_strdup(colon + 1);
            gboolean result = ns_css_supports_declaration(g_strstrip(property),
                                                           g_strstrip(value));
            g_free(property);
            g_free(value);
            g_free(copy);
            return result;
        }
    }
    const char *p = query;
    gboolean result = supports_expr(&p, end, 0);
    p = css_skip_ws_comments(p, end);
    result = result && p == end;
    g_free(copy);
    return result;
}

static __thread GHashTable *g_var_adjust_cache;

static char *
css_trim_dup_range(const char *start, const char *end)
{
    while (start < end && is_ws(*start)) start++;
    while (end > start && is_ws(end[-1])) end--;
    return g_strndup(start, (gsize)(end - start));
}

static void
css_stylesheet_ensure_layers(ns_css_stylesheet *sh)
{
    if (!sh->layer_names)
        sh->layer_names = g_ptr_array_new_with_free_func(g_free);
    if (!sh->layers)
        sh->layers = g_hash_table_new(g_str_hash, g_str_equal);
}

static int
css_layer_register(ns_css_stylesheet *sh, const char *name)
{
    if (!sh || !name || !*name) return NS_CSS_LAYER_NONE;
    css_stylesheet_ensure_layers(sh);
    gpointer existing = g_hash_table_lookup(sh->layers, name);
    if (existing) return GPOINTER_TO_INT(existing) - 1;
    int rank = (int)sh->layer_names->len;
    char *owned = g_strdup(name);
    g_ptr_array_add(sh->layer_names, owned);
    g_hash_table_insert(sh->layers, owned, GINT_TO_POINTER(rank + 1));
    return rank;
}

static char *css_layer_join(const char *parent, const char *child);

static char *
css_layer_anonymous(ns_css_stylesheet *sh, const char *current_layer)
{
    char *leaf = g_strdup_printf("@anon:%" G_GUINT64_FORMAT ":%u",
                                 sh ? sh->serial : 0,
                                 sh && sh->layer_names ? sh->layer_names->len : 0);
    char *full = css_layer_join(current_layer, leaf);
    g_free(leaf);
    css_layer_register(sh, full);
    return full;
}

static char *
css_layer_join(const char *parent, const char *child)
{
    if (!parent || !*parent) return g_strdup(child);
    if (!child || !*child) return g_strdup(parent);
    return g_strconcat(parent, ".", child, NULL);
}

static char *
css_layer_name_from_range(ns_css_stylesheet *sh, const char *current_layer,
                          const char *start, const char *end)
{
    char *name = css_trim_dup_range(start, end);
    if (!name || !*name) {
        g_free(name);
        return css_layer_anonymous(sh, current_layer);
    }
    char *full = current_layer ? css_layer_join(current_layer, name)
                               : g_strdup(name);
    css_layer_register(sh, full);
    g_free(name);
    return full;
}

static void
css_layer_register_list(ns_css_stylesheet *sh, const char *current_layer,
                        const char *start, const char *end)
{
    const char *p = start;
    while (p < end) {
        char term = 0;
        const char *item_end = css_scan_until(p, end, ",", &term);
        char *name = css_trim_dup_range(p, item_end);
        if (name && *name) {
            char *full = current_layer ? css_layer_join(current_layer, name)
                                       : g_strdup(name);
            css_layer_register(sh, full);
            g_free(full);
        }
        g_free(name);
        p = term == ',' ? item_end + 1 : item_end;
    }
}

static gboolean
css_at_keyword(const char *p, const char *end, const char *kw)
{
    gsize len = strlen(kw);
    if ((gsize)(end - p) < len) return FALSE;
    if (g_ascii_strncasecmp(p, kw, len) != 0) return FALSE;
    if (p + len == end) return TRUE;
    char c = p[len];
    return is_ws(c) || c == '(' || c == ';' || c == ',';
}

static char *
css_parse_import_url(const char **pp, const char *end)
{
    const char *p = *pp;
    p = css_skip_ws_comments(p, end);
    char *url = NULL;
    if (p + 4 <= end && g_ascii_strncasecmp(p, "url(", 4) == 0) {
        p += 4;
        p = css_skip_ws_comments(p, end);
        char quote = 0;
        if (p < end && (*p == '"' || *p == '\'')) {
            quote = *p;
            p++;
        }
        const char *start = p;
        if (quote) {
            while (p < end) {
                if (*p == '\\' && p + 1 < end) p += 2;
                else if (*p == quote) break;
                else p++;
            }
        } else {
            while (p < end && *p != ')' && !is_ws(*p)) p++;
        }
        url = g_strndup(start, (gsize)(p - start));
        if (quote && p < end && *p == quote) p++;
        p = css_skip_ws_comments(p, end);
        if (p < end && *p == ')') p++;
    } else if (p < end && (*p == '"' || *p == '\'')) {
        char quote = *p++;
        const char *start = p;
        while (p < end) {
            if (*p == '\\' && p + 1 < end) p += 2;
            else if (*p == quote) break;
            else p++;
        }
        url = g_strndup(start, (gsize)(p - start));
        if (p < end && *p == quote) p++;
    }
    *pp = p;
    if (url) g_strstrip(url);
    return url;
}

static char *
css_parse_layer_function(ns_css_stylesheet *sh, const char **pp,
                         const char *end)
{
    const char *p = *pp;
    if (!css_at_keyword(p, end, "layer")) return NULL;
    p += 5;
    p = css_skip_ws_comments(p, end);
    if (p < end && *p == '(') {
        p++;
        const char *start = p;
        int depth = 1;
        while (p < end && depth > 0) {
            if (*p == '(') depth++;
            else if (*p == ')') {
                depth--;
                if (depth == 0) break;
            }
            p++;
        }
        char *name = css_layer_name_from_range(sh, NULL, start, p);
        if (p < end && *p == ')') p++;
        *pp = p;
        return name;
    }
    *pp = p;
    return css_layer_anonymous(sh, NULL);
}

static void
css_import_clear(gpointer data)
{
    ns_css_import *im = data;
    g_free(im->url);
    g_free(im->layer_name);
    g_free(im->media);
}

static void
css_stylesheet_add_import(ns_css_stylesheet *sh, const char *url,
                          const char *layer_name, const char *media)
{
    if (!sh || !url || !*url) return;
    if (!sh->imports) {
        sh->imports = g_array_new(FALSE, FALSE, sizeof(ns_css_import));
        g_array_set_clear_func(sh->imports, css_import_clear);
    }
    if (layer_name) css_layer_register(sh, layer_name);
    ns_css_import im = {
        .url = g_strdup(url),
        .layer_name = layer_name ? g_strdup(layer_name) : NULL,
        .media = media && *media ? g_strdup(media) : NULL,
    };
    g_array_append_val(sh->imports, im);
}

static void
css_parse_import_prelude(ns_css_stylesheet *sh, const char *current_layer,
                         const char *start, const char *end)
{
    const char *p = start;
    char *url = css_parse_import_url(&p, end);
    if (!url || !*url) {
        g_free(url);
        return;
    }
    char *layer_name = NULL;
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        if (!css_at_keyword(p, end, "layer")) break;
        char *parsed = css_parse_layer_function(sh, &p, end);
        if (parsed) {
            g_free(layer_name);
            layer_name = parsed;
        }
    }
    if (current_layer) {
        char *full = layer_name ? css_layer_join(current_layer, layer_name)
                                : g_strdup(current_layer);
        g_free(layer_name);
        layer_name = full;
    }
    char *media = css_trim_dup_range(p, end);
    css_stylesheet_add_import(sh, url, layer_name, media);
    g_free(media);
    g_free(layer_name);
    g_free(url);
}

static void
ns_css_scope_text_free(gpointer data)
{
    ns_css_scope_text *s = data;
    if (!s) return;
    g_free(s->start);
    g_free(s->end);
    g_free(s);
}

static gboolean
css_scope_keyword_at(const char *p, const char *end, const char *kw)
{
    gsize n = strlen(kw);
    if ((gsize)(end - p) < n) return FALSE;
    if (g_ascii_strncasecmp(p, kw, n) != 0) return FALSE;
    return p + n == end || !is_ident(p[n]);
}

static gboolean
css_scope_selector_group_valid(GPtrArray *group)
{
    if (!group || group->len == 0) return FALSE;
    for (guint i = 0; i < group->len; i++) {
        const ns_css_selector *sel = g_ptr_array_index(group, i);
        if (!sel || sel->pseudo_element != NS_CSS_PE_NONE) return FALSE;
    }
    return TRUE;
}

static GPtrArray *
css_scope_parse_selector_list(const char *text)
{
    GPtrArray *group = parse_selector_group(text, strlen(text), 0);
    if (!css_scope_selector_group_valid(group)) {
        g_ptr_array_free(group, TRUE);
        return NULL;
    }
    return group;
}

static gboolean
css_scope_text_valid(const ns_css_scope_text *s)
{
    GPtrArray *roots = css_scope_parse_selector_list(s && s->start
                                                     ? s->start : ":root");
    if (!roots) return FALSE;
    g_ptr_array_free(roots, TRUE);
    if (s && s->end) {
        GPtrArray *limits = css_scope_parse_selector_list(s->end);
        if (!limits) return FALSE;
        g_ptr_array_free(limits, TRUE);
    }
    return TRUE;
}

static ns_css_scope_text *
css_scope_text_from_prelude(const char *start, const char *end)
{
    const char *p = css_skip_ws_comments(start, end);
    ns_css_scope_text *s = g_new0(ns_css_scope_text, 1);
    if (p < end && *p == '(') {
        char term = 0;
        const char *inner = p + 1;
        const char *close = css_scan_until(inner, end, ")", &term);
        if (term != ')') {
            ns_css_scope_text_free(s);
            return NULL;
        }
        s->start = css_trim_dup_range(inner, close);
        p = close + 1;
        if (!s->start || !*s->start) {
            ns_css_scope_text_free(s);
            return NULL;
        }
    }
    p = css_skip_ws_comments(p, end);
    if (css_scope_keyword_at(p, end, "to")) {
        p += 2;
        p = css_skip_ws_comments(p, end);
        if (p >= end || *p != '(') {
            ns_css_scope_text_free(s);
            return NULL;
        }
        char term = 0;
        const char *inner = p + 1;
        const char *close = css_scan_until(inner, end, ")", &term);
        if (term != ')') {
            ns_css_scope_text_free(s);
            return NULL;
        }
        s->end = css_trim_dup_range(inner, close);
        p = close + 1;
        if (!s->end || !*s->end) {
            ns_css_scope_text_free(s);
            return NULL;
        }
    }
    p = css_skip_ws_comments(p, end);
    if (p < end || !css_scope_text_valid(s)) {
        ns_css_scope_text_free(s);
        return NULL;
    }
    return s;
}

static ns_css_scope *
css_scope_from_text(const ns_css_scope_text *text)
{
    ns_css_scope *s = g_new0(ns_css_scope, 1);
    s->roots = css_scope_parse_selector_list(text && text->start
                                             ? text->start : ":root");
    if (!s->roots) {
        ns_css_scope_free(s);
        return NULL;
    }
    if (text && text->end) {
        s->limits = css_scope_parse_selector_list(text->end);
        if (!s->limits) {
            ns_css_scope_free(s);
            return NULL;
        }
    }
    return s;
}

static gboolean
css_scope_stack_apply_to_rule(ns_css_rule *rule, GPtrArray *scope_stack)
{
    if (!rule || !scope_stack || scope_stack->len == 0) return TRUE;
    rule->scopes = g_ptr_array_new_with_free_func((GDestroyNotify)ns_css_scope_free);
    for (guint i = 0; i < scope_stack->len; i++) {
        ns_css_scope_text *text = g_ptr_array_index(scope_stack, i);
        ns_css_scope *scope = css_scope_from_text(text);
        if (!scope) return FALSE;
        g_ptr_array_add(rule->scopes, scope);
    }
    return TRUE;
}

static gboolean
css_selector_segment_has_scope_marker(const char *p, const char *end)
{
    char quote = 0;
    int paren = 0, bracket = 0;
    while (p < end) {
        char c = *p;
        if (quote) {
            if (c == '\\' && p + 1 < end) p += 2;
            else {
                if (c == quote) quote = 0;
                p++;
            }
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
        if (c == '[') bracket++;
        else if (c == ']' && bracket > 0) bracket--;
        else if (c == '(') paren++;
        else if (c == ')' && paren > 0) paren--;
        if (bracket == 0 && c == '&') return TRUE;
        if (bracket == 0 && c == ':' &&
            (gsize)(end - p) >= 6 &&
            g_ascii_strncasecmp(p + 1, "scope", 5) == 0 &&
            (p + 6 == end || !is_ident(p[6])))
            return TRUE;
        p++;
    }
    return FALSE;
}

static void
css_scope_append_amp_rewritten(GString *out, const char *p, const char *end)
{
    char quote = 0;
    int bracket = 0;
    while (p < end) {
        char c = *p;
        if (quote) {
            if (c == '\\' && p + 1 < end) {
                g_string_append_len(out, p, 2);
                p += 2;
                continue;
            }
            g_string_append_c(out, c);
            if (c == quote) quote = 0;
            p++;
            continue;
        }
        if (c == '/' && p + 1 < end && p[1] == '*') {
            const char *q = css_skip_comment(p, end);
            g_string_append_len(out, p, (gssize)(q - p));
            p = q;
            continue;
        }
        if (c == '\\' && p + 1 < end) {
            g_string_append_len(out, p, 2);
            p += 2;
            continue;
        }
        if (c == '"' || c == '\'') {
            quote = c;
            g_string_append_c(out, c);
            p++;
            continue;
        }
        if (c == '[') bracket++;
        else if (c == ']' && bracket > 0) bracket--;
        if (c == '&' && bracket == 0) {
            g_string_append(out, ":scope");
            p++;
            continue;
        }
        g_string_append_c(out, c);
        p++;
    }
}

static char *
css_scope_selector_list_text(const char *start, const char *end)
{
    GString *out = g_string_new(NULL);
    const char *p = start;
    gboolean first = TRUE;
    while (p < end) {
        char term = 0;
        const char *seg_end = css_scan_until(p, end, ",", &term);
        const char *s = p;
        const char *e = seg_end;
        while (s < e && is_ws(*s)) s++;
        while (e > s && is_ws(e[-1])) e--;
        if (s < e) {
            if (!first) g_string_append(out, ", ");
            first = FALSE;
            gboolean has_scope = css_selector_segment_has_scope_marker(s, e);
            if (!has_scope) {
                g_string_append(out, ":where(:scope) ");
                g_string_append_len(out, s, (gssize)(e - s));
            } else {
                css_scope_append_amp_rewritten(out, s, e);
            }
        }
        p = term == ',' ? seg_end + 1 : seg_end;
    }
    return g_string_free(out, FALSE);
}

static gboolean
page_named_size(const char *name, double *w, double *h)
{
    static const struct { const char *name; double w_mm, h_mm; } kSizes[] = {
        { "a3", 297, 420 }, { "a4", 210, 297 }, { "a5", 148, 210 },
        { "b4", 250, 353 }, { "b5", 176, 250 },
        { "jis-b4", 257, 364 }, { "jis-b5", 182, 257 },
        { NULL, 0, 0 },
    };
    if (g_ascii_strcasecmp(name, "letter") == 0) { *w = 8.5 * 96; *h = 11 * 96; return TRUE; }
    if (g_ascii_strcasecmp(name, "legal") == 0)  { *w = 8.5 * 96; *h = 14 * 96; return TRUE; }
    if (g_ascii_strcasecmp(name, "ledger") == 0) { *w = 11 * 96;  *h = 17 * 96; return TRUE; }
    for (int i = 0; kSizes[i].name; i++) {
        if (g_ascii_strcasecmp(name, kSizes[i].name) != 0) continue;
        *w = kSizes[i].w_mm * (96.0 / 25.4);
        *h = kSizes[i].h_mm * (96.0 / 25.4);
        return TRUE;
    }
    return FALSE;
}

static gboolean
page_length_px(const char *text, double *out)
{
    ns_css_value *v = ns_css_parse_value_for(NS_CSS_WIDTH, text);
    gboolean ok = v && v->kind == NS_CSS_V_LENGTH &&
                  v->u.length.unit == NS_CSS_UNIT_PX;
    if (ok) *out = v->u.length.v;
    ns_css_value_free(v);
    return ok;
}

static void
page_apply_size(ns_css_page_rule *pr, const char *text)
{
    char **parts = g_strsplit_set(text, " \t\r\n", -1);
    double w = 0, h = 0, n1 = 0, n2 = 0;
    int lengths = 0;
    gboolean named = FALSE, portrait = FALSE, landscape = FALSE, bad = FALSE;
    for (int i = 0; parts[i]; i++) {
        if (!*parts[i]) continue;
        double px = 0;
        if (page_named_size(parts[i], &w, &h)) named = TRUE;
        else if (g_ascii_strcasecmp(parts[i], "portrait") == 0) portrait = TRUE;
        else if (g_ascii_strcasecmp(parts[i], "landscape") == 0) landscape = TRUE;
        else if (g_ascii_strcasecmp(parts[i], "auto") == 0) continue;
        else if (page_length_px(parts[i], &px)) {
            if (lengths == 0) n1 = px;
            else if (lengths == 1) n2 = px;
            lengths++;
        } else bad = TRUE;
    }
    g_strfreev(parts);
    if (bad || lengths > 2) return;
    if (lengths == 1) { w = h = n1; }
    else if (lengths == 2) { w = n1; h = n2; }
    else if (!named) {
        if (!portrait && !landscape) return;
        page_named_size("a4", &w, &h);
    }
    if (!(w > 0) || !(h > 0)) return;
    if (landscape && w < h) { double t = w; w = h; h = t; }
    if (portrait && w > h) { double t = w; w = h; h = t; }
    pr->width = w;
    pr->height = h;
    pr->has_size = TRUE;
    pr->landscape = w > h;
}

static void
page_apply_margin(ns_css_page_rule *pr, const char *text)
{
    char **parts = g_strsplit_set(text, " \t\r\n", -1);
    double v[4];
    int n = 0;
    for (int i = 0; parts[i] && n < 4; i++) {
        if (!*parts[i]) continue;
        if (!page_length_px(parts[i], &v[n])) { n = -1; break; }
        n++;
    }
    g_strfreev(parts);
    if (n < 1) return;
    double top = v[0];
    double right = n > 1 ? v[1] : top;
    double bottom = n > 2 ? v[2] : top;
    double left = n > 3 ? v[3] : right;
    pr->margin[0] = top;
    pr->margin[1] = right;
    pr->margin[2] = bottom;
    pr->margin[3] = left;
    for (int i = 0; i < 4; i++) pr->has_margin[i] = TRUE;
}

static void
css_parse_page_block(ns_css_stylesheet *sh, const char *body_start,
                     const char *body_end)
{
    if (!sh->page_rule)
        sh->page_rule = g_new0(ns_css_page_rule, 1);
    ns_css_page_rule *pr = sh->page_rule;
    static const char *const kSides[4] = {
        "margin-top", "margin-right", "margin-bottom", "margin-left"
    };
    const char *p = body_start;
    while (p < body_end) {
        char term = 0;
        const char *decl_end = css_scan_declaration_value(p, body_end, &term);
        char *decl = g_strndup(p, (gsize)(decl_end - p));
        char *line = g_strstrip(decl);
        char *colon = (char *)css_find_top_level_char(line,
                                                      line + strlen(line), ':');
        if (colon && line[0] != '@') {
            *colon = '\0';
            char *name = g_strstrip(line);
            char *value = g_strstrip(colon + 1);
            if (g_ascii_strcasecmp(name, "size") == 0)
                page_apply_size(pr, value);
            else if (g_ascii_strcasecmp(name, "margin") == 0)
                page_apply_margin(pr, value);
            else
                for (int i = 0; i < 4; i++) {
                    if (g_ascii_strcasecmp(name, kSides[i]) != 0) continue;
                    if (page_length_px(value, &pr->margin[i]))
                        pr->has_margin[i] = TRUE;
                }
        }
        g_free(decl);
        if (!term) break;
        p = decl_end + 1;
    }
}

static const char *
css_skip_invalid_qualified_rule(const char *p, const char *end,
                                gboolean nested)
{
    while (p < end) {
        char term = 0;
        const char *seg = css_scan_segment(p, end, &term);
        if (term == '{') return css_skip_to_block_end(seg, end);
        if (term == 0 || seg >= end) return end;
        if (term == '}' && nested) return seg;
        p = seg + 1;
    }
    return end;
}

static void
parse_rules_until(const char **pp, const char *end,
                  ns_css_stylesheet *sh, int *source_order,
                  char close_at, const char *current_layer,
                  GPtrArray *scope_stack)
{
    static int at_depth;
    gboolean nested = close_at == '}';
    if (nested) {
        if (at_depth >= NS_CSS_MAX_AT_NESTING) {
            const char *p = *pp;
            char term = 0;
            const char *seg = css_scan_until(p, end, "}", &term);
            p = term == '}' ? seg + 1 : seg;
            *pp = p;
            return;
        }
        at_depth++;
    }
    const char *p = *pp;
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        if (p >= end) break;

        if (!nested && p + 4 <= end && memcmp(p, "<!--", 4) == 0) {
            p += 4;
            continue;
        }
        if (!nested && p + 3 <= end && memcmp(p, "-->", 3) == 0) {
            p += 3;
            continue;
        }
        if (*p == '}') {
            if (close_at == '}') {
                p++;
                break;
            }
            p = css_skip_invalid_qualified_rule(p + 1, end, FALSE);
            continue;
        }
        if (*p == '@') {
            const char *at_start = p;
            p++;
            char *at_name = ns_css_read_ident(&p, end);
            if (!at_name || !*at_name) {
                g_free(at_name);
                p = at_start;
                skip_at_rule(&p, end);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "import") == 0) {
                char term = 0;
                const char *prelude_start = p;
                const char *prelude_end = css_scan_segment(p, end, &term);
                if (term == ';') {
                    css_parse_import_prelude(sh, current_layer,
                                             prelude_start, prelude_end);
                    p = prelude_end + 1;
                } else {
                    p = at_start;
                    skip_at_rule(&p, end);
                }
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "supports") == 0) {
                char term = 0;
                const char *cond_start = p;
                const char *cond_end = css_scan_segment(p, end, &term);
                p = cond_end;
                gsize cond_len = (gsize)(cond_end - cond_start);
                char *cond = g_strndup(cond_start, cond_len);
                g_strstrip(cond);
                if (p < end && *p == '{') {
                    p++;
                    if (ns_css_supports_condition(cond, FALSE)) {
                        parse_rules_until(&p, end, sh, source_order, '}',
                                          current_layer, scope_stack);
                    } else {
                        p = css_skip_to_block_end(p - 1, end);
                    }
                } else if (p < end && *p == ';') p++;
                g_free(cond);
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "font-face") == 0) {
                char term = 0;
                const char *prelude_end = css_scan_segment(p, end, &term);
                p = prelude_end;
                if (term == '{') {
                    const char *block_end = css_skip_to_block_end(p, end);
                    const char *body_start = p + 1;
                    const char *body_end = css_block_body_end(body_start,
                                                              block_end);
                    char *family = NULL;
                    char *src_url = NULL;
                    char *unicode_range = NULL;
                    ns_font_descriptors descriptors = { 0, NS_FONT_SLANT_AUTO };
                    const char *decl_p = body_start;
                    while (decl_p < body_end) {
                        char dterm = 0;
                        const char *decl_end =
                            css_scan_declaration_value(decl_p, body_end, &dterm);
                        char *decl = g_strndup(decl_p,
                                               (gsize)(decl_end - decl_p));
                        char *line = g_strstrip(decl);
                        char *colon = (char *)css_find_top_level_char(
                            line, line + strlen(line), ':');
                        if (!colon) {
                            g_free(decl);
                            if (!dterm) break;
                            decl_p = decl_end + 1;
                            continue;
                        }
                        *colon = '\0';
                        char *prop = g_strstrip(line);
                        char *val  = g_strstrip(colon + 1);
                        if (g_ascii_strcasecmp(prop, "font-family") == 0 && !family) {
                            char *v = val;
                            while (*v == ' ' || *v == '\'' || *v == '"') v++;
                            gsize vlen = strlen(v);
                            while (vlen > 0 && (v[vlen - 1] == ' ' ||
                                                v[vlen - 1] == '\'' ||
                                                v[vlen - 1] == '"')) vlen--;
                            if (vlen > 0) family = g_strndup(v, vlen);
                        } else if (g_ascii_strcasecmp(prop, "src") == 0) {
                            font_src_consider_urls(&src_url, val);
                        } else if (g_ascii_strcasecmp(prop, "unicode-range") == 0) {
                            g_free(unicode_range);
                            unicode_range = g_strdup(val);
                        } else if (g_ascii_strcasecmp(prop, "font-weight") == 0) {
                            descriptors.weight = font_face_weight_descriptor(val);
                        } else if (g_ascii_strcasecmp(prop, "font-style") == 0) {
                            descriptors.slant = font_face_style_descriptor(val);
                        }
                        g_free(decl);
                        if (!dterm) break;
                        decl_p = decl_end + 1;
                    }
                    if (!sh->font_faces) {
                        sh->font_faces = g_array_new(FALSE, FALSE,
                                                     sizeof(ns_css_font_face));
                        g_array_set_clear_func(sh->font_faces, font_face_clear);
                    }
                    if (family && *family && src_url && *src_url) {
                        ns_css_font_face ff = {
                            family, src_url, unicode_range, descriptors
                        };
                        g_array_append_val(sh->font_faces, ff);
                        family = NULL;
                        src_url = NULL;
                        unicode_range = NULL;
                    }
                    g_free(family);
                    g_free(src_url);
                    g_free(unicode_range);
                    p = block_end;
                } else if (term == ';') p++;
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "keyframes") == 0 ||
                g_ascii_strcasecmp(at_name, "-webkit-keyframes") == 0) {
                char term = 0;
                const char *nm_start = p;
                const char *prelude_end = css_scan_segment(p, end, &term);
                char *kf_name = css_keyframes_name_from_range(nm_start,
                                                              prelude_end);
                {
                    const char *q = nm_start;
                    while (q < prelude_end && is_ws(*q)) q++;
                    gboolean quoted = q < prelude_end && (*q == '"' || *q == '\'');
                    if (kf_name && *kf_name && !quoted &&
                        (ns_css_wide_keyword_or_default(kf_name) ||
                         g_ascii_strcasecmp(kf_name, "none") == 0 ||
                         !ns_css_content_ident_valid(kf_name))) {
                        g_free(kf_name);
                        kf_name = g_strdup("");
                    }
                }
                p = prelude_end;
                if (term == '{') {
                    p++;
                    GArray *stops = g_array_new(FALSE, FALSE,
                                                sizeof(ns_css_keyframe_stop));
                    while (p < end) {
                        p = css_skip_ws_comments(p, end);
                        if (p < end && *p == '}') { p++; break; }
                        const char *sel_start = p;
                        char sel_term = 0;
                        const char *sel_end =
                            css_scan_segment(p, end, &sel_term);
                        if (sel_term != '{') break;
                        const char *body_start = sel_end + 1;
                        const char *block_end = css_skip_to_block_end(sel_end, end);
                        const char *body_end = css_block_body_end(body_start,
                                                                  block_end);
                        p = block_end;
                        gsize sel_len = (gsize)(sel_end - sel_start);
                        char *sel = g_strndup(sel_start, sel_len);
                        g_strstrip(sel);
                        double op = 0;
                        gboolean has_op = FALSE;
                        ns_css_transform tf = { 0 };
                        gboolean has_tf = FALSE;
                        ns_css_transform tf_ind = { 0 };
                        guint8 col[4] = { 0 }, bgcol[4] = { 0 };
                        gboolean has_col = FALSE, has_bgcol = FALSE;
                        GString *raw = NULL;
                        const char *decl_p = body_start;
                        while (decl_p < body_end) {
                            char dterm = 0;
                            const char *decl_end =
                                css_scan_declaration_value(decl_p, body_end, &dterm);
                            char *decl = g_strndup(decl_p,
                                                   (gsize)(decl_end - decl_p));
                            char *line = g_strstrip(decl);
                            char *colon = (char *)css_find_top_level_char(
                                line, line + strlen(line), ':');
                            if (!colon) {
                                g_free(decl);
                                if (!dterm) break;
                                decl_p = decl_end + 1;
                                continue;
                            }
                            *colon = '\0';
                            char *prop = g_strstrip(line);
                            char *val  = g_strstrip(colon + 1);
                            gboolean tf_prop =
                                g_ascii_strcasecmp(prop, "transform") == 0 ||
                                g_ascii_strcasecmp(prop, "translate") == 0 ||
                                g_ascii_strcasecmp(prop, "rotate") == 0 ||
                                g_ascii_strcasecmp(prop, "scale") == 0;
                            if (!raw) raw = g_string_new(NULL);
                            if (raw->len) g_string_append_c(raw, ';');
                            g_string_append_printf(raw, "%s:%s", prop, val);
                            if (tf_prop && strstr(val, "var(")) {
                            } else if (g_ascii_strcasecmp(prop, "opacity") == 0) {
                                op = g_ascii_strtod(val, NULL);
                                has_op = TRUE;
                            } else if (g_ascii_strcasecmp(prop, "transform") == 0) {
                                ns_css_value *tv = ns_css_parse_transform(val);
                                if (tv) {
                                    tf = tv->u.transform;
                                    has_tf = TRUE;
                                    ns_css_value_free(tv);
                                }
                            } else if (g_ascii_strcasecmp(prop, "translate") == 0 ||
                                       g_ascii_strcasecmp(prop, "rotate") == 0 ||
                                       g_ascii_strcasecmp(prop, "scale") == 0) {
                                ns_css_value *tv =
                                    g_ascii_strcasecmp(prop, "translate") == 0
                                        ? ns_css_parse_translate_prop(val)
                                    : g_ascii_strcasecmp(prop, "rotate") == 0
                                        ? ns_css_parse_rotate_prop(val)
                                        : ns_css_parse_scale_prop(val);
                                if (tv) {
                                    if (tf_ind.n_ops < NS_CSS_TRANSFORM_OPS_MAX)
                                        tf_ind.ops[tf_ind.n_ops++] =
                                            tv->u.transform.ops[0];
                                    ns_css_value_free(tv);
                                }
                            } else if (g_ascii_strcasecmp(prop, "color") == 0) {
                                if (ns_css_parse_color(val, &col[0], &col[1],
                                                &col[2], &col[3]))
                                    has_col = TRUE;
                            } else if (g_ascii_strcasecmp(prop, "background-color") == 0 ||
                                       g_ascii_strcasecmp(prop, "background") == 0) {
                                if (ns_css_parse_color(val, &bgcol[0], &bgcol[1],
                                                &bgcol[2], &bgcol[3]))
                                    has_bgcol = TRUE;
                            }
                            g_free(decl);
                            if (!dterm) break;
                            decl_p = decl_end + 1;
                        }
                        if (tf_ind.n_ops > 0) {
                            ns_css_transform merged = tf_ind;
                            for (int k = 0; k < tf.n_ops &&
                                            merged.n_ops < NS_CSS_TRANSFORM_OPS_MAX;
                                 k++)
                                merged.ops[merged.n_ops++] = tf.ops[k];
                            tf = merged;
                            has_tf = TRUE;
                        }
                        const char *sel_p = sel;
                        const char *sel_all_end = sel + strlen(sel);
                        while (sel_p < sel_all_end) {
                            char cterm = 0;
                            const char *one_end =
                                css_scan_until(sel_p, sel_all_end, ",", &cterm);
                            char *one = css_trim_dup_range(sel_p, one_end);
                            double pct = 0;
                            if (parse_keyframe_stop_pct(one, &pct)) {
                                ns_css_keyframe_stop s = {
                                    .pct = pct,
                                    .opacity = op, .has_opacity = has_op,
                                    .transform = tf, .has_transform = has_tf,
                                    .has_color = has_col, .has_bg_color = has_bgcol,
                                    .raw_props = raw && raw->len
                                        ? g_strdup(raw->str) : NULL,
                                };
                                memcpy(s.color, col, 4);
                                memcpy(s.bg_color, bgcol, 4);
                                g_array_append_val(stops, s);
                            }
                            g_free(one);
                            sel_p = cterm == ',' ? one_end + 1 : one_end;
                        }
                        if (raw) g_string_free(raw, TRUE);
                        g_free(sel);
                    }
                    if (kf_name && *kf_name) {
                        if (!sh->keyframes) {
                            sh->keyframes = g_array_new(FALSE, FALSE,
                                                        sizeof(ns_css_keyframes));
                            g_array_set_clear_func(sh->keyframes, keyframes_clear);
                        }
                        g_array_sort(stops, keyframe_stop_cmp);
                        ns_css_keyframes kf = {
                            .name = g_strdup(kf_name),
                            .n_stops = (int)stops->len,
                            .stops = stops->len > 0
                                ? (ns_css_keyframe_stop *)g_memdup2(
                                    stops->data,
                                    stops->len * sizeof(ns_css_keyframe_stop))
                                : g_new0(ns_css_keyframe_stop, 1),
                        };
                        g_array_append_val(sh->keyframes, kf);
                    } else {
                        for (guint i = 0; i < stops->len; i++)
                            g_free(g_array_index(stops, ns_css_keyframe_stop,
                                                 i).raw_props);
                    }
                    g_array_free(stops, TRUE);
                } else if (term == ';') p++;
                g_free(kf_name);
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "media") == 0) {
                char term = 0;
                const char *cond_start = p;
                const char *cond_end = css_scan_segment(p, end, &term);
                p = cond_end;
                gsize cond_len = (gsize)(cond_end - cond_start);
                char *cond = g_strndup(cond_start, cond_len);
                g_strstrip(cond);
                if (p < end && *p == '{') {
                    p++;
                    if (ns_css_media_query_matches(cond)) {
                        parse_rules_until(&p, end, sh, source_order, '}',
                                          current_layer, scope_stack);
                    } else {
                        p = css_skip_to_block_end(p - 1, end);
                    }
                } else if (p < end && *p == ';') p++;
                g_free(cond);
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "container") == 0) {
                char term = 0;
                const char *cond_start = p;
                const char *cond_end = css_scan_segment(p, end, &term);
                p = cond_end;
                gsize cond_len = (gsize)(cond_end - cond_start);
                char *cond = g_strndup(cond_start, cond_len);
                g_strstrip(cond);
                char *canon = ns_css_container_condition_canonical(cond);
                if (p < end && *p == '{' && !canon) {
                    p = css_skip_to_block_end(p, end);
                } else if (p < end && *p == '{') {
                    p++;
                    guint before = sh->rules->len;
                    parse_rules_until(&p, end, sh, source_order, '}',
                                      current_layer, scope_stack);
                    sh->has_container_rules = TRUE;
                    for (guint ri = before; ri < sh->rules->len; ri++) {
                        ns_css_rule *r = g_ptr_array_index(sh->rules, ri);
                        if (r->container_condition) {
                            char *joined = g_strdup_printf("%s\x1f%s",
                                canon, r->container_condition);
                            g_free(r->container_condition);
                            r->container_condition = joined;
                        } else {
                            r->container_condition = g_strdup(canon);
                        }
                    }
                } else if (p < end && *p == ';') p++;
                g_free(canon);
                g_free(cond);
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "scope") == 0) {
                char term = 0;
                const char *prelude_start = p;
                const char *prelude_end = css_scan_segment(p, end, &term);
                p = prelude_end;
                if (term == '{') {
                    ns_css_scope_text *scope =
                        css_scope_text_from_prelude(prelude_start, prelude_end);
                    p++;
                    if (scope) {
                        g_ptr_array_add(scope_stack, scope);
                        parse_rules_until(&p, end, sh, source_order, '}',
                                          current_layer, scope_stack);
                        g_ptr_array_remove_index(scope_stack,
                                                 scope_stack->len - 1);
                    } else {
                        p = css_skip_to_block_end(p - 1, end);
                    }
                } else if (term == ';') p++;
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "layer") == 0) {
                char term = 0;
                const char *prelude_start = p;
                const char *prelude_end = css_scan_segment(p, end, &term);
                if (term == '{') {
                    char *layer_name = css_layer_name_from_range(
                        sh, current_layer, prelude_start, prelude_end);
                    p = prelude_end;
                    p++;
                    parse_rules_until(&p, end, sh, source_order, '}',
                                      layer_name, scope_stack);
                    g_free(layer_name);
                } else if (term == ';') {
                    css_layer_register_list(sh, current_layer,
                                            prelude_start, prelude_end);
                    p = prelude_end + 1;
                } else p = prelude_end;
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "property") == 0) {
                char term = 0;
                const char *name_start = p;
                const char *name_end = css_scan_segment(p, end, &term);
                p = name_end;
                char *prop_name = css_trim_dup_range(name_start, name_end);
                if (term == '{' && prop_name &&
                    prop_name[0] == '-' && prop_name[1] == '-' && prop_name[2]) {
                    const char *block_end = css_skip_to_block_end(p, end);
                    const char *body_start = p + 1;
                    const char *body_end = css_block_body_end(body_start,
                                                              block_end);
                    char *initial_value = NULL;
                    char *syntax_text = NULL;
                    gboolean inherits = TRUE;
                    gboolean has_inherits = FALSE;
                    gboolean has_initial = FALSE;
                    const char *decl_p = body_start;
                    while (decl_p < body_end) {
                        char dterm = 0;
                        const char *decl_end =
                            css_scan_declaration_value(decl_p, body_end, &dterm);
                        char *decl = g_strndup(decl_p,
                                               (gsize)(decl_end - decl_p));
                        char *line = g_strstrip(decl);
                        char *colon = (char *)css_find_top_level_char(
                            line, line + strlen(line), ':');
                        if (colon) {
                            *colon = '\0';
                            char *dprop = g_strstrip(line);
                            char *dval = g_strstrip(colon + 1);
                            if (g_ascii_strcasecmp(dprop, "inherits") == 0) {
                                if (g_ascii_strcasecmp(dval, "true") == 0 ||
                                    g_ascii_strcasecmp(dval, "false") == 0) {
                                    inherits =
                                        g_ascii_strcasecmp(dval, "false") != 0;
                                    has_inherits = TRUE;
                                }
                            } else if (g_ascii_strcasecmp(dprop,
                                                          "initial-value") == 0) {
                                g_free(initial_value);
                                initial_value = g_strdup(dval);
                                has_initial = TRUE;
                            } else if (g_ascii_strcasecmp(dprop,
                                                          "syntax") == 0) {
                                g_free(syntax_text);
                                syntax_text = css_string_descriptor_dup(dval);
                            }
                        }
                        g_free(decl);
                        if (!dterm) break;
                        decl_p = decl_end + 1;
                    }
                    ns_css_syntax_def *syntax =
                        syntax_text ? ns_css_syntax_def_parse(syntax_text) : NULL;
                    gboolean rule_valid = syntax && has_inherits;
                    if (rule_valid && !ns_css_syntax_def_universal(syntax))
                        rule_valid = has_initial &&
                            ns_css_syntax_def_initial_valid(syntax,
                                                            initial_value);
                    else if (rule_valid && has_initial)
                        rule_valid = ns_css_syntax_def_initial_valid(
                            syntax, initial_value);
                    if (rule_valid) {
                        if (!sh->property_rules) {
                            sh->property_rules = g_array_new(FALSE, FALSE,
                                sizeof(ns_css_property_rule));
                            g_array_set_clear_func(sh->property_rules,
                                                   property_rule_clear);
                        }
                        ns_css_property_rule pr = {
                            .name = g_strdup(prop_name),
                            .initial_value = initial_value,
                            .syntax_text = syntax_text,
                            .syntax = syntax,
                            .inherits = inherits,
                            .has_initial = has_initial,
                        };
                        g_array_append_val(sh->property_rules, pr);
                    } else {
                        g_free(initial_value);
                        g_free(syntax_text);
                        ns_css_syntax_def_free(syntax);
                    }
                    p = block_end;
                } else if (term == ';' && p < end) {
                    p++;
                } else if (term == '{') {
                    p = css_skip_to_block_end(p, end);
                }
                g_free(prop_name);
                g_free(at_name);
                continue;
            }
            if (g_ascii_strcasecmp(at_name, "page") == 0) {
                char term = 0;
                const char *prelude_end = css_scan_segment(p, end, &term);
                p = prelude_end;
                if (term == '{') {
                    const char *block_end = css_skip_to_block_end(p, end);
                    const char *body_start = p + 1;
                    css_parse_page_block(sh, body_start,
                                         css_block_body_end(body_start,
                                                            block_end));
                    p = block_end;
                } else if (term == ';' && p < end) p++;
                g_free(at_name);
                continue;
            }
            g_free(at_name);
            p = at_start;
            skip_at_rule(&p, end);
            continue;
        }

        ns_css_rule *rule = g_new0(ns_css_rule, 1);
        rule->selectors = g_ptr_array_new();
        rule->decls     = g_array_new(FALSE, FALSE, sizeof(ns_css_decl));
        rule->layer_name = current_layer ? g_strdup(current_layer) : NULL;
        rule->source_order = (*source_order)++;
        if (!css_scope_stack_apply_to_rule(rule, scope_stack)) {
            ns_css_rule_free(rule);
            char term = 0;
            const char *skip_to = css_scan_segment(p, end, &term);
            if (term == '{') p = css_skip_to_block_end(skip_to, end);
            else p = term == ';' ? skip_to + 1 : skip_to;
            continue;
        }

        char term = 0;
        const char *sel_start = p;
        const char *sel_end = css_scan_segment(p, end, &term);
        if (term != '{') {
            ns_css_rule_free(rule);
            p = term == ';'
                ? css_skip_invalid_qualified_rule(sel_end + 1, end, nested)
                : sel_end;
            continue;
        }
        char *scoped_sel = rule->scopes
            ? css_scope_selector_list_text(sel_start, sel_end) : NULL;
        const char *parse_p = scoped_sel ? scoped_sel : sel_start;
        const char *parse_end = scoped_sel ? scoped_sel + strlen(scoped_sel)
                                           : sel_end;

        gboolean ok = FALSE;
        g_sel_has_hover = FALSE;
        g_sel_has_active = FALSE;
        g_sel_parse_error = FALSE;
        while (parse_p < parse_end) {
            ns_css_selector *sel = parse_one_selector(&parse_p, parse_end, 0);
            if (sel) {
                g_ptr_array_add(rule->selectors, sel);
                ok = TRUE;
            } else {
                g_sel_parse_error = TRUE;
            }
            while (parse_p < parse_end && is_ws(*parse_p)) parse_p++;
            if (parse_p < parse_end && *parse_p == ',') {
                parse_p++;
                while (parse_p < parse_end && is_ws(*parse_p)) parse_p++;
                if (parse_p >= parse_end) g_sel_parse_error = TRUE;
                continue;
            }
            if (parse_p < parse_end) g_sel_parse_error = TRUE;
            break;
        }
        if (g_sel_parse_error) ok = FALSE;
        if (ok && g_sel_has_hover)
            sh->has_hover_rules = TRUE;
        if (ok && g_sel_has_active)
            sh->has_active_rules = TRUE;
        g_free(scoped_sel);
        if (!ok) {
            ns_css_rule_free(rule);
            p = css_skip_to_block_end(sel_end, end);
            continue;
        }
        p = sel_end + 1;
        parse_declaration_block(&p, end, rule->decls, rule);
        g_ptr_array_add(sh->rules, rule);
    }
    *pp = p;
    if (nested) at_depth--;
}

static gboolean
css_append_nested_selector(GString *out, const char *part,
                           const char *parent_expr)
{
    const char *p = part;
    const char *end = part + strlen(part);
    char quote = 0;
    int bracket = 0;
    gboolean replaced = FALSE;
    while (p < end) {
        char c = *p;
        if (quote) {
            if (c == '\\' && p + 1 < end) {
                g_string_append_len(out, p, 2);
                p += 2;
                continue;
            }
            g_string_append_c(out, c);
            if (c == quote) quote = 0;
            p++;
            continue;
        }
        if (c == '/' && p + 1 < end && p[1] == '*') {
            const char *q = css_skip_comment(p, end);
            g_string_append_len(out, p, (gssize)(q - p));
            p = q;
            continue;
        }
        if (c == '\\' && p + 1 < end) {
            g_string_append_len(out, p, 2);
            p += 2;
            continue;
        }
        if (c == '"' || c == '\'') {
            quote = c;
            g_string_append_c(out, c);
            p++;
            continue;
        }
        if (c == '[') bracket++;
        else if (c == ']' && bracket > 0) bracket--;
        if (c == '&' && bracket == 0) {
            g_string_append(out, parent_expr);
            replaced = TRUE;
            p++;
            continue;
        }
        g_string_append_c(out, c);
        p++;
    }
    return replaced;
}

static char *
css_combine_selectors(const char *parent, const char *child, gsize max_len)
{
    char *pc = g_strstrip(g_strdup(parent));
    char *cc = g_strstrip(g_strdup(child));
    gsize per_parent = strlen(pc) + 6;
    GString *out = g_string_new(NULL);
    const char *p = cc;
    const char *end = cc + strlen(cc);
    while (p < end) {
        char term = 0;
        const char *seg = css_scan_until(p, end, ",", &term);
        char *part_buf = css_trim_dup_range(p, seg);
        char *part = part_buf;
        if (!*part) {
            g_free(part_buf);
            p = term == ',' ? seg + 1 : seg;
            continue;
        }
        gsize part_len = strlen(part);
        gsize amps = 0;
        for (const char *q = part; *q; q++)
            if (*q == '&') amps++;
        gsize room = max_len - out->len;
        if (part_len + 2 > room ||
            amps >= (room - part_len - 2) / per_parent) {
            g_free(part_buf);
            g_free(pc);
            g_free(cc);
            g_string_free(out, TRUE);
            return NULL;
        }
        if (out->len) g_string_append(out, ", ");
        char *isparent = g_strdup_printf(":is(%s)", pc);
        GString *piece = g_string_new(NULL);
        if (css_append_nested_selector(piece, part, isparent))
            g_string_append_len(out, piece->str, (gssize)piece->len);
        else
            g_string_append_printf(out, ":is(%s) %s", pc, part);
        g_string_free(piece, TRUE);
        g_free(isparent);
        g_free(part_buf);
        p = term == ',' ? seg + 1 : seg;
    }
    g_free(pc);
    g_free(cc);
    return g_string_free(out, FALSE);
}

static gboolean css_body_has_nested_rule(const char *s, const char *e);
static void css_flatten_style_rule(GString *out, const char *sel,
                                   const char *body_s, const char *body_e,
                                   int depth, gsize *budget);

#define NS_CSS_NEST_MAX_DEPTH 128
#define NS_CSS_NEST_SELECTOR_BUDGET ((gsize)16 * 1024 * 1024)

static void
css_trim_selector(char *sel)
{
    char *s = sel;
    while (*s && is_ws(*s)) s++;
    if (s != sel) memmove(sel, s, strlen(s) + 1);
    size_t n = strlen(sel);
    while (n > 0 && is_ws((unsigned char)sel[n - 1])) {
        size_t bs = 0, i = n - 1;
        while (i > 0 && sel[i - 1] == '\\') { bs++; i--; }
        if (bs % 2 == 1) break;
        n--;
    }
    sel[n] = '\0';
}

static void
css_flatten_rule_list(GString *out, const char *p, const char *end, int depth,
                      gsize *budget)
{
    if (depth > NS_CSS_NEST_MAX_DEPTH) return;
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        if (p >= end) break;
        if (p + 4 <= end && memcmp(p, "<!--", 4) == 0) {
            p += 4;
            continue;
        }
        if (p + 3 <= end && memcmp(p, "-->", 3) == 0) {
            p += 3;
            continue;
        }
        if (*p == '}') {
            p = css_skip_invalid_qualified_rule(p + 1, end, FALSE);
            continue;
        }
        if (*p == '@') {
            const char *prelude = p;
            char term = 0;
            const char *seg_end = css_scan_segment(p, end, &term);
            if (term == '{') {
                gboolean group = (g_ascii_strncasecmp(prelude, "@media", 6) == 0 ||
                                  g_ascii_strncasecmp(prelude, "@supports", 9) == 0 ||
                                  g_ascii_strncasecmp(prelude, "@container", 10) == 0 ||
                                  g_ascii_strncasecmp(prelude, "@layer", 6) == 0 ||
                                  g_ascii_strncasecmp(prelude, "@scope", 6) == 0);
                const char *block_end = css_skip_to_block_end(seg_end, end);
                if (group) {
                    g_string_append_len(out, prelude, (gssize)(seg_end - prelude));
                    g_string_append_c(out, '{');
                    const char *body_s = seg_end + 1;
                    css_flatten_rule_list(out, body_s,
                                          css_block_body_end(body_s, block_end),
                                          depth + 1, budget);
                    g_string_append_c(out, '}');
                } else {
                    g_string_append_len(out, prelude, (gssize)(block_end - prelude));
                }
                p = block_end;
            } else {
                g_string_append_len(out, prelude, (gssize)(seg_end - prelude));
                if (term == ';' && seg_end < end) { g_string_append_c(out, ';'); p = seg_end + 1; }
                else p = seg_end;
            }
            continue;
        }
        char term = 0;
        const char *seg_end = css_scan_segment(p, end, &term);
        if (term != '{') {
            p = css_skip_invalid_qualified_rule(p, end, FALSE);
            continue;
        }
        char *sel = g_strndup(p, (gsize)(seg_end - p));
        css_trim_selector(sel);
        const char *body_s = seg_end + 1;
        const char *block_end = css_skip_to_block_end(seg_end, end);
        const char *body_e = css_block_body_end(body_s, block_end);
        css_flatten_style_rule(out, sel, body_s, body_e, depth + 1, budget);
        g_free(sel);
        p = block_end;
    }
}

static gboolean
css_body_has_nested_rule(const char *s, const char *e)
{
    const char *p = s;
    while (p < e) {
        while (p < e && is_ws(*p)) p++;
        if (p >= e) break;
        if (p + 1 < e && p[0] == '/' && p[1] == '*') {
            p += 2;
            while (p + 1 < e && !(p[0] == '*' && p[1] == '/')) p++;
            if (p + 1 < e) p += 2;
            continue;
        }
        char term = 0;
        const char *seg_end = css_scan_segment(p, e, &term);
        if (term == '{') return TRUE;
        if (term == 0) break;
        p = seg_end + 1;
    }
    return FALSE;
}

static gboolean
css_flatten_take_budget(gsize *budget, const char *sel)
{
    if (*budget == 0) return FALSE;
    gsize n = strlen(sel);
    if (n > *budget) {
        *budget = 0;
        return FALSE;
    }
    *budget -= n;
    return TRUE;
}

static void
css_flatten_flush_decls(GString *out, const char *sel, GString *decls,
                        gsize *budget)
{
    if (decls->len == 0) return;
    if (!css_flatten_take_budget(budget, sel)) {
        g_string_truncate(decls, 0);
        return;
    }
    g_string_append(out, sel);
    g_string_append_c(out, '{');
    g_string_append_len(out, decls->str, (gssize)decls->len);
    g_string_append_c(out, '}');
    g_string_truncate(decls, 0);
}

static void
css_flatten_style_rule(GString *out, const char *sel,
                       const char *body_s, const char *body_e, int depth,
                       gsize *budget)
{
    if (depth > NS_CSS_NEST_MAX_DEPTH) return;
    if (!css_body_has_nested_rule(body_s, body_e)) {
        if (!css_flatten_take_budget(budget, sel)) return;
        g_string_append(out, sel);
        g_string_append_c(out, '{');
        g_string_append_len(out, body_s, (gssize)(body_e - body_s));
        g_string_append_c(out, '}');
        return;
    }
    GString *decls = g_string_new(NULL);
    const char *p = body_s;
    while (p < body_e) {
        while (p < body_e && is_ws(*p)) p++;
        if (p >= body_e) break;
        if (p + 1 < body_e && p[0] == '/' && p[1] == '*') {
            const char *cs = p;
            p += 2;
            while (p + 1 < body_e && !(p[0] == '*' && p[1] == '/')) p++;
            if (p + 1 < body_e) p += 2;
            g_string_append_len(decls, cs, (gssize)(p - cs));
            continue;
        }
        char term = 0;
        const char *seg_end = css_scan_segment(p, body_e, &term);
        if (term == '{') {
            css_flatten_flush_decls(out, sel, decls, budget);
            char *nsel = g_strndup(p, (gsize)(seg_end - p));
            css_trim_selector(nsel);
            const char *nbody_s = seg_end + 1;
            const char *nblock_end = css_skip_to_block_end(seg_end, body_e);
            const char *nbody_e = css_block_body_end(nbody_s, nblock_end);
            if (nsel[0] == '@') {
                gboolean group =
                    g_ascii_strncasecmp(nsel, "@media", 6) == 0 ||
                    g_ascii_strncasecmp(nsel, "@supports", 9) == 0 ||
                    g_ascii_strncasecmp(nsel, "@container", 10) == 0 ||
                    g_ascii_strncasecmp(nsel, "@layer", 6) == 0 ||
                    g_ascii_strncasecmp(nsel, "@scope", 6) == 0;
                if (group) {
                    g_string_append(out, nsel);
                    g_string_append_c(out, '{');
                    css_flatten_style_rule(out, sel, nbody_s, nbody_e,
                                           depth + 1, budget);
                    g_string_append_c(out, '}');
                }
            } else {
                char *combined = *budget
                    ? css_combine_selectors(sel, nsel, *budget) : NULL;
                if (!combined)
                    *budget = 0;
                else if (css_flatten_take_budget(budget, combined))
                    css_flatten_style_rule(out, combined, nbody_s, nbody_e,
                                           depth + 1, budget);
                g_free(combined);
            }
            g_free(nsel);
            p = nblock_end;
        } else {
            g_string_append_len(decls, p, (gssize)(seg_end - p));
            if (term == ';') g_string_append_c(decls, ';');
            p = (seg_end < body_e) ? seg_end + 1 : body_e;
        }
    }
    css_flatten_flush_decls(out, sel, decls, budget);
    g_string_free(decls, TRUE);
}

static char *
css_flatten_nesting(const char *text, gssize len)
{
    if (!text) return NULL;
    if (len < 0) len = (gssize)strlen(text);
    gsize budget = (gsize)len <= (G_MAXSIZE - NS_CSS_NEST_SELECTOR_BUDGET) / 16
        ? (gsize)len * 16 + NS_CSS_NEST_SELECTOR_BUDGET : G_MAXSIZE;
    GString *out = g_string_new(NULL);
    css_flatten_rule_list(out, text, text + len, 0, &budget);
    return g_string_free(out, FALSE);
}

static guint64 g_stylesheet_serial_next = 1;

gboolean
ns_css_text_has_container_units(const char *text, gssize len)
{
    if (!text) return FALSE;
    if (len < 0) len = (gssize)strlen(text);
    const char *end = text + len;
    for (const char *p = text; p < end; p++) {
        if (*p != 'c' && *p != 'C') continue;
        gsize left = (gsize)(end - p);
        if ((left >= 3 && (g_ascii_strncasecmp(p, "cqw", 3) == 0 ||
                           g_ascii_strncasecmp(p, "cqh", 3) == 0 ||
                           g_ascii_strncasecmp(p, "cqi", 3) == 0 ||
                           g_ascii_strncasecmp(p, "cqb", 3) == 0)) ||
            (left >= 5 && (g_ascii_strncasecmp(p, "cqmin", 5) == 0 ||
                           g_ascii_strncasecmp(p, "cqmax", 5) == 0)))
            return TRUE;
    }
    return FALSE;
}

ns_css_stylesheet *
ns_css_stylesheet_parse(const char *text, gssize len_in)
{
    ns_css_stylesheet *sh = g_new0(ns_css_stylesheet, 1);
    sh->serial = g_stylesheet_serial_next++;
    sh->rules = g_ptr_array_new_with_free_func((GDestroyNotify)ns_css_rule_free);
    if (!text) return sh;
    if (len_in < 0) len_in = (gssize)strlen(text);
    sh->has_container_units = ns_css_text_has_container_units(text, len_in);

    char *flattened = css_flatten_nesting(text, len_in);
    const char *p   = flattened;
    const char *end = flattened + strlen(flattened);
    int source_order = 0;
    GPtrArray *scope_stack =
        g_ptr_array_new_with_free_func(ns_css_scope_text_free);
    parse_rules_until(&p, end, sh, &source_order, 0, NULL, scope_stack);
    g_ptr_array_free(scope_stack, TRUE);
    g_free(flattened);
    return sh;
}

static gboolean
css_url_should_resolve(const char *url)
{
    if (!url || !*url) return FALSE;
    if (url[0] == '#') return FALSE;
    if (g_ascii_strncasecmp(url, "data:", 5) == 0) return FALSE;
    if (g_ascii_strncasecmp(url, "blob:", 5) == 0) return FALSE;
    return TRUE;
}

static void
css_value_resolve_url(ns_css_value *v, const char *base_url)
{
    if (!base_url) return;
    for (; v; v = v->next_layer) {
        if (v->kind != NS_CSS_V_URL || !css_url_should_resolve(v->u.url))
            continue;
        char *abs_url = ns_url_resolve(base_url, v->u.url);
        if (!abs_url) continue;
        g_free(v->u.url);
        v->u.url = abs_url;
    }
}

static char *
css_raw_text_resolve_urls(const char *text, const char *base_url)
{
    if (!text || !strstr(text, "url(")) return NULL;
    GString *out = g_string_new(NULL);
    const char *p = text;
    gboolean changed = FALSE;
    while (*p) {
        const char *hit = strstr(p, "url(");
        if (!hit) { g_string_append(out, p); break; }
        const char *close = strchr(hit + 4, ')');
        if (!close) { g_string_append(out, p); break; }
        g_string_append_len(out, p, (gssize)(hit - p));
        const char *s = hit + 4;
        while (s < close && g_ascii_isspace(*s)) s++;
        const char *e = close;
        while (e > s && g_ascii_isspace(e[-1])) e--;
        if (e > s && (*s == '"' || *s == '\'')) {
            char q = *s;
            s++;
            if (e > s && e[-1] == q) e--;
        }
        char *rel = g_strndup(s, (gsize)(e - s));
        char *abs = css_url_should_resolve(rel)
            ? ns_url_resolve(base_url, rel) : NULL;
        if (abs && strcmp(abs, rel) != 0) {
            g_string_append(out, "url(\"");
            g_string_append(out, abs);
            g_string_append(out, "\")");
            changed = TRUE;
        } else {
            g_string_append_len(out, hit, (gssize)(close + 1 - hit));
        }
        g_free(abs);
        g_free(rel);
        p = close + 1;
    }
    if (!changed) {
        g_string_free(out, TRUE);
        return NULL;
    }
    return g_string_free(out, FALSE);
}

void
ns_css_stylesheet_resolve_urls(ns_css_stylesheet *s, const char *base_url)
{
    if (!s || !base_url) return;
    if (s->resolved_base && strcmp(s->resolved_base, base_url) == 0) return;
    g_free(s->resolved_base);
    s->resolved_base = g_strdup(base_url);
    if (s->rules) {
        for (guint ri = 0; ri < s->rules->len; ri++) {
            ns_css_rule *r = g_ptr_array_index(s->rules, ri);
            if (!r) continue;
            if (r->decls) {
                for (guint di = 0; di < r->decls->len; di++) {
                    ns_css_decl *d = &g_array_index(r->decls, ns_css_decl, di);
                    css_value_resolve_url(d->value, base_url);
                }
            }
            if (r->vars) {
                GHashTableIter it;
                gpointer k, v;
                g_hash_table_iter_init(&it, r->vars);
                while (g_hash_table_iter_next(&it, &k, &v)) {
                    char *resolved = css_raw_text_resolve_urls(v, base_url);
                    if (resolved) g_hash_table_iter_replace(&it, resolved);
                }
            }
            if (r->pending) {
                for (guint pi = 0; pi < r->pending->len; pi++) {
                    ns_css_pending_decl *pd =
                        &g_array_index(r->pending, ns_css_pending_decl, pi);
                    char *resolved =
                        css_raw_text_resolve_urls(pd->raw_vtext, base_url);
                    if (resolved) {
                        g_free(pd->raw_vtext);
                        pd->raw_vtext = resolved;
                    }
                }
            }
        }
    }
    if (s->font_faces) {
        for (guint i = 0; i < s->font_faces->len; i++) {
            ns_css_font_face *ff =
                &g_array_index(s->font_faces, ns_css_font_face, i);
            if (!css_url_should_resolve(ff->src_url)) continue;
            char *abs_url = ns_url_resolve(base_url, ff->src_url);
            if (!abs_url) continue;
            g_free(ff->src_url);
            ff->src_url = abs_url;
        }
    }
}

typedef struct css_candidate {
    guint rule_idx;
    guint selector_idx;
} css_candidate;

typedef enum css_index_kind {
    CSS_INDEX_NONE,
    CSS_INDEX_ID,
    CSS_INDEX_CLASS,
    CSS_INDEX_TAG,
    CSS_INDEX_ATTR,
} css_index_kind;

typedef struct css_index_counts {
    GHashTable *by_id;
    GHashTable *by_class;
    GHashTable *by_tag;
    GHashTable *by_attr;
} css_index_counts;

typedef struct ns_css_rule_index {
    GHashTable *by_id;
    GHashTable *by_class;
    GHashTable *by_tag;
    GHashTable *by_attr;
    GArray     *universal;
} ns_css_rule_index;

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

void
ns_css_stylesheet_force_layer(ns_css_stylesheet *s, const char *layer_name)
{
    if (!s || !layer_name || !*layer_name) return;
    s->serial = g_stylesheet_serial_next++;
    GPtrArray *old_names = s->layer_names;
    GHashTable *old_layers = s->layers;
    s->layer_names = NULL;
    s->layers = NULL;
    css_layer_register(s, layer_name);
    if (old_names) {
        for (guint i = 0; i < old_names->len; i++) {
            const char *old = g_ptr_array_index(old_names, i);
            char *full = css_layer_join(layer_name, old);
            css_layer_register(s, full);
            g_free(full);
        }
    }
    if (s->rules) {
        for (guint i = 0; i < s->rules->len; i++) {
            ns_css_rule *r = g_ptr_array_index(s->rules, i);
            char *full = r->layer_name ? css_layer_join(layer_name, r->layer_name)
                                       : g_strdup(layer_name);
            g_free(r->layer_name);
            r->layer_name = full;
        }
    }
    if (s->imports) {
        for (guint i = 0; i < s->imports->len; i++) {
            ns_css_import *im = &g_array_index(s->imports, ns_css_import, i);
            char *full = im->layer_name ? css_layer_join(layer_name, im->layer_name)
                                        : g_strdup(layer_name);
            g_free(im->layer_name);
            im->layer_name = full;
            css_layer_register(s, im->layer_name);
        }
    }
    if (old_layers) g_hash_table_destroy(old_layers);
    if (old_names) g_ptr_array_free(old_names, TRUE);
}

static void
free_bucket_array(gpointer data)
{
    g_array_free((GArray *)data, TRUE);
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

static void
index_add_candidate_array(GArray *bucket, guint rule_idx, guint selector_idx)
{
    css_candidate cand = { rule_idx, selector_idx };
    if (bucket->len > 0) {
        css_candidate last =
            g_array_index(bucket, css_candidate, bucket->len - 1);
        if (last.rule_idx == rule_idx && last.selector_idx == selector_idx)
            return;
    }
    g_array_append_val(bucket, cand);
}

static void
index_add(GHashTable *table, const char *key, guint rule_idx, guint selector_idx)
{
    GArray *bucket = g_hash_table_lookup(table, key);
    if (!bucket) {
        bucket = g_array_new(FALSE, FALSE, sizeof(css_candidate));
        g_hash_table_insert(table, g_strdup(key), bucket);
    }
    index_add_candidate_array(bucket, rule_idx, selector_idx);
}

static void
index_add_lowercase(GHashTable *table, const char *key, guint rule_idx,
                    guint selector_idx)
{
    char *lk = g_ascii_strdown(key, -1);
    GArray *bucket = g_hash_table_lookup(table, lk);
    if (!bucket) {
        bucket = g_array_new(FALSE, FALSE, sizeof(css_candidate));
        g_hash_table_insert(table, lk, bucket);
        lk = NULL;
    }
    if (bucket) index_add_candidate_array(bucket, rule_idx, selector_idx);
    g_free(lk);
}

static void
index_count_inc(GHashTable *table, const char *key)
{
    guint n = GPOINTER_TO_UINT(g_hash_table_lookup(table, key));
    g_hash_table_replace(table, g_strdup(key), GUINT_TO_POINTER(n + 1));
}

static void
index_count_inc_lowercase(GHashTable *table, const char *key)
{
    char *lk = g_ascii_strdown(key, -1);
    guint n = GPOINTER_TO_UINT(g_hash_table_lookup(table, lk));
    g_hash_table_replace(table, lk, GUINT_TO_POINTER(n + 1));
}

static guint
index_count_lookup_lowercase(GHashTable *table, const char *key)
{
    char *lk = g_ascii_strdown(key, -1);
    guint n = GPOINTER_TO_UINT(g_hash_table_lookup(table, lk));
    g_free(lk);
    return n;
}

static css_index_counts
index_counts_new(void)
{
    css_index_counts counts = {
        .by_id = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL),
        .by_class = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL),
        .by_tag = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL),
        .by_attr = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL),
    };
    return counts;
}

static void
index_counts_free(css_index_counts *counts)
{
    g_hash_table_destroy(counts->by_id);
    g_hash_table_destroy(counts->by_class);
    g_hash_table_destroy(counts->by_tag);
    g_hash_table_destroy(counts->by_attr);
}

static void
index_counts_add_subject(css_index_counts *counts, const ns_css_simple *subj)
{
    if (!counts || !subj || subj->never_match) return;
    if (subj->id && *subj->id)
        index_count_inc(counts->by_id, subj->id);
    for (guint i = 0; subj->classes && i < subj->classes->len; i++) {
        const char *cls = g_ptr_array_index(subj->classes, i);
        if (cls && *cls) index_count_inc(counts->by_class, cls);
    }
    if (subj->type && *subj->type && strcmp(subj->type, "*") != 0)
        index_count_inc_lowercase(counts->by_tag, subj->type);
    for (guint i = 0; subj->attrs && i < subj->attrs->len; i++) {
        const ns_css_attr_pred *a =
            &g_array_index(subj->attrs, ns_css_attr_pred, i);
        if (a && a->name && *a->name)
            index_count_inc_lowercase(counts->by_attr, a->name);
    }
}

static gboolean
index_choice_take(guint count, guint *best)
{
    if (count == 0 || count >= *best) return FALSE;
    *best = count;
    return TRUE;
}

static css_index_kind
index_subject_kind(const css_index_counts *counts,
                   const ns_css_simple *subj,
                   guint *out_class_i,
                   guint *out_attr_i)
{
    css_index_kind kind = CSS_INDEX_NONE;
    guint best = G_MAXUINT;
    if (out_class_i) *out_class_i = 0;
    if (out_attr_i) *out_attr_i = 0;
    if (!counts || !subj || subj->never_match) return kind;
    if (subj->id && *subj->id &&
        index_choice_take(GPOINTER_TO_UINT(g_hash_table_lookup(counts->by_id,
                                                               subj->id)),
                          &best)) {
        kind = CSS_INDEX_ID;
    }
    for (guint i = 0; subj->classes && i < subj->classes->len; i++) {
        const char *cls = g_ptr_array_index(subj->classes, i);
        if (!cls || !*cls) continue;
        guint count = GPOINTER_TO_UINT(g_hash_table_lookup(counts->by_class,
                                                           cls));
        if (index_choice_take(count, &best)) {
            kind = CSS_INDEX_CLASS;
            if (out_class_i) *out_class_i = i;
        }
    }
    if (subj->type && *subj->type && strcmp(subj->type, "*") != 0 &&
        index_choice_take(index_count_lookup_lowercase(counts->by_tag,
                                                       subj->type),
                          &best)) {
        kind = CSS_INDEX_TAG;
    }
    for (guint i = 0; subj->attrs && i < subj->attrs->len; i++) {
        const ns_css_attr_pred *a =
            &g_array_index(subj->attrs, ns_css_attr_pred, i);
        if (!a || !a->name || !*a->name) continue;
        if (index_choice_take(index_count_lookup_lowercase(counts->by_attr,
                                                           a->name),
                              &best)) {
            kind = CSS_INDEX_ATTR;
            if (out_attr_i) *out_attr_i = i;
        }
    }
    return kind;
}

static ns_css_rule_index *
ns_css_rule_index_build(const ns_css_stylesheet *sheet)
{
    ns_css_rule_index *idx = g_new0(ns_css_rule_index, 1);
    idx->by_id    = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, free_bucket_array);
    idx->by_class = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, free_bucket_array);
    idx->by_tag   = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, free_bucket_array);
    idx->by_attr  = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, free_bucket_array);
    idx->universal = g_array_new(FALSE, FALSE, sizeof(css_candidate));

    css_index_counts counts = index_counts_new();
    for (guint ri = 0; ri < sheet->rules->len; ri++) {
        const ns_css_rule *r = g_ptr_array_index(sheet->rules, ri);
        for (guint si = 0; si < r->selectors->len; si++) {
            const ns_css_selector *sel = g_ptr_array_index(r->selectors, si);
            if (!sel || sel->compounds->len == 0) continue;
            const ns_css_simple *subj =
                g_ptr_array_index(sel->compounds, sel->compounds->len - 1);
            index_counts_add_subject(&counts, subj);
        }
    }

    for (guint ri = 0; ri < sheet->rules->len; ri++) {
        const ns_css_rule *r = g_ptr_array_index(sheet->rules, ri);
        gboolean had_matchable_selector = FALSE;
        for (guint si = 0; si < r->selectors->len; si++) {
            const ns_css_selector *sel = g_ptr_array_index(r->selectors, si);
            if (sel && sel->pseudo_element != NS_CSS_PE_NONE) {
                ((ns_css_stylesheet *)sheet)->pseudo_mask |=
                    (1u << sel->pseudo_element);
                ((ns_css_rule *)r)->pe_mask |= (1u << sel->pseudo_element);
            }
            if (!sel || sel->compounds->len == 0) {
                index_add_candidate_array(idx->universal, ri, si);
                had_matchable_selector = TRUE;
                continue;
            }
            const ns_css_simple *subj =
                g_ptr_array_index(sel->compounds, sel->compounds->len - 1);
            if (!subj || subj->never_match) continue;
            had_matchable_selector = TRUE;
            guint class_i = 0, attr_i = 0;
            switch (index_subject_kind(&counts, subj, &class_i, &attr_i)) {
            case CSS_INDEX_ID:
                index_add(idx->by_id, subj->id, ri, si);
                continue;
            case CSS_INDEX_CLASS: {
                const char *cls = g_ptr_array_index(subj->classes, class_i);
                if (cls && *cls) {
                    index_add(idx->by_class, cls, ri, si);
                    continue;
                }
                break;
            }
            case CSS_INDEX_TAG:
                index_add_lowercase(idx->by_tag, subj->type, ri, si);
                continue;
            case CSS_INDEX_ATTR: {
                const ns_css_attr_pred *a0 =
                    &g_array_index(subj->attrs, ns_css_attr_pred, attr_i);
                if (a0 && a0->name && *a0->name) {
                    index_add_lowercase(idx->by_attr, a0->name, ri, si);
                    continue;
                }
                break;
            }
            case CSS_INDEX_NONE:
                break;
            }
            index_add_candidate_array(idx->universal, ri, si);
        }
        if (!had_matchable_selector) continue;
    }
    index_counts_free(&counts);
    return idx;
}

static const ns_css_rule_index *
ns_css_rule_index_ensure(const ns_css_stylesheet *sheet)
{
    if (!sheet) return NULL;
    if (!sheet->index)
        ((ns_css_stylesheet *)sheet)->index = ns_css_rule_index_build(sheet);
    return sheet->index;
}

static gboolean match_selector(const ns_css_selector *sel, const ns_node *el);
static const ns_node *g_css_match_scope;
typedef struct css_sibling_position {
    int child;
    int last_child;
    int of_type;
    int last_of_type;
} css_sibling_position;

static __thread GHashTable *g_sibling_positions;
static __thread GPtrArray  *g_sibling_position_blocks;
static __thread guint g_css_selector_batch_depth;

const ns_node *
ns_css_set_match_scope(const ns_node *scope)
{
    const ns_node *prev = g_css_match_scope;
    g_css_match_scope = scope;
    return prev;
}

void
ns_css_selector_batch_begin(void)
{
    if (g_css_selector_batch_depth++ > 0) return;
    g_sibling_positions = g_hash_table_new(g_direct_hash, g_direct_equal);
    g_sibling_position_blocks = g_ptr_array_new_with_free_func(g_free);
}

void
ns_css_selector_batch_end(void)
{
    if (g_css_selector_batch_depth == 0 || --g_css_selector_batch_depth > 0)
        return;
    g_clear_pointer(&g_sibling_positions, g_hash_table_destroy);
    g_clear_pointer(&g_sibling_position_blocks, g_ptr_array_unref);
}

static gboolean match_simple(const ns_css_simple *sel, const ns_node *el);

static gboolean
ns_input_is_text_entry(const ns_node *el)
{
    const char *type = ns_element_get_attr(el, "type");
    if (!type || !*type) return TRUE;
    return g_ascii_strcasecmp(type, "text") == 0 ||
           g_ascii_strcasecmp(type, "search") == 0 ||
           g_ascii_strcasecmp(type, "url") == 0 ||
           g_ascii_strcasecmp(type, "tel") == 0 ||
           g_ascii_strcasecmp(type, "email") == 0 ||
           g_ascii_strcasecmp(type, "password") == 0 ||
           g_ascii_strcasecmp(type, "number") == 0;
}

static gboolean
ns_el_is_read_write(const ns_node *el)
{
    if (!el->name) return FALSE;
    if (strcmp(el->name, "input") == 0)
        return ns_input_type_supports_readonly(ns_element_get_attr(el, "type")) &&
               !ns_element_get_attr(el, "readonly") &&
               !ns_element_effectively_disabled(el);
    if (strcmp(el->name, "textarea") == 0)
        return !ns_element_get_attr(el, "readonly") &&
               !ns_element_effectively_disabled(el);
    const char *ce = ns_element_get_attr(el, "contenteditable");
    if (ce && (!*ce || g_ascii_strcasecmp(ce, "true") == 0 ||
               g_ascii_strcasecmp(ce, "plaintext-only") == 0))
        return TRUE;
    return FALSE;
}

static gboolean
ns_el_placeholder_shown(const ns_node *el)
{
    if (!el->name) return FALSE;
    const char *ph = ns_element_get_attr(el, "placeholder");
    if (!ph) return FALSE;
    if (strcmp(el->name, "input") == 0) {
        if (!ns_input_is_text_entry(el)) return FALSE;
        const char *v = ns_element_get_attr(el, "value");
        return !v || !*v;
    }
    if (strcmp(el->name, "textarea") == 0) {
        char *txt = ns_node_collect_text(el);
        gboolean empty = TRUE;
        if (txt) {
            for (const char *q = txt; *q; q++)
                if (!is_ws(*q)) { empty = FALSE; break; }
            g_free(txt);
        }
        return empty;
    }
    return FALSE;
}

static gboolean
ns_el_is_checked(const ns_node *el)
{
    if (ns_node_is_element_named(el, "option")) {
        if (ns_element_get_attr(el, "selected")) return TRUE;
        const ns_node *sel = el->parent;
        if (ns_node_is_element_named(sel, "optgroup")) sel = sel->parent;
        return ns_node_is_element_named(sel, "select") &&
               !ns_element_get_attr(sel, "multiple") &&
               ns_select_chosen_option(sel) == el;
    }
    if (!ns_node_is_element_named(el, "input"))
        return FALSE;
    const char *type = ns_element_get_attr(el, "type");
    if (!type || (g_ascii_strcasecmp(type, "checkbox") != 0 &&
                  g_ascii_strcasecmp(type, "radio") != 0))
        return FALSE;
    return ns_input_is_checked(el);
}

static gboolean
ns_el_is_submit_button(const ns_node *el)
{
    if (ns_node_is_element_named(el, "input")) {
        const char *type = ns_element_get_attr(el, "type");
        return type && (g_ascii_strcasecmp(type, "submit") == 0 ||
                        g_ascii_strcasecmp(type, "image") == 0);
    }
    if (!ns_node_is_element_named(el, "button")) return FALSE;
    const char *type = ns_element_get_attr(el, "type");
    return !type || !*type ||
           g_ascii_strcasecmp(type, "submit") == 0 ||
           g_ascii_strcasecmp(type, "auto") == 0;
}

static const ns_node *
ns_css_first_submit_button_for(const ns_node *scan, const ns_node *doc,
                               const ns_node *owner, int depth)
{
    if (!scan || depth >= 512) return NULL;
    if (scan->kind == NS_NODE_ELEMENT &&
        ns_el_is_submit_button(scan) &&
        !ns_element_effectively_disabled(scan) &&
        ns_form_owner(scan, doc) == owner)
        return scan;
    if (ns_node_is_element_named(scan, "template")) return NULL;
    for (const ns_node *c = scan->first_child; c; c = c->next_sibling) {
        const ns_node *hit =
            ns_css_first_submit_button_for(c, doc, owner, depth + 1);
        if (hit) return hit;
    }
    return NULL;
}

static gboolean
ns_el_is_default(const ns_node *el)
{
    if (ns_node_is_element_named(el, "option"))
        return ns_element_get_attr(el, "selected") != NULL;
    if (ns_node_is_element_named(el, "input")) {
        const char *type = ns_element_get_attr(el, "type");
        if (type && (g_ascii_strcasecmp(type, "checkbox") == 0 ||
                     g_ascii_strcasecmp(type, "radio") == 0))
            return ns_element_get_attr(el, "checked") != NULL;
    }
    if (!ns_el_is_submit_button(el)) return FALSE;
    const ns_node *doc = ns_node_root(el);
    const ns_node *owner = ns_form_owner(el, doc);
    if (!owner) return FALSE;
    return ns_css_first_submit_button_for(doc ? doc : owner, doc, owner, 0) == el;
}

static gboolean
ns_css_radio_group_has_checked(const ns_node *scan, const ns_node *doc,
                               const ns_node *owner, const char *name,
                               int depth)
{
    if (!scan || depth >= 512) return FALSE;
    if (ns_node_is_element_named(scan, "input")) {
        const char *type = ns_element_get_attr(scan, "type");
        if (type && g_ascii_strcasecmp(type, "radio") == 0) {
            const char *scan_name = ns_element_get_attr(scan, "name");
            if (!scan_name) scan_name = "";
            if (strcmp(scan_name, name) == 0 &&
                ns_form_owner(scan, doc) == owner &&
                ns_input_is_checked(scan))
                return TRUE;
        }
    }
    if (ns_node_is_element_named(scan, "template")) return FALSE;
    for (const ns_node *c = scan->first_child; c; c = c->next_sibling)
        if (ns_css_radio_group_has_checked(c, doc, owner, name, depth + 1))
            return TRUE;
    return FALSE;
}

static gboolean
ns_el_is_indeterminate(const ns_node *el)
{
    if (ns_node_is_element_named(el, "progress"))
        return ns_element_get_attr(el, "value") == NULL;
    if (!ns_node_is_element_named(el, "input")) return FALSE;
    const char *type = ns_element_get_attr(el, "type");
    if (!type || g_ascii_strcasecmp(type, "radio") != 0) return FALSE;
    const char *name = ns_element_get_attr(el, "name");
    if (!name) name = "";
    const ns_node *doc = ns_node_root(el);
    const ns_node *owner = ns_form_owner(el, doc);
    return !ns_css_radio_group_has_checked(doc ? doc : el, doc, owner, name, 0);
}

static gboolean
ns_el_range_state(const ns_node *el, gboolean *under, gboolean *over)
{
    if (under) *under = FALSE;
    if (over) *over = FALSE;
    if (!ns_node_is_element_named(el, "input")) return FALSE;
    const char *type = ns_element_get_attr(el, "type");
    if (!ns_input_type_has_number_value(type)) return FALSE;
    if (!ns_element_get_attr(el, "min") && !ns_element_get_attr(el, "max"))
        return FALSE;
    const char *value = ns_element_get_attr(el, "value");
    if (!value || !*value) return FALSE;
    return ns_input_value_range_state(el, value, under, over);
}

static gboolean
ns_el_is_blank(const ns_node *el)
{
    if (ns_node_is_element_named(el, "input")) {
        if (!ns_input_is_text_entry(el)) return FALSE;
        const char *value = ns_element_get_attr(el, "value");
        return !value || !*value;
    }
    if (!ns_node_is_element_named(el, "textarea")) return FALSE;
    char *txt = ns_node_collect_text(el);
    gboolean blank = TRUE;
    for (const char *p = txt ? txt : ""; *p; p++) {
        if (!is_ws(*p)) {
            blank = FALSE;
            break;
        }
    }
    g_free(txt);
    return blank;
}

static gboolean
ns_el_is_empty(const ns_node *el)
{
    for (const ns_node *c = el ? el->first_child : NULL; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT) return FALSE;
        if (c->kind == NS_NODE_TEXT && c->text && c->text[0] != '\0')
            return FALSE;
    }
    return TRUE;
}

static gboolean
ns_el_is_link(const ns_node *el)
{
    if (!ns_element_get_attr(el, "href")) return FALSE;
    return ns_node_is_element_named(el, "a") ||
           ns_node_is_element_named(el, "area");
}

static GHashTable *g_visited_urls = NULL;
static char       *g_css_doc_base = NULL;
static char       *g_css_doc_language = NULL;

void
ns_css_mark_visited(const char *abs_url)
{
    if (!abs_url || !*abs_url) return;
    if (!g_visited_urls)
        g_visited_urls = g_hash_table_new_full(g_str_hash, g_str_equal,
                                               g_free, NULL);
    if (!g_hash_table_contains(g_visited_urls, abs_url))
        g_hash_table_add(g_visited_urls, g_strdup(abs_url));
}

void
ns_css_set_doc_base(const char *base_url)
{
    g_free(g_css_doc_base);
    g_css_doc_base = (base_url && *base_url) ? g_strdup(base_url) : NULL;
}

void
ns_css_set_doc_language(const char *lang)
{
    g_free(g_css_doc_language);
    g_css_doc_language = (lang && *lang) ? g_strdup(lang) : NULL;
}

static gboolean
ns_el_is_visited_link(const ns_node *el)
{
    if (!g_visited_urls || !g_css_doc_base || !ns_el_is_link(el)) return FALSE;
    const char *href = ns_element_get_attr(el, "href");
    if (!href || !*href) return FALSE;
    char *abs_url = ns_url_resolve(g_css_doc_base, href);
    if (!abs_url) return FALSE;
    gboolean v = g_hash_table_contains(g_visited_urls, abs_url);
    g_free(abs_url);
    return v;
}

static gboolean
selector_group_matches_element(const GPtrArray *group, const ns_node *el)
{
    for (guint i = 0; group && i < group->len; i++) {
        const ns_css_selector *sub = g_ptr_array_index(group, i);
        if (match_selector(sub, el)) return TRUE;
    }
    return FALSE;
}

static void
css_sibling_positions_fill(const ns_node *parent)
{
    guint n = 0;
    for (const ns_node *c = parent->first_child; c; c = c->next_sibling)
        if (c->kind == NS_NODE_ELEMENT) n++;
    if (n == 0) return;
    css_sibling_position *block = g_new0(css_sibling_position, n);
    g_ptr_array_add(g_sibling_position_blocks, block);
    GHashTable *type_counts = g_hash_table_new(g_str_hash, g_str_equal);
    guint i = 0;
    for (const ns_node *c = parent->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT) continue;
        css_sibling_position *pos = &block[i++];
        pos->child = (int)i;
        pos->last_child = (int)(n - i + 1);
        pos->of_type = 1;
        if (c->name) {
            guint seen = GPOINTER_TO_UINT(
                g_hash_table_lookup(type_counts, c->name)) + 1;
            g_hash_table_insert(type_counts, c->name, GUINT_TO_POINTER(seen));
            pos->of_type = (int)seen;
        }
        g_hash_table_insert(g_sibling_positions, (gpointer)c, pos);
    }
    i = 0;
    for (const ns_node *c = parent->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT) continue;
        css_sibling_position *pos = &block[i++];
        pos->last_of_type = c->name
            ? (int)GPOINTER_TO_UINT(g_hash_table_lookup(type_counts, c->name))
                  - pos->of_type + 1
            : 1;
    }
    g_hash_table_destroy(type_counts);
}

static const css_sibling_position *
css_sibling_position_of(const ns_node *el)
{
    if (!g_sibling_positions || !el->parent) return NULL;
    const css_sibling_position *pos =
        g_hash_table_lookup(g_sibling_positions, el);
    if (pos) return pos;
    css_sibling_positions_fill(el->parent);
    return g_hash_table_lookup(g_sibling_positions, el);
}

static gboolean
ns_css_sibling_counts_for_nth(const ns_node *el, const ns_css_pseudo_pred *pc,
                              int *idx_out)
{
    if (!pc->of_group) {
        const css_sibling_position *pos = css_sibling_position_of(el);
        if (pos) {
            switch (pc->kind) {
            case NS_CSS_PC_NTH_CHILD:        *idx_out = pos->child; break;
            case NS_CSS_PC_NTH_LAST_CHILD:   *idx_out = pos->last_child; break;
            case NS_CSS_PC_NTH_OF_TYPE:      *idx_out = pos->of_type; break;
            default:                         *idx_out = pos->last_of_type; break;
            }
            return TRUE;
        }
    }
    int idx = 1;
    gboolean reverse = pc->kind == NS_CSS_PC_NTH_LAST_CHILD ||
                       pc->kind == NS_CSS_PC_NTH_LAST_OF_TYPE;
    gboolean typed = pc->kind == NS_CSS_PC_NTH_OF_TYPE ||
                     pc->kind == NS_CSS_PC_NTH_LAST_OF_TYPE;
    const ns_node *s = reverse ? el->next_sibling : el->prev_sibling;
    while (s) {
        if (s->kind == NS_NODE_ELEMENT &&
            (!typed || (el->name && ns_node_is_element_named(s, el->name))) &&
            (!pc->of_group || selector_group_matches_element(pc->of_group, s)))
            idx++;
        s = reverse ? s->next_sibling : s->prev_sibling;
    }
    if (pc->of_group && !selector_group_matches_element(pc->of_group, el))
        return FALSE;
    *idx_out = idx;
    return TRUE;
}

static __thread const ns_node *g_pragma_doc;
static __thread const char    *g_pragma_lang;
static __thread gboolean       g_pragma_valid;

static void
ns_css_pragma_language_scan(const ns_node *n, const char **found, int depth)
{
    if (!n || depth >= 512) return;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            g_ascii_strcasecmp(c->name, "meta") == 0) {
            const char *he = ns_element_get_attr(c, "http-equiv");
            const char *content = he &&
                g_ascii_strcasecmp(he, "content-language") == 0
                ? ns_element_get_attr(c, "content") : NULL;
            if (content && !strchr(content, ',')) {
                const char *s = content;
                while (*s && g_ascii_isspace((guchar)*s)) s++;
                const char *e = s;
                while (*e && !g_ascii_isspace((guchar)*e)) e++;
                if (e > s) {
                    static __thread char buf[128];
                    gsize len = (gsize)(e - s);
                    if (len >= sizeof buf) len = sizeof buf - 1;
                    memcpy(buf, s, len);
                    buf[len] = '\0';
                    *found = buf;
                }
            }
        }
        ns_css_pragma_language_scan(c, found, depth + 1);
    }
}

static const char *
ns_css_node_language(const ns_node *el)
{
    static const char xml_ns[] = "http://www.w3.org/XML/1998/namespace";
    for (const ns_node *n = el; n; n = n->parent) {
        if (n->kind != NS_NODE_ELEMENT) continue;
        const ns_attr *xa = ns_element_find_attr_ns(n, xml_ns, "lang");
        if (xa) return xa->value ? xa->value : "";
        const ns_attr *la = ns_element_find_attr_ns(n, NULL, "lang");
        if (la && !(n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS)))
            return la->value ? la->value : "";
    }
    const ns_node *root = el;
    while (root && root->parent) root = root->parent;
    if (!g_pragma_valid || g_pragma_doc != root) {
        const char *found = NULL;
        if (root) ns_css_pragma_language_scan(root, &found, 0);
        g_pragma_doc = root;
        g_pragma_lang = found;
        g_pragma_valid = TRUE;
    }
    return g_pragma_lang ? g_pragma_lang : g_css_doc_language;
}

static gboolean
ns_css_lang_one_matches(const char *lang, const char *want)
{
    if (!lang || !want || !*want) return FALSE;
    while (*want == ' ' || *want == '\'' || *want == '"') want++;
    gsize wlen = strlen(want);
    while (wlen > 0 && (is_ws(want[wlen - 1]) ||
                        want[wlen - 1] == '\'' || want[wlen - 1] == '"'))
        wlen--;
    if (wlen == 0) return FALSE;
    if (wlen == 1 && want[0] == '*') return TRUE;
    if (want[0] == '*' && want[1] == '-') {
        const char *needle = want + 2;
        gsize nlen = wlen - 2;
        const char *p = lang;
        while ((p = strchr(p, '-')) != NULL) {
            p++;
            if (g_ascii_strncasecmp(p, needle, nlen) == 0 &&
                (p[nlen] == '\0' || p[nlen] == '-'))
                return TRUE;
        }
        return FALSE;
    }
    if (g_ascii_strncasecmp(lang, want, wlen) != 0) return FALSE;
    return lang[wlen] == '\0' || lang[wlen] == '-';
}

static gboolean
ns_css_lang_matches(const ns_node *el, const char *arg)
{
    const char *lang = ns_css_node_language(el);
    if (!lang || !arg) return FALSE;
    const char *p = arg;
    const char *end = arg + strlen(arg);
    while (p < end) {
        char term = 0;
        const char *seg = css_scan_until(p, end, ",", &term);
        char *want = css_trim_dup_range(p, seg);
        gboolean ok = ns_css_lang_one_matches(lang, want);
        g_free(want);
        if (ok) return TRUE;
        p = term == ',' ? seg + 1 : seg;
    }
    return FALSE;
}

static gboolean
ns_dir_is_rtl_script(GUnicodeScript s)
{
    switch (s) {
    case G_UNICODE_SCRIPT_HEBREW:
    case G_UNICODE_SCRIPT_ARABIC:
    case G_UNICODE_SCRIPT_SYRIAC:
    case G_UNICODE_SCRIPT_THAANA:
    case G_UNICODE_SCRIPT_NKO:
    case G_UNICODE_SCRIPT_SAMARITAN:
    case G_UNICODE_SCRIPT_MANDAIC:
        return TRUE;
    default:
        return FALSE;
    }
}

static const char *
ns_dir_first_strong(const ns_node *n, int depth)
{
    if (!n || depth > 256) return NULL;
    if (n->kind == NS_NODE_TEXT && n->text) {
        for (const char *p = n->text; *p; p = g_utf8_next_char(p)) {
            gunichar c = g_utf8_get_char(p);
            if (ns_dir_is_rtl_script(g_unichar_get_script(c))) return "rtl";
            if (g_unichar_isalpha(c)) return "ltr";
        }
        return NULL;
    }
    if (n->kind != NS_NODE_ELEMENT) return NULL;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        gboolean html_ns = c->kind == NS_NODE_ELEMENT &&
            !(c->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS));
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            ((html_ns &&
              (g_ascii_strcasecmp(c->name, "script") == 0 ||
               g_ascii_strcasecmp(c->name, "style") == 0 ||
               g_ascii_strcasecmp(c->name, "textarea") == 0 ||
               g_ascii_strcasecmp(c->name, "bdi") == 0)) ||
             ns_element_get_attr(c, "dir")))
            continue;
        const char *d = ns_dir_first_strong(c, depth + 1);
        if (d) return d;
    }
    return NULL;
}

static const char *
ns_dir_first_strong_str(const char *s)
{
    if (!s) return NULL;
    for (const char *p = s; *p; p = g_utf8_next_char(p)) {
        gunichar c = g_utf8_get_char(p);
        if (ns_dir_is_rtl_script(g_unichar_get_script(c))) return "rtl";
        if (g_unichar_isalpha(c)) return "ltr";
    }
    return NULL;
}

static const char *
ns_dir_form_control_value(const ns_node *n)
{
    if (!n->name) return NULL;
    if (g_ascii_strcasecmp(n->name, "textarea") == 0)
        return ns_node_editable_value(n);
    if (g_ascii_strcasecmp(n->name, "input") != 0) return NULL;
    const char *type = ns_element_get_attr(n, "type");
    if (type) {
        static const char *const uses[] = { "hidden", "text", "search", "tel",
            "url", "email", "password", "submit", "reset", "button", NULL };
        gboolean ok = FALSE;
        for (int i = 0; uses[i]; i++)
            if (g_ascii_strcasecmp(type, uses[i]) == 0) { ok = TRUE; break; }
        if (!ok) return NULL;
    }
    return ns_node_editable_value(n);
}

static const char *
ns_dir_auto_resolve(const ns_node *n)
{
    const char *val = ns_dir_form_control_value(n);
    if (val) {
        const char *d = ns_dir_first_strong_str(val);
        return d ? d : "ltr";
    }
    const char *d = ns_dir_first_strong(n, 0);
    return d ? d : "ltr";
}

const char *
ns_css_node_dir(const ns_node *el)
{
    for (const ns_node *n = el; n; n = n->parent) {
        if (n->kind != NS_NODE_ELEMENT) continue;
        const char *dir = ns_element_get_attr(n, "dir");
        gboolean is_bdi = n->name && g_ascii_strcasecmp(n->name, "bdi") == 0;
        if (dir) {
            if (g_ascii_strcasecmp(dir, "ltr") == 0) return "ltr";
            if (g_ascii_strcasecmp(dir, "rtl") == 0) return "rtl";
            if (g_ascii_strcasecmp(dir, "auto") == 0)
                return ns_dir_auto_resolve(n);
        } else if (is_bdi) {
            const char *d = ns_dir_first_strong(n, 0);
            return d ? d : "ltr";
        }
        if (n == el && n->name && g_ascii_strcasecmp(n->name, "input") == 0) {
            const char *type = ns_element_get_attr(n, "type");
            if (type && g_ascii_strcasecmp(type, "tel") == 0) return "ltr";
        }
    }
    return "ltr";
}

static gboolean
ns_css_node_is_target(const ns_node *el)
{
    if (!g_target_fragment || !el) return FALSE;
    const char *eid = ns_element_get_attr(el, "id");
    if (eid && strcmp(eid, g_target_fragment) == 0) return TRUE;
    if (el->name && g_ascii_strcasecmp(el->name, "a") == 0) {
        const char *nm = ns_element_get_attr(el, "name");
        if (nm && strcmp(nm, g_target_fragment) == 0) return TRUE;
    }
    return FALSE;
}

static gboolean
ns_css_node_has_target_within(const ns_node *el, int depth)
{
    if (!el || depth >= 512) return FALSE;
    if (el->kind == NS_NODE_ELEMENT && ns_css_node_is_target(el))
        return TRUE;
    if (ns_node_is_element_named(el, "template")) return FALSE;
    for (const ns_node *c = el->first_child; c; c = c->next_sibling)
        if (ns_css_node_has_target_within(c, depth + 1))
            return TRUE;
    return FALSE;
}

static gboolean
ns_css_value_matches_pattern(const char *value, const char *pattern)
{
    if (!pattern || !*pattern) return TRUE;
    char *anchored = g_strdup_printf("^(?:%s)$", pattern);
    GError *err = NULL;
    GRegex *re = g_regex_new(anchored, 0, 0, &err);
    g_free(anchored);
    if (!re) { g_clear_error(&err); return TRUE; }
    gboolean ok = g_regex_match(re, value ? value : "", 0, NULL);
    g_regex_unref(re);
    return ok;
}

static gboolean
ns_css_node_will_validate(const ns_node *el)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !el->name) return FALSE;
    gboolean is_input = strcmp(el->name, "input") == 0;
    if (!is_input &&
        strcmp(el->name, "textarea") != 0 &&
        strcmp(el->name, "select") != 0)
        return FALSE;
    if (ns_element_effectively_disabled(el)) return FALSE;
    if (ns_form_control_readonly_bars_validation(el)) return FALSE;
    const char *type = is_input ? ns_element_get_attr(el, "type") : NULL;
    if (type && (g_ascii_strcasecmp(type, "submit") == 0 ||
                 g_ascii_strcasecmp(type, "button") == 0 ||
                 g_ascii_strcasecmp(type, "reset")  == 0 ||
                 g_ascii_strcasecmp(type, "image")  == 0 ||
                 g_ascii_strcasecmp(type, "hidden") == 0))
        return FALSE;
    return TRUE;
}

static char *
ns_css_control_value_dup(const ns_node *el)
{
    if (!el || !el->name) return g_strdup("");
    if (strcmp(el->name, "textarea") == 0)
        return ns_node_collect_text(el);
    if (strcmp(el->name, "select") == 0) {
        const ns_node *opt = ns_element_get_attr(el, "multiple")
            ? ns_select_first_selected_option(el)
            : ns_select_chosen_option(el);
        return opt ? ns_option_value_dup(opt) : g_strdup("");
    }
    return g_strdup(ns_element_get_attr(el, "value") ?
                    ns_element_get_attr(el, "value") : "");
}

static gboolean
ns_css_control_is_valid(const ns_node *el)
{
    if (!ns_css_node_will_validate(el)) return FALSE;
    const char *custom = ns_element_get_attr(el, NS_CUSTOM_VALIDITY_ATTR);
    if (custom && *custom) return FALSE;
    char *owned = ns_css_control_value_dup(el);
    const char *value = owned ? owned : "";
    gboolean valid = TRUE;
    const char *type = el->name && strcmp(el->name, "input") == 0
        ? ns_element_get_attr(el, "type") : NULL;
    if (ns_form_control_supports_required(el) &&
        ns_element_get_attr(el, "required") &&
        ns_form_control_value_missing(el, value, ns_node_root(el)))
        valid = FALSE;
    if (valid && *value && type) {
        if (g_ascii_strcasecmp(type, "email") == 0) {
            if (!ns_input_email_value_valid(el, value))
                valid = FALSE;
        } else if (g_ascii_strcasecmp(type, "url") == 0) {
            if (!ns_url_is_valid_absolute(value))
                valid = FALSE;
        } else if (ns_input_type_has_number_value(type)) {
            double parsed;
            if (!ns_input_value_to_number(type, value, &parsed)) valid = FALSE;
        }
        if (valid) {
            gboolean under = FALSE, over = FALSE;
            if (ns_input_value_range_state(el, value, &under, &over) &&
                (under || over))
                valid = FALSE;
        }
        if (valid && ns_input_value_step_mismatch(el, value))
            valid = FALSE;
    }
    if (valid && *value &&
        el->name && strcmp(el->name, "input") == 0 &&
        ns_input_type_supports_text_constraints(type) &&
        !ns_css_value_matches_pattern(value, ns_element_get_attr(el, "pattern")))
        valid = FALSE;
    if (valid && *value && ns_form_control_length_limits_apply(el)) {
        glong vlen = (glong)g_utf8_strlen(value, -1);
        const char *minlen = ns_element_get_attr(el, "minlength");
        const char *maxlen = ns_element_get_attr(el, "maxlength");
        if (minlen && vlen < (glong)ns_parse_int(minlen, 0, 0, 1000000))
            valid = FALSE;
        if (maxlen && vlen > (glong)ns_parse_int(maxlen, 0, 0, 1000000))
            valid = FALSE;
    }
    g_free(owned);
    return valid;
}

static gboolean
has_simple_scope_pseudo(const ns_css_simple *sel)
{
    for (guint i = 0; sel && sel->pseudos && i < sel->pseudos->len; i++) {
        const ns_css_pseudo_pred *pc =
            &g_array_index(sel->pseudos, ns_css_pseudo_pred, i);
        if (pc->kind == NS_CSS_PC_SCOPE) return TRUE;
    }
    return FALSE;
}

static const ns_node *
next_element_sibling(const ns_node *n)
{
    const ns_node *s = n ? n->next_sibling : NULL;
    while (s && s->kind != NS_NODE_ELEMENT) s = s->next_sibling;
    return s;
}

static gboolean
relative_chain_matches(const ns_css_selector *rel, const ns_node *anchor,
                       const ns_node *base, guint idx, int depth);

static gboolean
relative_try_candidate(const ns_css_selector *rel, const ns_node *anchor,
                       const ns_node *candidate, guint idx, int depth)
{
    if (!candidate || candidate->kind != NS_NODE_ELEMENT) return FALSE;
    const ns_css_simple *cmp = g_ptr_array_index(rel->compounds, idx);
    if (!match_simple(cmp, candidate)) return FALSE;
    return relative_chain_matches(rel, anchor, candidate, idx + 1, depth + 1);
}

static gboolean
relative_descendant_matches(const ns_css_selector *rel, const ns_node *anchor,
                            const ns_node *base, guint idx, int depth)
{
    if (depth >= 512) return FALSE;
    for (const ns_node *c = base ? base->first_child : NULL; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT) continue;
        if (relative_try_candidate(rel, anchor, c, idx, depth)) return TRUE;
        if (relative_descendant_matches(rel, anchor, c, idx, depth + 1))
            return TRUE;
    }
    return FALSE;
}

static gboolean
relative_chain_matches(const ns_css_selector *rel, const ns_node *anchor,
                       const ns_node *base, guint idx, int depth)
{
    if (depth >= 512) return FALSE;
    if (!rel || idx >= rel->compounds->len) return TRUE;
    const ns_css_simple *cmp = g_ptr_array_index(rel->compounds, idx);
    ns_css_comb comb = g_array_index(rel->combinators, ns_css_comb, idx);
    if (idx == 0 && (comb == NS_CSS_COMB_NONE ||
                     comb == NS_CSS_COMB_DESCENDANT)) {
        if (has_simple_scope_pseudo(cmp) &&
            relative_try_candidate(rel, anchor, anchor, idx, depth))
            return TRUE;
        return relative_descendant_matches(rel, anchor, anchor, idx, depth + 1);
    }
    if (comb == NS_CSS_COMB_CHILD) {
        for (const ns_node *c = base ? base->first_child : NULL; c; c = c->next_sibling)
            if (relative_try_candidate(rel, anchor, c, idx, depth))
                return TRUE;
        return FALSE;
    }
    if (comb == NS_CSS_COMB_ADJACENT)
        return relative_try_candidate(rel, anchor, next_element_sibling(base),
                                      idx, depth);
    if (comb == NS_CSS_COMB_SIBLING) {
        for (const ns_node *s = next_element_sibling(base); s; s = next_element_sibling(s))
            if (relative_try_candidate(rel, anchor, s, idx, depth))
                return TRUE;
        return FALSE;
    }
    return relative_descendant_matches(rel, anchor, base, idx, depth + 1);
}

static gboolean
has_relative_matches(const ns_css_selector *rel, const ns_node *anchor)
{
    if (!rel || rel->pseudo_element != NS_CSS_PE_NONE) return FALSE;
    const ns_node *prev_scope = g_css_match_scope;
    if (!g_css_match_scope) g_css_match_scope = anchor;
    gboolean matched = relative_chain_matches(rel, anchor, anchor, 0, 0);
    g_css_match_scope = prev_scope;
    return matched;
}

typedef struct has_memo_key {
    const void *group;
    const void *anchor;
} has_memo_key;

static guint
has_memo_hash(gconstpointer p)
{
    const has_memo_key *k = p;
    guintptr x = (guintptr)k->group * 2654435761u ^
                 (guintptr)k->anchor * 0x9E3779B9u;
    return (guint)(x ^ (x >> 16));
}

static gboolean
has_memo_equal(gconstpointer pa, gconstpointer pb)
{
    const has_memo_key *a = pa, *b = pb;
    return a->group == b->group && a->anchor == b->anchor;
}

static GHashTable *g_has_memo;

static gboolean
has_group_matches(const GPtrArray *group, const ns_node *anchor)
{
    has_memo_key probe = { group, anchor };
    gpointer cached;
    if (g_has_memo &&
        g_hash_table_lookup_extended(g_has_memo, &probe, NULL, &cached))
        return GPOINTER_TO_INT(cached) != 0;
    gboolean matched = FALSE;
    for (guint j = 0; j < group->len && !matched; j++) {
        const ns_css_selector *sub = g_ptr_array_index(group, j);
        if (has_relative_matches(sub, anchor)) matched = TRUE;
    }
    if (g_has_memo)
        g_hash_table_replace(g_has_memo,
                             g_memdup2(&probe, sizeof probe),
                             GINT_TO_POINTER(matched ? 1 : 0));
    return matched;
}

static gboolean
ns_css_html_ci_attr(const char *name)
{
    static const char *const list[] = {
        "accept", "accept-charset", "align", "alink", "axis", "bgcolor",
        "charset", "checked", "clear", "codetype", "color", "compact",
        "declare", "defer", "dir", "direction", "disabled", "enctype",
        "face", "frame", "hreflang", "http-equiv", "lang", "language",
        "link", "media", "method", "multiple", "nohref", "noresize",
        "noshade", "nowrap", "readonly", "rel", "rev", "rules", "scope",
        "scrolling", "selected", "shape", "target", "text", "type",
        "valign", "valuetype", "vlink",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(list); i++)
        if (g_ascii_strcasecmp(name, list[i]) == 0) return TRUE;
    return FALSE;
}

static inline gboolean
css_name_equals_lower(const char *name, const char *lower)
{
    for (;; name++, lower++) {
        unsigned char c = (unsigned char)*name;
        if (c >= 'A' && c <= 'Z') c = (unsigned char)(c + ('a' - 'A'));
        if (c != (unsigned char)*lower) return FALSE;
        if (!c) return TRUE;
    }
}

static gboolean
match_simple(const ns_css_simple *sel, const ns_node *el)
{
    if (sel->never_match) return FALSE;
    if (!el || el->kind != NS_NODE_ELEMENT) return FALSE;
    if (sel->ns_none) {
        gboolean null_ns = (el->flags & NS_NODE_FOREIGN_NS) &&
                           !(el->flags & NS_NODE_SVG_NS) &&
                           !ns_element_get_attr(el, "data-nd-ns-uri");
        if (!null_ns) return FALSE;
    }
    if (sel->type && !(sel->type[0] == '*' && sel->type[1] == '\0')) {
        if (!el->name) return FALSE;
        if (el->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS)) {
            if (strcmp(sel->type, el->name) != 0) return FALSE;
        }
        else if (!css_name_equals_lower(el->name, sel->type)) {
            return FALSE;
        }
    }
    if (sel->id) {
        const char *id = ns_element_get_attr(el, "id");
        if (!id || strcmp(id, sel->id) != 0) return FALSE;
    }
    if (sel->classes->len > 0) {
        for (guint i = 0; i < sel->classes->len; i++) {
            const char *want = g_ptr_array_index(sel->classes, i);
            gsize want_len = sel->class_lens && i < sel->class_lens->len
                ? g_array_index(sel->class_lens, gsize, i)
                : strlen(want);
            if (!ns_node_has_class(el, want, want_len))
                return FALSE;
        }
    }
    if (sel->attrs && sel->attrs->len > 0) {
        guint64 elbloom = ns_node_attr_bloom(el);
        gboolean html_doc = !(el->flags & NS_NODE_XML_DOC) &&
                             !(el->flags & (NS_NODE_FOREIGN_NS | NS_NODE_SVG_NS));
        for (guint i = 0; i < sel->attrs->len; i++) {
            const ns_css_attr_pred *a = &g_array_index(sel->attrs, ns_css_attr_pred, i);
            if (a->name_bit && (elbloom & a->name_bit) == 0) return FALSE;
            const char *v = ns_element_get_attr(el, a->name);
            if (a->op == NS_CSS_ATTR_PRESENT) {
                if (!v) return FALSE;
            } else {
                if (!v || !a->value) return FALSE;
                gsize vl = strlen(v), wl = strlen(a->value);
                gboolean ci = a->case_insensitive ||
                    (!a->case_sensitive && html_doc &&
                     a->html_ci);
                switch (a->op) {
                case NS_CSS_ATTR_EQ:
                    if (ci ? g_ascii_strcasecmp(v, a->value)
                           : strcmp(v, a->value)) return FALSE;
                    break;
                case NS_CSS_ATTR_PREFIX:
                    if (wl == 0 || vl < wl) return FALSE;
                    if (ci ? g_ascii_strncasecmp(v, a->value, wl)
                           : strncmp(v, a->value, wl)) return FALSE;
                    break;
                case NS_CSS_ATTR_SUFFIX:
                    if (wl == 0 || vl < wl) return FALSE;
                    if (ci ? g_ascii_strcasecmp(v + vl - wl, a->value)
                           : strcmp(v + vl - wl, a->value)) return FALSE;
                    break;
                case NS_CSS_ATTR_SUBSTR:
                    if (wl == 0) return FALSE;
                    if (ci) {
                        gboolean found = FALSE;
                        for (gsize i2 = 0; i2 + wl <= vl; i2++) {
                            if (g_ascii_strncasecmp(v + i2, a->value, wl) == 0) {
                                found = TRUE; break;
                            }
                        }
                        if (!found) return FALSE;
                    } else {
                        if (!strstr(v, a->value)) return FALSE;
                    }
                    break;
                case NS_CSS_ATTR_WORD: {
                    gboolean found = FALSE;
                    const char *s = v;
                    while (*s) {
                        while (*s && is_ws(*s)) s++;
                        const char *tok = s;
                        while (*s && !is_ws(*s)) s++;
                        if ((gsize)(s - tok) == wl &&
                            (ci ? g_ascii_strncasecmp(tok, a->value, wl)
                                : strncmp(tok, a->value, wl)) == 0) {
                            found = TRUE; break;
                        }
                    }
                    if (!found) return FALSE;
                    break;
                }
                case NS_CSS_ATTR_HYPHEN: {
                    if (vl < wl) return FALSE;
                    if (ci ? g_ascii_strncasecmp(v, a->value, wl)
                           : strncmp(v, a->value, wl)) return FALSE;
                    if (vl > wl && v[wl] != '-') return FALSE;
                    break;
                }
                case NS_CSS_ATTR_PRESENT: break;
                }
            }
        }
    }
    if (sel->pseudos && sel->pseudos->len > 0) {
        for (guint i = 0; i < sel->pseudos->len; i++) {
            const ns_css_pseudo_pred *pc =
                &g_array_index(sel->pseudos, ns_css_pseudo_pred, i);
            switch (pc->kind) {
            case NS_CSS_PC_FIRST_CHILD: {
                const ns_node *s = el->prev_sibling;
                while (s && s->kind != NS_NODE_ELEMENT) s = s->prev_sibling;
                if (s) return FALSE;
                break;
            }
            case NS_CSS_PC_LAST_CHILD: {
                const ns_node *s = el->next_sibling;
                while (s && s->kind != NS_NODE_ELEMENT) s = s->next_sibling;
                if (s) return FALSE;
                break;
            }
            case NS_CSS_PC_ONLY_CHILD: {
                const ns_node *s = el->prev_sibling;
                while (s && s->kind != NS_NODE_ELEMENT) s = s->prev_sibling;
                if (s) return FALSE;
                s = el->next_sibling;
                while (s && s->kind != NS_NODE_ELEMENT) s = s->next_sibling;
                if (s) return FALSE;
                break;
            }
            case NS_CSS_PC_ONLY_OF_TYPE: {
                if (!el->name) return FALSE;
                for (const ns_node *s = el->prev_sibling; s; s = s->prev_sibling)
                    if (ns_node_is_element_named(s, el->name)) return FALSE;
                for (const ns_node *s = el->next_sibling; s; s = s->next_sibling)
                    if (ns_node_is_element_named(s, el->name)) return FALSE;
                break;
            }
            case NS_CSS_PC_FIRST_OF_TYPE: {
                if (!el->name) return FALSE;
                for (const ns_node *s = el->prev_sibling; s; s = s->prev_sibling)
                    if (ns_node_is_element_named(s, el->name)) return FALSE;
                break;
            }
            case NS_CSS_PC_LAST_OF_TYPE: {
                if (!el->name) return FALSE;
                for (const ns_node *s = el->next_sibling; s; s = s->next_sibling)
                    if (ns_node_is_element_named(s, el->name)) return FALSE;
                break;
            }
            case NS_CSS_PC_EMPTY:
                if (!ns_el_is_empty(el)) return FALSE;
                break;
            case NS_CSS_PC_ROOT:
                if (!el->parent || el->parent->kind != NS_NODE_DOCUMENT ||
                    (el->parent->flags & NS_NODE_FRAGMENT))
                    return FALSE;
                break;
            case NS_CSS_PC_SCOPE:
                if (g_css_match_scope) {
                    if (el != g_css_match_scope) return FALSE;
                } else if (el->parent && el->parent->kind == NS_NODE_ELEMENT) {
                    return FALSE;
                }
                break;
            case NS_CSS_PC_CHECKED:
                if (!ns_el_is_checked(el))
                    return FALSE;
                break;
            case NS_CSS_PC_DISABLED:
                if (!ns_element_supports_disabled(el) ||
                    !ns_element_effectively_disabled(el))
                    return FALSE;
                break;
            case NS_CSS_PC_ENABLED:
                if (!ns_element_supports_disabled(el) ||
                    ns_element_effectively_disabled(el))
                    return FALSE;
                break;
            case NS_CSS_PC_REQUIRED:
                if (!ns_form_control_supports_required(el) ||
                    !ns_element_get_attr(el, "required"))
                    return FALSE;
                break;
            case NS_CSS_PC_OPTIONAL:
                if (!ns_form_control_supports_required(el) ||
                    ns_element_get_attr(el, "required"))
                    return FALSE;
                break;
            case NS_CSS_PC_VALID:
                if (!ns_css_node_will_validate(el) ||
                    !ns_css_control_is_valid(el))
                    return FALSE;
                break;
            case NS_CSS_PC_INVALID:
                if (!ns_css_node_will_validate(el) ||
                    ns_css_control_is_valid(el))
                    return FALSE;
                break;
            case NS_CSS_PC_IN_RANGE: {
                gboolean under = FALSE, over = FALSE;
                if (!ns_el_range_state(el, &under, &over) || under || over)
                    return FALSE;
                break;
            }
            case NS_CSS_PC_OUT_OF_RANGE: {
                gboolean under = FALSE, over = FALSE;
                if (!ns_el_range_state(el, &under, &over) || (!under && !over))
                    return FALSE;
                break;
            }
            case NS_CSS_PC_DEFAULT:
                if (!ns_el_is_default(el)) return FALSE;
                break;
            case NS_CSS_PC_INDETERMINATE:
                if (!ns_el_is_indeterminate(el)) return FALSE;
                break;
            case NS_CSS_PC_NTH_CHILD:
            case NS_CSS_PC_NTH_LAST_CHILD:
            case NS_CSS_PC_NTH_LAST_OF_TYPE:
            case NS_CSS_PC_NTH_OF_TYPE: {
                int idx = 1;
                if (!ns_css_sibling_counts_for_nth(el, pc, &idx)) return FALSE;
                int a = pc->a, b = pc->b;
                if (a == 0) {
                    if (idx != b) return FALSE;
                } else {
                    int diff = idx - b;
                    if ((diff % a) != 0) return FALSE;
                    if ((diff / a) < 0) return FALSE;
                }
                break;
            }
            case NS_CSS_PC_ANY_LINK:
                if (!ns_el_is_link(el)) return FALSE;
                break;
            case NS_CSS_PC_LINK:
                if (!ns_el_is_link(el) || ns_el_is_visited_link(el))
                    return FALSE;
                break;
            case NS_CSS_PC_VISITED:
                if (!ns_el_is_visited_link(el)) return FALSE;
                break;
            case NS_CSS_PC_HOVER: {
                if (!g_css_hover_node) return FALSE;
                gboolean on = FALSE;
                for (const ns_node *h = g_css_hover_node; h; h = h->parent)
                    if (h == el) { on = TRUE; break; }
                if (!on) return FALSE;
                break;
            }
            case NS_CSS_PC_ACTIVE: {
                if (!g_css_active_node) return FALSE;
                gboolean pressed = FALSE;
                for (const ns_node *a = g_css_active_node; a; a = a->parent)
                    if (a == el) { pressed = TRUE; break; }
                if (!pressed) return FALSE;
                break;
            }
            case NS_CSS_PC_FOCUS:
                if (!g_css_focus_node || el != g_css_focus_node) return FALSE;
                break;
            case NS_CSS_PC_FOCUS_VISIBLE:
                if (!g_css_focus_node || el != g_css_focus_node ||
                    el != g_css_focus_visible_node)
                    return FALSE;
                break;
            case NS_CSS_PC_FOCUS_WITHIN: {
                if (!g_css_focus_node) return FALSE;
                const ns_node *f = g_css_focus_node;
                gboolean within = FALSE;
                for (; f; f = f->parent)
                    if (f == el) { within = TRUE; break; }
                if (!within) return FALSE;
                break;
            }
            case NS_CSS_PC_TARGET: {
                if (!ns_css_node_is_target(el)) return FALSE;
                break;
            }
            case NS_CSS_PC_TARGET_WITHIN:
                if (!g_target_fragment ||
                    !ns_css_node_has_target_within(el, 0))
                    return FALSE;
                break;
            case NS_CSS_PC_DEFINED:
                if (!el->name) return FALSE;
                if (!strchr(el->name, '-')) break;
                if (ns_css_is_defined_element(el->name)) break;
                return FALSE;
            case NS_CSS_PC_PLACEHOLDER_SHOWN:
                if (!ns_el_placeholder_shown(el)) return FALSE;
                break;
            case NS_CSS_PC_READ_WRITE:
                if (!ns_el_is_read_write(el)) return FALSE;
                break;
            case NS_CSS_PC_READ_ONLY:
                if (ns_el_is_read_write(el)) return FALSE;
                break;
            case NS_CSS_PC_BLANK:
                if (!ns_el_is_blank(el)) return FALSE;
                break;
            case NS_CSS_PC_LANG:
                if (!ns_css_lang_matches(el, pc->arg)) return FALSE;
                break;
            case NS_CSS_PC_DIR:
                if (!pc->arg || strcmp(ns_css_node_dir(el), pc->arg) != 0)
                    return FALSE;
                break;
            case NS_CSS_PC_OPEN:
                if ((!ns_node_is_element_named(el, "details") &&
                     !ns_node_is_element_named(el, "dialog")) ||
                    !ns_element_get_attr(el, "open"))
                    return FALSE;
                break;
            case NS_CSS_PC_POPOVER_OPEN:
                if (!ns_element_get_attr(el, "popover") ||
                    !ns_element_get_attr(el, "data-nd-popover-open"))
                    return FALSE;
                break;
            case NS_CSS_PC_MODAL:
                if (!ns_element_get_attr(el, "data-nd-modal")) return FALSE;
                break;
            case NS_CSS_PC_FULLSCREEN:
                if (g_css_fullscreen_node != el) return FALSE;
                break;
            case NS_CSS_PC_HEADING: {
                int level = 0;
                if (el->kind == NS_NODE_ELEMENT && el->name &&
                    el->name[0] == 'h' && el->name[1] >= '1' &&
                    el->name[1] <= '6' && el->name[2] == '\0')
                    level = el->name[1] - '0';
                if (level == 0) return FALSE;
                if (pc->arg) {
                    char **items = g_strsplit(pc->arg, ",", -1);
                    gboolean any = FALSE;
                    for (int hi = 0; items[hi] && !any; hi++) {
                        int v = 0;
                        if (anb_int_strict(g_strstrip(items[hi]), &v) &&
                            level == v)
                            any = TRUE;
                    }
                    g_strfreev(items);
                    if (!any) return FALSE;
                }
                break;
            }
            case NS_CSS_PC_USER_VALID:
                if (!ns_css_node_will_validate(el) || !ns_css_control_is_valid(el) ||
                    !ns_element_get_attr(el, "data-nd-vdirty"))
                    return FALSE;
                break;
            case NS_CSS_PC_USER_INVALID:
                if (!ns_css_node_will_validate(el) || ns_css_control_is_valid(el) ||
                    !ns_element_get_attr(el, "data-nd-vdirty"))
                    return FALSE;
                break;
            case NS_CSS_PC_AUTOFILL:
                if (!ns_element_get_attr(el, "autofill") &&
                    !ns_element_get_attr(el, "data-nd-autofill"))
                    return FALSE;
                break;
            case NS_CSS_PC_PLAYING:
                if ((!ns_node_is_element_named(el, "video") &&
                     !ns_node_is_element_named(el, "audio")) ||
                    !ns_element_get_attr(el, "data-nd-playing"))
                    return FALSE;
                break;
            case NS_CSS_PC_PAUSED:
                if ((!ns_node_is_element_named(el, "video") &&
                     !ns_node_is_element_named(el, "audio")) ||
                    ns_element_get_attr(el, "data-nd-playing"))
                    return FALSE;
                break;
            case NS_CSS_PC_MUTED:
                if ((!ns_node_is_element_named(el, "video") &&
                     !ns_node_is_element_named(el, "audio")) ||
                    (!ns_element_get_attr(el, "muted") &&
                     !ns_element_get_attr(el, "data-nd-muted")))
                    return FALSE;
                break;
            case NS_CSS_PC_SEEKING:
                if ((!ns_node_is_element_named(el, "video") &&
                     !ns_node_is_element_named(el, "audio")) ||
                    !ns_element_get_attr(el, "data-nd-seeking"))
                    return FALSE;
                break;
            case NS_CSS_PC_BUFFERING:
                if ((!ns_node_is_element_named(el, "video") &&
                     !ns_node_is_element_named(el, "audio")) ||
                    !ns_element_get_attr(el, "data-nd-buffering"))
                    return FALSE;
                break;
            case NS_CSS_PC_STALLED:
                if ((!ns_node_is_element_named(el, "video") &&
                     !ns_node_is_element_named(el, "audio")) ||
                    !ns_element_get_attr(el, "data-nd-stalled"))
                    return FALSE;
                break;
            }
        }
    }
    if (sel->matches_any) {
        for (guint i = 0; i < sel->matches_any->len; i++) {
            const GPtrArray *group = g_ptr_array_index(sel->matches_any, i);
            gboolean any = FALSE;
            for (guint j = 0; j < group->len; j++) {
                const ns_css_selector *sub = g_ptr_array_index(group, j);
                if (match_selector(sub, el)) { any = TRUE; break; }
            }
            if (!any) return FALSE;
        }
    }
    if (sel->matches_none) {
        for (guint i = 0; i < sel->matches_none->len; i++) {
            const GPtrArray *group = g_ptr_array_index(sel->matches_none, i);
            for (guint j = 0; j < group->len; j++) {
                const ns_css_selector *sub = g_ptr_array_index(group, j);
                if (match_selector(sub, el)) return FALSE;
            }
        }
    }
    if (sel->has_groups) {
        for (guint i = 0; i < sel->has_groups->len; i++) {
            const GPtrArray *group = g_ptr_array_index(sel->has_groups, i);
            if (!has_group_matches(group, el)) return FALSE;
        }
    }
    return TRUE;
}

static char *
css_serialize_urls(char *value)
{
    GString *out = g_string_new(NULL);
    const char *p = value;
    const char *end = value + strlen(value);
    gboolean changed = FALSE;
    while (p < end) {
        if (end - p >= 4 && g_ascii_strncasecmp(p, "url(", 4) == 0 &&
            (p == value || !is_ident(p[-1]))) {
            const char *close = match_close_paren(p + 4, end);
            if (close) {
                const char *start = p + 4;
                while (start < close && is_ws(*start)) start++;
                const char *stop = close;
                while (stop > start && is_ws(stop[-1])) stop--;
                if (stop > start &&
                    ((*start == '"' && stop[-1] == '"') ||
                     (*start == '\'' && stop[-1] == '\''))) {
                    start++;
                    stop--;
                }
                g_string_append(out, "url(\"");
                for (const char *q = start; q < stop; q++) {
                    if (*q == '"' && (q == start || q[-1] != '\\'))
                        g_string_append_c(out, '\\');
                    g_string_append_c(out, *q);
                }
                g_string_append(out, "\")");
                p = close + 1;
                changed = TRUE;
                continue;
            }
        }
        g_string_append_c(out, *p++);
    }
    if (!changed) {
        g_string_free(out, TRUE);
        return value;
    }
    g_free(value);
    return g_string_free(out, FALSE);
}

static char *
place_shorthand_canonical(const char *prop, char *value)
{
    ns_css_prop ap = NS_CSS_ALIGN_CONTENT, jp = NS_CSS_JUSTIFY_CONTENT;
    if (strcmp(prop, "place-items") == 0) {
        ap = NS_CSS_ALIGN_ITEMS;
        jp = NS_CSS_JUSTIFY_ITEMS;
    } else if (strcmp(prop, "place-self") == 0) {
        ap = NS_CSS_ALIGN_SELF;
        jp = NS_CSS_JUSTIFY_SELF;
    }
    char *tokens[4] = {0};
    int n = split_ws(value, tokens);
    static const int splits[5][2] = { {0, 0}, {1, 0}, {2, 1}, {1, 2}, {2, 0} };
    char *result = NULL;
    for (int si = 0; si < 2 && n >= 1 && n <= 4 && !result; si++) {
        int k = splits[n][si];
        if (k == 0) break;
        char *first = k == 2 ? g_strdup_printf("%s %s", tokens[0], tokens[1])
                             : g_strdup(tokens[0]);
        char *second;
        if (k >= n) second = g_strdup(first);
        else if (n - k == 2) second = g_strdup_printf("%s %s", tokens[k], tokens[k + 1]);
        else second = g_strdup(tokens[k]);
        ns_css_value *av = ns_css_parse_value_for(ap, first);
        ns_css_value *jv = av ? ns_css_parse_value_for(jp, second) : NULL;
        if (av && jv && av->kind == NS_CSS_V_KEYWORD && jv->kind == NS_CSS_V_KEYWORD) {
            result = strcmp(av->u.keyword, jv->u.keyword) == 0
                ? g_strdup(av->u.keyword)
                : g_strdup_printf("%s %s", av->u.keyword, jv->u.keyword);
        }
        if (av) ns_css_value_free(av);
        if (jv) ns_css_value_free(jv);
        g_free(first);
        g_free(second);
    }
    for (int i = 0; i < n; i++) g_free(tokens[i]);
    if (!result) return value;
    g_free(value);
    return result;
}

static char *
css_inline_value_canonical(const char *prop, char *value)
{
    if (!value) return g_strdup("");
    switch (ns_css_prop_id(prop)) {
    case NS_CSS_BORDER_IMAGE_SLICE:
    case NS_CSS_BORDER_IMAGE_WIDTH:
    case NS_CSS_BORDER_IMAGE_OUTSET:
    case NS_CSS_BORDER_IMAGE_REPEAT: {
        ns_css_value *v = ns_css_parse_value_for((ns_css_prop)ns_css_prop_id(prop),
                                          value);
        if (v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword) {
            g_free(value);
            value = g_strdup(v->u.keyword);
        }
        ns_css_value_free(v);
        return value;
    }
    default:
        break;
    }
    if (strcmp(prop, "unicode-range") == 0) {
        char *canon = ns_css_unicode_range_canonical(value);
        if (canon) {
            g_free(value);
            return canon;
        }
        return value;
    }
    if (strcmp(prop, "place-self") == 0 || strcmp(prop, "place-items") == 0 ||
        strcmp(prop, "place-content") == 0)
        value = place_shorthand_canonical(prop, value);
    if (g_str_has_prefix(prop, "grid-row") ||
        g_str_has_prefix(prop, "grid-column") ||
        strcmp(prop, "grid-area") == 0) {
        gboolean shorthand = strcmp(prop, "grid-row") == 0 ||
                             strcmp(prop, "grid-column") == 0 ||
                             strcmp(prop, "grid-area") == 0;
        char *canon = shorthand
            ? ns_css_grid_placement_canonical(value, prop[5] == 'a')
            : ns_css_grid_line_canonical(value, NULL);
        if (canon) {
            g_free(value);
            return canon;
        }
    }
    if (strcmp(prop, "grid-template") == 0 || strcmp(prop, "grid") == 0) {
        char *parts[6] = {0};
        char *canon = NULL;
        gboolean ok = prop[4] == '\0' ? ns_css_grid_shorthand_parse(value, parts, &canon)
                                      : ns_css_grid_template_parse(value, parts, &canon);
        for (int i = 0; i < 6; i++) g_free(parts[i]);
        if (ok) {
            g_free(value);
            return canon;
        }
    }
    if (strcmp(prop, "grid-auto-flow") == 0) {
        char *canon = ns_css_grid_auto_flow_canonical(value);
        if (canon) {
            g_free(value);
            return canon;
        }
    }
    value = ns_css_add_leading_zeros(value);
    value = ns_css_normalize_negative_zero(value);
    value = css_serialize_urls(value);
    if (strcmp(prop, "content") == 0) {
        char *canon = ns_css_content_canonical(value);
        if (canon) {
            g_free(value);
            value = canon;
        }
    } else if (strcmp(prop, "font-family") == 0) {
        char *canon = ns_css_font_family_canonical(value);
        if (canon) {
            g_free(value);
            value = canon;
        }
    } else if (strcmp(prop, "font") == 0) {
        char *canon = ns_css_font_shorthand_canonical(value);
        if (canon) {
            g_free(value);
            value = canon;
        }
    } else if (strcmp(prop, "object-position") == 0 ||
               strcmp(prop, "background-position") == 0) {
        GString *out = g_string_new(NULL);
        const char *p = value;
        const char *end = value + strlen(value);
        gboolean ok = TRUE;
        while (p < end && ok) {
            char term = 0;
            const char *seg_end = css_scan_until(p, end, ",", &term);
            char *layer = css_trim_dup_range(p, seg_end);
            char *canon = ns_css_position_canonical_ex(
                layer, TRUE, strcmp(prop, "background-position") == 0);
            if (canon) {
                if (out->len) g_string_append(out, ", ");
                g_string_append(out, canon);
            } else {
                ok = FALSE;
            }
            g_free(canon);
            g_free(layer);
            p = term == ',' ? seg_end + 1 : seg_end;
        }
        if (ok) {
            g_free(value);
            value = g_string_free(out, FALSE);
        } else {
            g_string_free(out, TRUE);
        }
    } else if (strcmp(prop, "container") == 0) {
        char *canon = ns_css_container_shorthand_canonical(value);
        if (canon) {
            g_free(value);
            value = canon;
        }
    } else if (strcmp(prop, "background-image") == 0 ||
               strcmp(prop, "mask-image") == 0 ||
               strcmp(prop, "list-style-image") == 0 ||
               strcmp(prop, "border-image-source") == 0) {
        char *canon = ns_css_image_value_canonical(value);
        if (canon) {
            g_free(value);
            value = canon;
        }
    }
    return value;
}

#define INLINE_DECL_SHEETS_MAX 4096

static __thread GHashTable *g_inline_decl_sheets;
static __thread double g_inline_decl_sheets_vw, g_inline_decl_sheets_vh;

static const ns_css_stylesheet *
inline_declaration_sheet(const char *name, const char *value)
{
    if (!g_inline_decl_sheets)
        g_inline_decl_sheets = g_hash_table_new_full(
            g_str_hash, g_str_equal, g_free,
            (GDestroyNotify)ns_css_stylesheet_free);
    if (g_inline_decl_sheets_vw != g_viewport_w ||
        g_inline_decl_sheets_vh != g_viewport_h ||
        g_hash_table_size(g_inline_decl_sheets) >= INLINE_DECL_SHEETS_MAX) {
        g_hash_table_remove_all(g_inline_decl_sheets);
        g_inline_decl_sheets_vw = g_viewport_w;
        g_inline_decl_sheets_vh = g_viewport_h;
    }
    char *declaration = g_strdup_printf("*{%s:%s}", name, value);
    ns_css_stylesheet *sheet = g_hash_table_lookup(g_inline_decl_sheets,
                                                   declaration);
    if (sheet) {
        g_free(declaration);
        return sheet;
    }
    sheet = ns_css_stylesheet_parse(declaration, -1);
    if (sheet)
        g_hash_table_insert(g_inline_decl_sheets, declaration, sheet);
    else
        g_free(declaration);
    return sheet;
}

static char *
inline_expanded_value(const char *name, const char *value, int prop,
                      gboolean *important)
{
    if (g_ascii_strcasecmp(name, "list-style") == 0 &&
        (prop == NS_CSS_LIST_STYLE_TYPE || prop == NS_CSS_LIST_STYLE_POSITION ||
         prop == NS_CSS_LIST_STYLE_IMAGE)) {
        char *plain = g_strdup(value);
        gboolean imp = FALSE;
        css_strip_important(plain, &imp);
        char *type = NULL, *position = NULL, *image = NULL;
        gboolean ok = list_style_split(plain, &type, &position, &image);
        g_free(plain);
        if (!ok) return NULL;
        *important = imp;
        char *result = prop == NS_CSS_LIST_STYLE_TYPE ? type
                     : prop == NS_CSS_LIST_STYLE_POSITION ? position : image;
        if (result != type) g_free(type);
        if (result != position) g_free(position);
        if (result != image) g_free(image);
        return result;
    }
    const ns_css_stylesheet *sheet = inline_declaration_sheet(name, value);
    char *result = NULL;
    if (sheet) {
        for (guint ri = 0; ri < sheet->rules->len; ri++) {
            ns_css_rule *rule = g_ptr_array_index(sheet->rules, ri);
            for (guint di = 0; di < rule->decls->len; di++) {
                ns_css_decl *decl = &g_array_index(rule->decls,
                                                   ns_css_decl, di);
                if ((int)decl->prop != prop) continue;
                g_free(result);
                result = ns_css_value_serialize_specified(decl->value);
                *important = decl->important;
            }
        }
    }
    return result;
}

static gboolean
inline_property_is_all_covered(const char *name)
{
    return name && name[0] != '-' &&
           g_ascii_strcasecmp(name, "direction") != 0 &&
           g_ascii_strcasecmp(name, "unicode-bidi") != 0 &&
           ns_css_named_property_supported(name);
}

static const char *
inline_skip_at_rule(const char *p, const char *end)
{
    char term = 0;
    const char *stop = css_scan_segment(p, end, &term);
    if (term == '{') return css_skip_to_block_end(stop, end);
    if (term == ';') return stop + 1;
    return stop > p ? stop : p + 1;
}

static char *
inline_all_value_for(const char *style, const char *prefix)
{
    const char *p = style ? style : "";
    const char *end = p + strlen(p);
    char *all_value = NULL;
    gboolean all_important = FALSE;
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        while (p < end && *p == ';') p = css_skip_ws_comments(p + 1, end);
        if (p >= end) break;
        if (*p == '@') {
            p = inline_skip_at_rule(p, end);
            continue;
        }
        char term = 0;
        const char *kend = css_scan_until(p, end, ":;", &term);
        char *name = css_trim_dup_range(p, kend);
        if (term != ':') {
            g_free(name);
            p = term == ';' ? kend + 1 : kend;
            continue;
        }
        p = css_skip_ws_comments(kend + 1, end);
        const char *vend = css_scan_declaration_value(p, end, &term);
        char *value = css_trim_dup_range(p, vend);
        gboolean important = FALSE;
        css_strip_important(value, &important);
        g_strstrip(value);
        if (g_ascii_strcasecmp(name, "all") == 0 &&
            ns_css_named_declaration_valid("all", value) &&
            (!all_value || important || !all_important)) {
            g_free(all_value);
            all_value = g_strdup(value);
            all_important = important;
        } else if (all_value && inline_property_is_all_covered(name) &&
                   (!prefix || g_ascii_strcasecmp(name, prefix) == 0 ||
                    (g_ascii_strncasecmp(name, prefix, strlen(prefix)) == 0 &&
                     name[strlen(prefix)] == '-')) &&
                   ns_css_named_declaration_valid(name, value) &&
                   (important || !all_important)) {
            g_clear_pointer(&all_value, g_free);
            all_important = FALSE;
        }
        g_free(name);
        g_free(value);
        p = term == ';' ? vend + 1 : vend;
    }
    return all_value;
}

static char *
inline_all_value(const char *style)
{
    return inline_all_value_for(style, NULL);
}

static gboolean
inline_css_wide_value(const char *value)
{
    static const char *const wide[] = {
        "inherit", "initial", "revert", "revert-layer", "revert-rule",
        "unset",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(wide); i++)
        if (strcmp(value, wide[i]) == 0) return TRUE;
    return FALSE;
}

static const int *
inline_quad_ids(const char *prop)
{
    static const struct {
        const char *name;
        int ids[4];
    } quads[] = {
        { "margin", { NS_CSS_MARGIN_TOP, NS_CSS_MARGIN_RIGHT,
                      NS_CSS_MARGIN_BOTTOM, NS_CSS_MARGIN_LEFT } },
        { "padding", { NS_CSS_PADDING_TOP, NS_CSS_PADDING_RIGHT,
                       NS_CSS_PADDING_BOTTOM, NS_CSS_PADDING_LEFT } },
        { "border-width", { NS_CSS_BORDER_TOP_WIDTH,
                            NS_CSS_BORDER_RIGHT_WIDTH,
                            NS_CSS_BORDER_BOTTOM_WIDTH,
                            NS_CSS_BORDER_LEFT_WIDTH } },
        { "border-color", { NS_CSS_BORDER_TOP_COLOR,
                            NS_CSS_BORDER_RIGHT_COLOR,
                            NS_CSS_BORDER_BOTTOM_COLOR,
                            NS_CSS_BORDER_LEFT_COLOR } },
        { "border-style", { NS_CSS_BORDER_TOP_STYLE,
                            NS_CSS_BORDER_RIGHT_STYLE,
                            NS_CSS_BORDER_BOTTOM_STYLE,
                            NS_CSS_BORDER_LEFT_STYLE } },
    };
    for (gsize i = 0; i < G_N_ELEMENTS(quads); i++)
        if (strcmp(prop, quads[i].name) == 0) return quads[i].ids;
    return NULL;
}

static char *
inline_quad_value(const char *style, const char *prop,
                  gboolean *important_out)
{
    const int *ids = inline_quad_ids(prop);
    if (!ids) return NULL;
    char *wrapped = g_strconcat("*{", style ? style : "", "}", NULL);
    ns_css_stylesheet *sheet = ns_css_stylesheet_parse(wrapped, -1);
    g_free(wrapped);
    char *values[4] = { NULL, NULL, NULL, NULL };
    gboolean priorities[4] = { FALSE, FALSE, FALSE, FALSE };
    if (sheet) {
        for (guint ri = 0; ri < sheet->rules->len; ri++) {
            ns_css_rule *rule = g_ptr_array_index(sheet->rules, ri);
            for (guint di = 0; di < rule->decls->len; di++) {
                ns_css_decl *decl = &g_array_index(rule->decls,
                                                   ns_css_decl, di);
                for (int side = 0; side < 4; side++) {
                    if ((int)decl->prop != ids[side] ||
                        (priorities[side] && !decl->important))
                        continue;
                    g_free(values[side]);
                    values[side] = ns_css_value_serialize_specified(decl->value);
                    priorities[side] = decl->important;
                }
            }
        }
        ns_css_stylesheet_free(sheet);
    }
    char *result = NULL;
    gboolean complete = TRUE;
    for (int i = 0; i < 4; i++)
        if (!values[i] || priorities[i] != priorities[0]) complete = FALSE;
    gboolean any_wide = FALSE;
    for (int i = 0; i < 4; i++)
        if (values[i] && inline_css_wide_value(values[i])) any_wide = TRUE;
    if (complete && (!any_wide ||
        (strcmp(values[0], values[1]) == 0 &&
         strcmp(values[1], values[2]) == 0 &&
         strcmp(values[2], values[3]) == 0))) {
        if (strcmp(values[0], values[1]) == 0 &&
            strcmp(values[1], values[2]) == 0 &&
            strcmp(values[2], values[3]) == 0)
            result = g_strdup(values[0]);
        else if (strcmp(values[0], values[2]) == 0 &&
                 strcmp(values[1], values[3]) == 0)
            result = g_strdup_printf("%s %s", values[0], values[1]);
        else if (strcmp(values[1], values[3]) == 0)
            result = g_strdup_printf("%s %s %s", values[0], values[1],
                                     values[2]);
        else
            result = g_strdup_printf("%s %s %s %s", values[0], values[1],
                                     values[2], values[3]);
        if (important_out) *important_out = priorities[0];
    }
    for (int i = 0; i < 4; i++) g_free(values[i]);
    return result;
}

static char *
inline_pair_value(const char *style, int first_id, int second_id,
                  gboolean *important_out)
{
    char *wrapped = g_strconcat("*{", style ? style : "", "}", NULL);
    ns_css_stylesheet *sheet = ns_css_stylesheet_parse(wrapped, -1);
    g_free(wrapped);
    char *values[2] = { NULL, NULL };
    gboolean priorities[2] = { FALSE, FALSE };
    if (sheet) {
        for (guint ri = 0; ri < sheet->rules->len; ri++) {
            ns_css_rule *rule = g_ptr_array_index(sheet->rules, ri);
            for (guint di = 0; di < rule->decls->len; di++) {
                ns_css_decl *decl = &g_array_index(rule->decls,
                                                   ns_css_decl, di);
                if ((int)decl->prop == NS_CSS_OVERFLOW &&
                    first_id == NS_CSS_OVERFLOW_X &&
                    second_id == NS_CSS_OVERFLOW_Y) {
                    char *serialized = ns_css_value_serialize_specified(decl->value);
                    for (int index = 0; index < 2; index++) {
                        if (priorities[index] && !decl->important) continue;
                        g_free(values[index]);
                        values[index] = g_strdup(serialized);
                        priorities[index] = decl->important;
                    }
                    g_free(serialized);
                    continue;
                }
                int index = (int)decl->prop == first_id ? 0 :
                            (int)decl->prop == second_id ? 1 : -1;
                if (index < 0 || (priorities[index] && !decl->important))
                    continue;
                g_free(values[index]);
                values[index] = ns_css_value_serialize_specified(decl->value);
                priorities[index] = decl->important;
            }
        }
        ns_css_stylesheet_free(sheet);
    }
    char *result = NULL;
    if (values[0] && values[1] && priorities[0] == priorities[1] &&
        (!inline_css_wide_value(values[0]) ||
         strcmp(values[0], values[1]) == 0) &&
        (!inline_css_wide_value(values[1]) ||
         strcmp(values[0], values[1]) == 0)) {
        result = strcmp(values[0], values[1]) == 0
            ? g_strdup(values[0])
            : g_strdup_printf("%s %s", values[0], values[1]);
        if (important_out) *important_out = priorities[0];
    }
    g_free(values[0]);
    g_free(values[1]);
    return result;
}

static char *
inline_anim_shorthand_value(const char *style, gboolean is_animation)
{
    static const ns_css_prop anim_lh[] = {
        NS_CSS_ANIMATION_NAME, NS_CSS_ANIMATION_DURATION, NS_CSS_ANIMATION_DELAY,
        NS_CSS_ANIMATION_TIMING_FUNCTION, NS_CSS_ANIMATION_ITERATION_COUNT,
        NS_CSS_ANIMATION_DIRECTION, NS_CSS_ANIMATION_FILL_MODE,
        NS_CSS_ANIMATION_PLAY_STATE,
    };
    static const ns_css_prop trans_lh[] = {
        NS_CSS_TRANSITION_PROPERTY, NS_CSS_TRANSITION_DURATION,
        NS_CSS_TRANSITION_DELAY, NS_CSS_TRANSITION_TIMING_FUNCTION,
        NS_CSS_TRANSITION_BEHAVIOR,
    };
    const ns_css_prop *lh = is_animation ? anim_lh : trans_lh;
    gsize n = is_animation ? G_N_ELEMENTS(anim_lh) : G_N_ELEMENTS(trans_lh);
    ns_style *tmp = g_new0(ns_style, 1);
    gboolean any = FALSE;
    for (gsize i = 0; i < n; i++) {
        char *text = ns_inline_style_get(style, ns_css_prop_name(lh[i]));
        if (!text) continue;
        char *bang = strstr(text, " !important");
        if (bang) *bang = '\0';
        tmp->values[lh[i]] = ns_css_parse_value_for(lh[i], text);
        if (tmp->values[lh[i]]) any = TRUE;
        g_free(text);
    }
    char *r = NULL;
    if (any) {
        ns_css_anim_list list;
        gboolean mismatch = FALSE;
        ns_css_anim_lists(tmp, is_animation, &list, &mismatch);
        r = mismatch ? NULL : ns_css_anim_shorthand_serialize(&list, is_animation);
        ns_css_anim_list_clear(&list);
    }
    ns_style_free(tmp);
    return r;
}

static char *
inline_background_value(const char *style, gboolean *important_out)
{
    static const char *const names[] = {
        "background-image", "background-position", "background-size",
        "background-repeat", "background-attachment", "background-origin",
        "background-clip", "background-color",
    };
    char *values[G_N_ELEMENTS(names)] = { NULL };
    gboolean important[G_N_ELEMENTS(names)] = { FALSE };
    gboolean ok = TRUE;
    for (gsize i = 0; i < G_N_ELEMENTS(names) && ok; i++) {
        values[i] = ns_inline_style_get(style, names[i]);
        if (!values[i] || !*values[i]) ok = FALSE;
        else important[i] = ns_inline_value_strip_important(values[i]);
        if (ok && i > 0 && important[i] != important[0]) ok = FALSE;
    }
    char *r = NULL;
    gboolean all_wide = ok && inline_css_wide_value(values[0]);
    for (gsize i = 1; all_wide && i < G_N_ELEMENTS(names); i++)
        if (strcmp(values[i], values[0]) != 0) all_wide = FALSE;
    if (all_wide) {
        r = g_strdup(values[0]);
    } else if (ok) {
        r = ns_css_background_shorthand_serialize(values[0], values[1],
                values[2], values[3], values[4], values[5], values[6],
                values[7]);
    }
    if (r && important[0]) {
        char *with = g_strconcat(r, " !important", NULL);
        g_free(r);
        r = with;
    }
    if (r && important_out) *important_out = important[0];
    for (gsize i = 0; i < G_N_ELEMENTS(names); i++) g_free(values[i]);
    return r;
}

static char *
inline_grid_value(const char *style, gboolean full)
{
    static const char *const names[6] = {
        "grid-template-rows", "grid-template-columns", "grid-template-areas",
        "grid-auto-flow", "grid-auto-rows", "grid-auto-columns",
    };
    int n = full ? 6 : 3;
    char *v[6] = {0};
    int important = 0, present = 0;
    for (int i = 0; i < n; i++) {
        v[i] = ns_inline_style_get(style, names[i]);
        if (!v[i] || !*v[i]) continue;
        present++;
        gboolean imp = FALSE;
        css_strip_important(v[i], &imp);
        g_strstrip(v[i]);
        if (imp) important++;
    }
    char *r = NULL;
    if (present == n && (important == 0 || important == n)) {
        int wide = 0;
        for (int i = 0; i < n; i++)
            if (inline_css_wide_value(v[i])) wide++;
        if (wide == n) {
            gboolean same = TRUE;
            for (int i = 1; i < n; i++)
                if (strcmp(v[i], v[0]) != 0) same = FALSE;
            r = g_strdup(same ? v[0] : "");
        } else if (wide == 0) {
            r = full ? ns_css_grid_compose(v) : ns_css_grid_template_compose(v);
        } else {
            r = g_strdup("");
        }
        if (important && *r) {
            char *with = g_strconcat(r, " !important", NULL);
            g_free(r);
            r = with;
        }
    } else if (present > 0) {
        r = g_strdup("");
    }
    for (int i = 0; i < n; i++) g_free(v[i]);
    return r;
}

#define INLINE_GET_MEMO 32

static __thread struct {
    char *style;
    char *prop;
    char *value;
} g_inline_get_memo[INLINE_GET_MEMO];
static __thread guint g_inline_get_next;
static __thread double g_inline_get_vw, g_inline_get_vh;

static gboolean
inline_get_memo_hit(const char *style, const char *prop, char **out)
{
    if (g_inline_get_vw != g_viewport_w || g_inline_get_vh != g_viewport_h) {
        for (guint i = 0; i < INLINE_GET_MEMO; i++) {
            g_clear_pointer(&g_inline_get_memo[i].style, g_free);
            g_clear_pointer(&g_inline_get_memo[i].prop, g_free);
            g_clear_pointer(&g_inline_get_memo[i].value, g_free);
        }
        g_inline_get_vw = g_viewport_w;
        g_inline_get_vh = g_viewport_h;
        return FALSE;
    }
    for (guint i = 0; i < INLINE_GET_MEMO; i++)
        if (g_inline_get_memo[i].style &&
            strcmp(g_inline_get_memo[i].prop, prop) == 0 &&
            strcmp(g_inline_get_memo[i].style, style) == 0) {
            *out = g_strdup(g_inline_get_memo[i].value);
            return TRUE;
        }
    return FALSE;
}

static char *
inline_get_memo_keep(const char *style, const char *prop, char *value)
{
    guint slot = g_inline_get_next++ % INLINE_GET_MEMO;
    g_free(g_inline_get_memo[slot].style);
    g_free(g_inline_get_memo[slot].prop);
    g_free(g_inline_get_memo[slot].value);
    g_inline_get_memo[slot].style = g_strdup(style);
    g_inline_get_memo[slot].prop = g_strdup(prop);
    g_inline_get_memo[slot].value = g_strdup(value);
    return value;
}

char *
ns_inline_style_get(const char *style, const char *prop)
{
    if (!style || !prop) return NULL;
    char *hit = NULL;
    if (inline_get_memo_hit(style, prop, &hit)) return hit;
    if (g_ascii_strcasecmp(prop, "all") == 0)
        return inline_get_memo_keep(style, prop, inline_all_value(style));
    if (inline_quad_ids(prop))
        return inline_get_memo_keep(style, prop,
                                    inline_quad_value(style, prop, NULL));
    if (g_ascii_strcasecmp(prop, "overflow") == 0)
        return inline_get_memo_keep(style, prop,
            inline_pair_value(style, NS_CSS_OVERFLOW_X, NS_CSS_OVERFLOW_Y,
                              NULL));
    if (g_ascii_strcasecmp(prop, "font") == 0) {
        char *font_all = inline_all_value_for(style, "font");
        if (font_all) return inline_get_memo_keep(style, prop, font_all);
    }
    if (g_ascii_strcasecmp(prop, "animation") == 0 ||
        g_ascii_strcasecmp(prop, "transition") == 0)
        return inline_get_memo_keep(style, prop,
            inline_anim_shorthand_value(style, prop[0] == 'a'));
    if (g_ascii_strcasecmp(prop, "list-style") == 0) {
        char *type = ns_inline_style_get(style, "list-style-type");
        char *pos = ns_inline_style_get(style, "list-style-position");
        char *img = ns_inline_style_get(style, "list-style-image");
        char *r = NULL;
        if (type && pos && img && inline_css_wide_value(type) &&
            strcmp(type, pos) == 0 && strcmp(pos, img) == 0)
            r = g_strdup(type);
        else if (type && pos && img)
            r = ns_css_list_style_serialize(type, pos, img);
        g_free(type);
        g_free(pos);
        g_free(img);
        return inline_get_memo_keep(style, prop, r);
    }
    if (g_ascii_strcasecmp(prop, "animation-range") == 0) {
        char *st = ns_inline_style_get(style, "animation-range-start");
        char *en = ns_inline_style_get(style, "animation-range-end");
        char *r = st && en ? ns_css_animation_range_serialize(st, en) : NULL;
        g_free(st);
        g_free(en);
        return inline_get_memo_keep(style, prop, r);
    }
    if (g_ascii_strcasecmp(prop, "background") == 0) {
        char *r = inline_background_value(style, NULL);
        if (r) return inline_get_memo_keep(style, prop, r);
    }
    if (g_ascii_strcasecmp(prop, "background-position") == 0) {
        char *xs = ns_inline_style_get(style, "background-position-x");
        char *ys = ns_inline_style_get(style, "background-position-y");
        char *r = xs && ys ? bg_position_zip(xs, ys) : NULL;
        g_free(xs);
        g_free(ys);
        if (r) return inline_get_memo_keep(style, prop, r);
    }
    if ((g_ascii_strcasecmp(prop, "grid") == 0 ||
         g_ascii_strcasecmp(prop, "grid-template") == 0) &&
        !strstr(style, "var(")) {
        char *r = inline_grid_value(style, prop[4] == '\0');
        if (r) return inline_get_memo_keep(style, prop, r);
    }
    if (ns_css_prop_id(prop) < 0 && ns_css_named_property_supported(prop)) {
        char *all = inline_all_value(style);
        if (all) return inline_get_memo_keep(style, prop, all);
    }
    int pid = ns_css_prop_id(prop);
    gsize plen = strlen(prop);
    const char *p = style;
    const char *end = style + strlen(style);
    char *winner = NULL;
    gboolean winner_important = FALSE;
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        while (p < end && *p == ';') {
            p++;
            p = css_skip_ws_comments(p, end);
        }
        if (p >= end) break;
        if (*p == '@') {
            p = inline_skip_at_rule(p, end);
            continue;
        }
        const char *kstart = p;
        char term = 0;
        const char *kend = css_scan_until(p, end, ":;", &term);
        char *key = css_trim_dup_range(kstart, kend);
        if (term != ':') {
            g_free(key);
            p = term == ';' ? kend + 1 : kend;
            continue;
        }
        p = css_skip_ws_comments(kend + 1, end);
        const char *vstart = p;
        const char *vend = css_scan_declaration_value(p, end, &term);
        char *value = css_trim_dup_range(vstart, vend);
        gboolean custom = prop[0] == '-' && prop[1] == '-';
        gboolean match = strlen(key) == plen &&
                         (custom ? strcmp(key, prop) == 0
                                 : g_ascii_strcasecmp(key, prop) == 0);
        gboolean important = FALSE;
        char *candidate = NULL;
        if (match) {
            char *priority_value = g_strdup(value);
            css_strip_important(priority_value, &important);
            g_free(priority_value);
            candidate = value;
            value = NULL;
        } else if (pid >= 0) {
            candidate = inline_expanded_value(key, value, pid, &important);
        }
        g_free(key);
        if (candidate) {
            if (!winner || important || !winner_important) {
                g_free(winner);
                winner = important && !match
                    ? g_strconcat(candidate, " !important", NULL) : candidate;
                if (winner != candidate) g_free(candidate);
                winner_important = important;
            } else {
                g_free(candidate);
            }
        }
        g_free(value);
        p = term == ';' ? vend + 1 : vend;
    }

    if (winner)
        return inline_get_memo_keep(style, prop,
                                    css_inline_value_canonical(prop, winner));

    return inline_get_memo_keep(style, prop, NULL);
}

typedef struct {
    char *name;
    char *value;
    gboolean important;
} ns_inline_decl;

static void
inline_decl_free(gpointer data)
{
    ns_inline_decl *decl = data;
    if (!decl) return;
    g_free(decl->name);
    g_free(decl->value);
    g_free(decl);
}

static ns_inline_decl *
inline_decl_find(GPtrArray *decls, const char *name)
{
    gboolean custom = name[0] == '-' && name[1] == '-';
    for (guint i = 0; i < decls->len; i++) {
        ns_inline_decl *decl = g_ptr_array_index(decls, i);
        if (custom ? strcmp(decl->name, name) == 0
                   : g_ascii_strcasecmp(decl->name, name) == 0)
            return decl;
    }
    return NULL;
}

#define INLINE_SERIALIZE_MEMO 16

static __thread struct {
    char *in;
    char *out;
} g_inline_serialize_memo[INLINE_SERIALIZE_MEMO];
static __thread guint g_inline_serialize_next;
static __thread double g_inline_serialize_vw, g_inline_serialize_vh;

static char *
inline_serialize_memo_hit(const char *key)
{
    if (g_inline_serialize_vw != g_viewport_w ||
        g_inline_serialize_vh != g_viewport_h) {
        for (guint i = 0; i < INLINE_SERIALIZE_MEMO; i++) {
            g_clear_pointer(&g_inline_serialize_memo[i].in, g_free);
            g_clear_pointer(&g_inline_serialize_memo[i].out, g_free);
        }
        g_inline_serialize_vw = g_viewport_w;
        g_inline_serialize_vh = g_viewport_h;
        return NULL;
    }
    for (guint i = 0; i < INLINE_SERIALIZE_MEMO; i++)
        if (g_inline_serialize_memo[i].in &&
            strcmp(g_inline_serialize_memo[i].in, key) == 0)
            return g_strdup(g_inline_serialize_memo[i].out);
    return NULL;
}

static char *
inline_serialize_memo_keep(const char *key, char *out)
{
    guint slot = g_inline_serialize_next++ % INLINE_SERIALIZE_MEMO;
    g_free(g_inline_serialize_memo[slot].in);
    g_free(g_inline_serialize_memo[slot].out);
    g_inline_serialize_memo[slot].in = g_strdup(key);
    g_inline_serialize_memo[slot].out = g_strdup(out);
    return out;
}

char *
ns_inline_style_serialize(const char *style)
{
    char *hit = inline_serialize_memo_hit(style ? style : "");
    if (hit) return hit;
    GPtrArray *decls = g_ptr_array_new_with_free_func(inline_decl_free);
    const char *p = style ? style : "";
    const char *end = p + strlen(p);
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        while (p < end && *p == ';') {
            p++;
            p = css_skip_ws_comments(p, end);
        }
        if (p >= end) break;
        if (*p == '@') {
            p = inline_skip_at_rule(p, end);
            continue;
        }
        const char *kstart = p;
        char term = 0;
        const char *kend = css_scan_until(p, end, ":;", &term);
        char *name = css_trim_dup_range(kstart, kend);
        if (term != ':') {
            g_free(name);
            p = term == ';' ? kend + 1 : kend;
            continue;
        }
        p = css_skip_ws_comments(kend + 1, end);
        const char *vstart = p;
        const char *vend = css_scan_declaration_value(p, end, &term);
        char *value = css_trim_dup_range(vstart, vend);
        gboolean custom = name[0] == '-' && name[1] == '-';
        if (!custom) {
            char *lower = g_ascii_strdown(name, -1);
            g_free(name);
            name = lower;
            if (strcmp(name, "-webkit-line-clamp") == 0) {
                g_free(name);
                name = g_strdup("line-clamp");
            }
        }
        gboolean important = FALSE;
        css_strip_important(value, &important);
        g_strstrip(value);
        if (!*name || !*value || !ns_css_named_property_supported(name) ||
            !ns_css_named_declaration_valid(name, value)) {
            g_free(name);
            g_free(value);
            p = term == ';' ? vend + 1 : vend;
            continue;
        }
        value = css_inline_value_canonical(name, value);
        char *canonical = custom ? NULL
            : ns_css_specified_canonical(name, value);
        if (canonical) {
            g_free(value);
            value = canonical;
        }
        ns_inline_decl *decl = inline_decl_find(decls, name);
        if (!decl) {
            decl = g_new0(ns_inline_decl, 1);
            decl->name = name;
            decl->value = value;
            decl->important = important;
            g_ptr_array_add(decls, decl);
        } else {
            g_free(name);
            if (important || !decl->important) {
                g_free(decl->value);
                decl->value = value;
                decl->important = important;
            } else {
                g_free(value);
            }
        }
        p = term == ';' ? vend + 1 : vend;
    }
    gint all_index = -1;
    for (guint i = 0; i < decls->len; i++) {
        ns_inline_decl *decl = g_ptr_array_index(decls, i);
        if (strcmp(decl->name, "all") == 0) all_index = (gint)i;
    }
    if (all_index >= 0) {
        ns_inline_decl *all_decl = g_ptr_array_index(decls, (guint)all_index);
        for (gint i = (gint)decls->len - 1; i >= 0; i--) {
            if (i == all_index) continue;
            ns_inline_decl *decl = g_ptr_array_index(decls, (guint)i);
            if (!inline_property_is_all_covered(decl->name)) continue;
            gboolean overridden = i < all_index &&
                (all_decl->important || !decl->important);
            gboolean redundant = i > all_index &&
                decl->important == all_decl->important &&
                strcmp(decl->value, all_decl->value) == 0;
            if (overridden || redundant)
                g_ptr_array_remove_index(decls, (guint)i);
        }
    }
    static const char *const quad_names[] = {
        "margin", "padding", "border-width", "border-color",
        "border-style",
    };
    char *quad_values[G_N_ELEMENTS(quad_names)] = { NULL };
    gboolean quad_priorities[G_N_ELEMENTS(quad_names)] = { FALSE };
    gboolean quad_complete[G_N_ELEMENTS(quad_names)] = { FALSE };
    gboolean quad_emitted[G_N_ELEMENTS(quad_names)] = { FALSE };
    for (gsize q = 0; q < G_N_ELEMENTS(quad_names); q++) {
        const int *ids = inline_quad_ids(quad_names[q]);
        if (!ids) continue;
        gboolean sides[4] = { FALSE, FALSE, FALSE, FALSE };
        for (guint i = 0; i < decls->len; i++) {
            ns_inline_decl *decl = g_ptr_array_index(decls, i);
            if (strcmp(decl->name, quad_names[q]) == 0) {
                quad_complete[q] = TRUE;
                break;
            }
            int id = ns_css_prop_id(decl->name);
            for (int side = 0; side < 4; side++)
                if (id == ids[side]) sides[side] = TRUE;
        }
        if (!quad_complete[q])
            quad_complete[q] = sides[0] && sides[1] && sides[2] && sides[3];
        if (quad_complete[q])
            quad_values[q] = inline_quad_value(style, quad_names[q],
                                                &quad_priorities[q]);
    }
    gboolean overflow_sides[2] = { FALSE, FALSE };
    gboolean overflow_complete = FALSE;
    gboolean overflow_important = FALSE;
    gboolean overflow_emitted = FALSE;
    for (guint i = 0; i < decls->len; i++) {
        ns_inline_decl *decl = g_ptr_array_index(decls, i);
        if (strcmp(decl->name, "overflow") == 0) overflow_complete = TRUE;
        int id = ns_css_prop_id(decl->name);
        if (id == NS_CSS_OVERFLOW_X) overflow_sides[0] = TRUE;
        if (id == NS_CSS_OVERFLOW_Y) overflow_sides[1] = TRUE;
    }
    overflow_complete = overflow_complete ||
                        (overflow_sides[0] && overflow_sides[1]);
    char *overflow_value = overflow_complete
        ? inline_pair_value(style, NS_CSS_OVERFLOW_X, NS_CSS_OVERFLOW_Y,
                            &overflow_important)
        : NULL;
    static const char *const outline_names[] = {
        "outline-color", "outline-style", "outline-width",
    };
    ns_inline_decl *outline_parts[G_N_ELEMENTS(outline_names)] = { NULL };
    for (gsize i = 0; i < G_N_ELEMENTS(outline_names); i++)
        outline_parts[i] = inline_decl_find(decls, outline_names[i]);
    gboolean outline_complete = outline_parts[0] && outline_parts[1] &&
                                outline_parts[2] &&
                                outline_parts[0]->important ==
                                    outline_parts[1]->important &&
                                outline_parts[1]->important ==
                                    outline_parts[2]->important;
    char *outline_value = outline_complete
        ? g_strdup_printf("%s %s %s", outline_parts[0]->value,
                          outline_parts[1]->value, outline_parts[2]->value)
        : NULL;
    gboolean outline_emitted = FALSE;
    static const char *const list_names[] = {
        "list-style-position", "list-style-type", "list-style-image",
    };
    ns_inline_decl *list_parts[G_N_ELEMENTS(list_names)] = { NULL };
    for (gsize i = 0; i < G_N_ELEMENTS(list_names); i++)
        list_parts[i] = inline_decl_find(decls, list_names[i]);
    gboolean list_complete = list_parts[0] && list_parts[1] && list_parts[2] &&
        list_parts[0]->important == list_parts[1]->important &&
        list_parts[1]->important == list_parts[2]->important;
    char *list_value = NULL;
    if (list_complete) {
        gboolean omit_image = strcmp(list_parts[2]->value, "none") == 0;
        list_value = omit_image
            ? g_strdup_printf("%s %s", list_parts[0]->value,
                              list_parts[1]->value)
            : g_strdup_printf("%s %s %s", list_parts[0]->value,
                              list_parts[1]->value, list_parts[2]->value);
    }
    gboolean list_emitted = FALSE;
    static const char *const background_names[] = {
        "background-image", "background-position-x", "background-position-y",
        "background-size", "background-repeat", "background-attachment",
        "background-origin", "background-clip", "background-color",
    };
    gboolean background_complete = TRUE;
    gboolean background_important = FALSE;
    for (gsize i = 0; i < G_N_ELEMENTS(background_names); i++) {
        ns_inline_decl *part = inline_decl_find(decls, background_names[i]);
        if (!part || (i > 0 && part->important != background_important))
            background_complete = FALSE;
        else if (i == 0)
            background_important = part->important;
    }
    char *background_value = background_complete
        ? inline_background_value(style, NULL) : NULL;
    if (background_value) ns_inline_value_strip_important(background_value);
    gboolean background_emitted = FALSE;
    GString *out = g_string_new(NULL);
    for (guint i = 0; i < decls->len; i++) {
        ns_inline_decl *decl = g_ptr_array_index(decls, i);
        gboolean collapsed = FALSE;
        gboolean background_member = FALSE;
        for (gsize part = 0; part < G_N_ELEMENTS(background_names); part++)
            if (strcmp(decl->name, background_names[part]) == 0)
                background_member = TRUE;
        if (background_value && background_member) {
            if (!background_emitted) {
                if (out->len) g_string_append_c(out, ' ');
                g_string_append_printf(out, "background: %s", background_value);
                if (background_important) g_string_append(out, " !important");
                g_string_append_c(out, ';');
                background_emitted = TRUE;
            }
            continue;
        }
        for (gsize q = 0; q < G_N_ELEMENTS(quad_names); q++) {
            if (!quad_values[q]) continue;
            const int *ids = inline_quad_ids(quad_names[q]);
            if (!ids) continue;
            int id = ns_css_prop_id(decl->name);
            gboolean member = strcmp(decl->name, quad_names[q]) == 0;
            for (int side = 0; side < 4; side++)
                if (id == ids[side]) member = TRUE;
            if (!member) continue;
            if (!quad_emitted[q]) {
                if (out->len) g_string_append_c(out, ' ');
                g_string_append_printf(out, "%s: %s", quad_names[q],
                                       quad_values[q]);
                if (quad_priorities[q]) g_string_append(out, " !important");
                g_string_append_c(out, ';');
                quad_emitted[q] = TRUE;
            }
            collapsed = TRUE;
            break;
        }
        int decl_id = ns_css_prop_id(decl->name);
        gboolean overflow_member = strcmp(decl->name, "overflow") == 0 ||
            decl_id == NS_CSS_OVERFLOW_X || decl_id == NS_CSS_OVERFLOW_Y;
        if (!collapsed && overflow_value && overflow_member) {
            if (!overflow_emitted) {
                if (out->len) g_string_append_c(out, ' ');
                g_string_append_printf(out, "overflow: %s", overflow_value);
                if (overflow_important) g_string_append(out, " !important");
                g_string_append_c(out, ';');
                overflow_emitted = TRUE;
            }
            collapsed = TRUE;
        }
        gboolean outline_member = FALSE;
        for (gsize part = 0; part < G_N_ELEMENTS(outline_names); part++)
            if (strcmp(decl->name, outline_names[part]) == 0)
                outline_member = TRUE;
        if (!collapsed && outline_value && outline_member) {
            if (!outline_emitted) {
                if (out->len) g_string_append_c(out, ' ');
                g_string_append_printf(out, "outline: %s", outline_value);
                if (outline_parts[0]->important)
                    g_string_append(out, " !important");
                g_string_append_c(out, ';');
                outline_emitted = TRUE;
            }
            collapsed = TRUE;
        }
        gboolean list_member = FALSE;
        for (gsize part = 0; part < G_N_ELEMENTS(list_names); part++)
            if (strcmp(decl->name, list_names[part]) == 0)
                list_member = TRUE;
        if (!collapsed && list_value && list_member) {
            if (!list_emitted) {
                if (out->len) g_string_append_c(out, ' ');
                g_string_append_printf(out, "list-style: %s", list_value);
                if (list_parts[0]->important)
                    g_string_append(out, " !important");
                g_string_append_c(out, ';');
                list_emitted = TRUE;
            }
            collapsed = TRUE;
        }
        if (collapsed) continue;
        if (out->len) g_string_append_c(out, ' ');
        g_string_append(out, decl->name);
        g_string_append(out, ": ");
        g_string_append(out, decl->value);
        if (decl->important) g_string_append(out, " !important");
        g_string_append_c(out, ';');
    }
    for (gsize q = 0; q < G_N_ELEMENTS(quad_names); q++)
        g_free(quad_values[q]);
    g_free(overflow_value);
    g_free(outline_value);
    g_free(list_value);
    g_free(background_value);
    g_ptr_array_free(decls, TRUE);
    return inline_serialize_memo_keep(style ? style : "",
                                      g_string_free(out, FALSE));
}

gboolean
ns_inline_value_strip_important(char *value)
{
    gboolean important = FALSE;
    css_strip_important(value, &important);
    return important;
}

static gboolean
inline_shorthand_follows(const char *style, const char *prop, int prop_id)
{
    if (!style || !*style) return FALSE;
    const char *p = style;
    const char *end = p + strlen(p);
    gboolean seen_prop = FALSE;
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        while (p < end && *p == ';') {
            p++;
            p = css_skip_ws_comments(p, end);
        }
        if (p >= end) break;
        if (*p == '@') {
            p = inline_skip_at_rule(p, end);
            continue;
        }
        char term = 0;
        const char *kend = css_scan_until(p, end, ":;", &term);
        char *key = css_trim_dup_range(p, kend);
        if (term != ':') {
            g_free(key);
            p = term == ';' ? kend + 1 : kend;
            continue;
        }
        p = css_skip_ws_comments(kend + 1, end);
        const char *vend = css_scan_declaration_value(p, end, &term);
        if (g_ascii_strcasecmp(key, prop) == 0) {
            seen_prop = TRUE;
        } else if (seen_prop && (ns_css_prop_id(key) < 0 ||
                                 ns_css_prop_id(key) == NS_CSS_BORDER_RADIUS)) {
            char *value = css_trim_dup_range(p, vend);
            gboolean important = FALSE;
            char *expanded = inline_expanded_value(key, value, prop_id,
                                                   &important);
            g_free(value);
            if (expanded) {
                g_free(expanded);
                g_free(key);
                return TRUE;
            }
        }
        g_free(key);
        p = term == ';' ? vend + 1 : vend;
    }
    return FALSE;
}

static int
inline_anim_member_ids(const char *prop, int out[16])
{
    if (!prop) return 0;
    if (strcmp(prop, "animation") == 0) {
        for (gsize i = 0; i < G_N_ELEMENTS(kAnimationLonghands); i++)
            out[i] = kAnimationLonghands[i];
        return (int)G_N_ELEMENTS(kAnimationLonghands);
    }
    if (strcmp(prop, "transition") == 0) {
        for (gsize i = 0; i < G_N_ELEMENTS(kTransitionLonghands); i++)
            out[i] = kTransitionLonghands[i];
        return (int)G_N_ELEMENTS(kTransitionLonghands);
    }
    if (strcmp(prop, "animation-range") == 0) {
        out[0] = NS_CSS_ANIMATION_RANGE_START;
        out[1] = NS_CSS_ANIMATION_RANGE_END;
        return 2;
    }
    if (strcmp(prop, "list-style") == 0) {
        out[0] = NS_CSS_LIST_STYLE_TYPE;
        out[1] = NS_CSS_LIST_STYLE_POSITION;
        out[2] = NS_CSS_LIST_STYLE_IMAGE;
        return 3;
    }
    return 0;
}

static char *
inline_anim_expanded(const char *prop, const char *value)
{
    if (strcmp(prop, "list-style") == 0) {
        char *type = NULL, *position = NULL, *image = NULL;
        if (!list_style_split(value, &type, &position, &image)) return NULL;
        char *r = g_strdup_printf("list-style-type: %s; list-style-position: %s; "
                                  "list-style-image: %s", type, position, image);
        g_free(type);
        g_free(position);
        g_free(image);
        return r;
    }
    char *text = g_strdup_printf("%s: %s;", prop, value);
    const char *p = text;
    GArray *expanded = g_array_new(FALSE, FALSE, sizeof(ns_css_decl));
    parse_declaration_block(&p, text + strlen(text), expanded, NULL);
    GString *out = g_string_new(NULL);
    for (guint i = 0; i < expanded->len; i++) {
        ns_css_decl *item = &g_array_index(expanded, ns_css_decl, i);
        if (item->prop != NS_CSS_ANIMATION && item->prop != NS_CSS_TRANSITION) {
            char *serialized = ns_css_value_serialize(item->value);
            if (out->len > 0) g_string_append(out, "; ");
            g_string_append(out, ns_css_prop_name(item->prop));
            g_string_append(out, ": ");
            g_string_append(out, serialized);
            g_free(serialized);
        }
        ns_css_value_free(item->value);
    }
    g_array_free(expanded, TRUE);
    g_free(text);
    if (out->len == 0) {
        g_string_free(out, TRUE);
        return NULL;
    }
    return g_string_free(out, FALSE);
}

char *
ns_inline_style_set(const char *style, const char *prop, const char *value)
{
    if (!prop) return g_strdup(style ? style : "");
    if (!style || !*style) {
        if (!value || !*value) return g_strdup("");
        int members[16];
        if (g_ascii_strcasecmp(prop, "all") != 0 && !inline_quad_ids(prop) &&
            inline_anim_member_ids(prop, members) == 0) {
            return g_strdup_printf("%s: %s", prop, value);
        }
    }
    GString *out = g_string_new(NULL);
    gboolean found = FALSE;
    gboolean set_all = g_ascii_strcasecmp(prop, "all") == 0;
    char *active_all = !set_all && inline_property_is_all_covered(prop)
        ? inline_all_value(style) : NULL;
    gboolean append_after_all = active_all != NULL;
    g_free(active_all);
    int anim_member_ids[16] = {0};
    int n_anim_members = inline_anim_member_ids(prop, anim_member_ids);
    char *anim_expanded = NULL;
    if (n_anim_members > 0 && value && *value) {
        anim_expanded = inline_anim_expanded(prop, value);
        if (!anim_expanded) {
            g_string_free(out, TRUE);
            return g_strdup(style ? style : "");
        }
    }
    int set_prop_id = ns_css_prop_id(prop);
    gboolean append_after_shorthand = set_prop_id >= 0 &&
        inline_shorthand_follows(style, prop, set_prop_id);
    gsize plen = prop ? strlen(prop) : 0;
    const char *p = style ? style : "";
    const char *end = p + strlen(p);
    while (p < end) {
        p = css_skip_ws_comments(p, end);
        while (p < end && *p == ';') {
            p++;
            p = css_skip_ws_comments(p, end);
        }
        if (p >= end) break;
        if (*p == '@') {
            p = inline_skip_at_rule(p, end);
            continue;
        }
        const char *kstart = p;
        char term = 0;
        const char *kend = css_scan_until(p, end, ":;", &term);
        char *key = css_trim_dup_range(kstart, kend);
        if (term != ':') {
            g_free(key);
            p = term == ';' ? kend + 1 : kend;
            continue;
        }
        p = css_skip_ws_comments(kend + 1, end);
        const char *vstart = p;
        const char *vend = css_scan_declaration_value(p, end, &term);
        char *old_value = css_trim_dup_range(vstart, vend);
        gboolean custom = prop && prop[0] == '-' && prop[1] == '-';
        gboolean match = strlen(key) == plen && prop &&
                         (custom ? strcmp(key, prop) == 0
                                 : g_ascii_strcasecmp(key, prop) == 0);
        int key_id = ns_css_prop_id(key);
        gboolean anim_member = FALSE;
        for (int m = 0; m < n_anim_members; m++)
            if (key_id == anim_member_ids[m]) anim_member = TRUE;
        gboolean remove_for_all = prop &&
            g_ascii_strcasecmp(prop, "all") == 0 &&
            inline_property_is_all_covered(key);
        if (set_all && (match || remove_for_all)) {
            found = TRUE;
            g_free(key);
            g_free(old_value);
            p = term == ';' ? vend + 1 : vend;
            continue;
        }
        if ((append_after_all || append_after_shorthand ||
             n_anim_members > 0) && (match || anim_member)) {
            found = TRUE;
            g_free(key);
            g_free(old_value);
            p = term == ';' ? vend + 1 : vend;
            continue;
        }
        if (match || remove_for_all) {
            if (!value || !*value || found) {
                found = TRUE;
                g_free(key);
                g_free(old_value);
                p = term == ';' ? vend + 1 : vend;
                continue;
            }
            if (out->len > 0) g_string_append(out, "; ");
            g_string_append(out, key);
            g_string_append(out, ": ");
            g_string_append(out, value);
            found = TRUE;
        } else {
            if (out->len > 0) g_string_append(out, "; ");
            g_string_append(out, key);
            g_string_append(out, ": ");
            g_string_append(out, old_value);
        }
        g_free(key);
        g_free(old_value);
        p = term == ';' ? vend + 1 : vend;
    }
    if ((set_all || append_after_all || append_after_shorthand ||
         n_anim_members > 0 || !found) && value && *value) {
        if (out->len > 0) g_string_append(out, "; ");
        if (anim_expanded) {
            g_string_append(out, anim_expanded);
        } else {
            g_string_append(out, prop);
            g_string_append(out, ": ");
            g_string_append(out, value);
        }
    }
    g_free(anim_expanded);
    return g_string_free(out, FALSE);
}

GPtrArray *
ns_css_parse_selector_list(const char *text)
{
    GPtrArray *out = g_ptr_array_new_with_free_func((GDestroyNotify)ns_css_selector_free);
    if (!text) return out;
    const char *p = text;
    const char *end = text + strlen(text);
    gboolean expect_selector = TRUE;
    while (p < end) {
        while (p < end && is_ws(*p)) p++;
        if (p >= end) break;
        if (*p == ',') {
            g_sel_parse_error = TRUE;
            p++;
            expect_selector = TRUE;
            continue;
        }
        const char *iter_start = p;
        ns_css_selector *sel = parse_one_selector(&p, end, 0);
        if (sel) {
            g_ptr_array_add(out, sel);
            expect_selector = FALSE;
        }
        while (p < end && is_ws(*p)) p++;
        if (p < end && *p == ',') { p++; expect_selector = TRUE; }
        else if (p == iter_start) break;
    }
    if (expect_selector)
        g_sel_parse_error = TRUE;
    return out;
}

GPtrArray *
ns_css_parse_selector_list_checked(const char *text, gboolean *out_valid)
{
    g_sel_parse_error = FALSE;
    g_sel_ns_prefix = FALSE;
    GPtrArray *out = ns_css_parse_selector_list(text);
    if (out_valid)
        *out_valid = !g_sel_parse_error && !g_sel_ns_prefix && out->len > 0;
    g_sel_parse_error = FALSE;
    g_sel_ns_prefix = FALSE;
    return out;
}

gboolean
ns_css_selector_matches(const ns_css_selector *sel, const ns_node *el)
{
    return match_selector(sel, el);
}

static __thread guint64 g_sel_match_ops;
static __thread int      g_sel_match_depth;
static __thread int      g_sel_chain_depth;

#define NS_SEL_MATCH_BUDGET 8000000ull
#define NS_SEL_MATCH_MAX_CHAIN 1024

typedef enum css_chain_result {
    CSS_CHAIN_MATCHES,
    CSS_CHAIN_FAILS_LOCALLY,
    CSS_CHAIN_FAILS_ALL_SIBLINGS,
    CSS_CHAIN_FAILS_COMPLETELY,
} css_chain_result;

static css_chain_result match_complex_chain(const ns_css_selector *sel,
                                            int idx, const ns_node *cur);

static css_chain_result
match_compound_then_chain(const ns_css_selector *sel, int idx,
                          const ns_node *el)
{
    if (g_sel_chain_depth >= NS_SEL_MATCH_MAX_CHAIN)
        return CSS_CHAIN_FAILS_COMPLETELY;
    if (!match_simple(g_ptr_array_index(sel->compounds, idx), el))
        return CSS_CHAIN_FAILS_LOCALLY;
    g_sel_chain_depth++;
    css_chain_result r = match_complex_chain(sel, idx, el);
    g_sel_chain_depth--;
    return r;
}

static css_chain_result
match_complex_chain(const ns_css_selector *sel, int idx, const ns_node *cur)
{
    if (idx <= 0) return CSS_CHAIN_MATCHES;
    ns_css_comb comb = g_array_index(sel->combinators, ns_css_comb, idx);
    if (comb == NS_CSS_COMB_CHILD) {
        const ns_node *p = cur->parent;
        if (++g_sel_match_ops > NS_SEL_MATCH_BUDGET)
            return CSS_CHAIN_FAILS_COMPLETELY;
        if (!p) return CSS_CHAIN_FAILS_COMPLETELY;
        return match_compound_then_chain(sel, idx - 1, p);
    }
    if (comb == NS_CSS_COMB_ADJACENT) {
        const ns_node *s = cur->prev_sibling;
        while (s && s->kind != NS_NODE_ELEMENT) s = s->prev_sibling;
        if (++g_sel_match_ops > NS_SEL_MATCH_BUDGET)
            return CSS_CHAIN_FAILS_COMPLETELY;
        if (!s) return CSS_CHAIN_FAILS_ALL_SIBLINGS;
        return match_compound_then_chain(sel, idx - 1, s);
    }
    if (comb == NS_CSS_COMB_SIBLING) {
        int depth = 0;
        const ns_node *s = cur->prev_sibling;
        for (; s && depth++ < NS_DOM_MAX_DEPTH; s = s->prev_sibling) {
            if (++g_sel_match_ops > NS_SEL_MATCH_BUDGET)
                return CSS_CHAIN_FAILS_COMPLETELY;
            if (s->kind != NS_NODE_ELEMENT) continue;
            css_chain_result r = match_compound_then_chain(sel, idx - 1, s);
            if (r != CSS_CHAIN_FAILS_LOCALLY) return r;
        }
        return s ? CSS_CHAIN_FAILS_LOCALLY : CSS_CHAIN_FAILS_ALL_SIBLINGS;
    }
    int depth = 0;
    const ns_node *p = cur->parent;
    for (; p && depth++ < NS_DOM_MAX_DEPTH; p = p->parent) {
        if (p->kind == NS_NODE_DOCUMENT) return CSS_CHAIN_FAILS_COMPLETELY;
        if (++g_sel_match_ops > NS_SEL_MATCH_BUDGET)
            return CSS_CHAIN_FAILS_COMPLETELY;
        css_chain_result r = match_compound_then_chain(sel, idx - 1, p);
        if (r == CSS_CHAIN_MATCHES || r == CSS_CHAIN_FAILS_COMPLETELY)
            return r;
    }
    return p ? CSS_CHAIN_FAILS_LOCALLY : CSS_CHAIN_FAILS_COMPLETELY;
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
                css_attr_value_hash(a->name, a->value, strlen(a->value)), delta);
}

static void
css_ancestor_filter_update(const ns_node *el, int delta)
{
    if (el->name)
        css_ancestor_filter_count(
            css_identifier_hash('%', el->name, strlen(el->name)), delta);
    const char *id = ns_element_get_attr(el, "id");
    if (id)
        css_ancestor_filter_count(css_identifier_hash('#', id, strlen(id)),
                                  delta);
    const char *cls = ns_element_get_attr(el, "class");
    for (const char *c = cls; c && *c; ) {
        while (*c && is_ws(*c)) c++;
        const char *token = c;
        while (*c && !is_ws(*c)) c++;
        if (c > token)
            css_ancestor_filter_count(
                css_identifier_hash('.', token, (gsize)(c - token)), delta);
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

static gboolean
match_selector_structural(const ns_css_selector *sel, const ns_node *el)
{
    if (!sel || sel->compounds->len == 0) return FALSE;
    if (g_sel_match_depth == 0) g_sel_match_ops = 0;
    g_sel_match_depth++;
    int idx = (int)sel->compounds->len - 1;
    gboolean r = match_compound_then_chain(sel, idx, el) == CSS_CHAIN_MATCHES;
    g_sel_match_depth--;
    return r;
}

static gboolean
match_selector(const ns_css_selector *sel, const ns_node *el)
{
    if (!sel) return FALSE;
    if (sel->pseudo_element != NS_CSS_PE_NONE) return FALSE;
    return match_selector_structural(sel, el);
}

static gboolean
match_selector_for_pe(const ns_css_selector *sel, const ns_node *el,
                      ns_css_pseudo_element pe)
{
    if (!sel) return FALSE;
    if (sel->pseudo_element != pe) return FALSE;
    return match_selector_structural(sel, el);
}

static gboolean
selector_group_matches_with_scope(const GPtrArray *group, const ns_node *el,
                                  const ns_node *scope)
{
    const ns_node *prev = g_css_match_scope;
    g_css_match_scope = scope;
    gboolean matched = FALSE;
    for (guint i = 0; group && i < group->len; i++) {
        const ns_css_selector *sel = g_ptr_array_index(group, i);
        if (match_selector(sel, el)) {
            matched = TRUE;
            break;
        }
    }
    g_css_match_scope = prev;
    return matched;
}

static gboolean
css_scope_root_matches(const ns_css_scope *scope, const ns_node *el)
{
    return selector_group_matches_with_scope(scope ? scope->roots : NULL,
                                            el, g_css_match_scope);
}

static int
css_scope_hops(const ns_node *root, const ns_node *el)
{
    int hops = 0;
    for (const ns_node *n = el; n; n = n->parent, hops++)
        if (n == root) return hops;
    return INT_MAX;
}

static gboolean
css_scope_limit_excludes(const ns_css_scope *scope, const ns_node *root,
                         const ns_node *el)
{
    if (!scope || !scope->limits) return FALSE;
    for (const ns_node *n = el; n; n = n->parent) {
        if (n->kind == NS_NODE_ELEMENT &&
            selector_group_matches_with_scope(scope->limits, n, root))
            return TRUE;
        if (n == root) break;
    }
    return FALSE;
}

static gboolean
css_scope_contains(const ns_css_scope *scope, const ns_node *root,
                   const ns_node *el)
{
    if (!scope || !root || !el) return FALSE;
    if (css_scope_hops(root, el) == INT_MAX) return FALSE;
    return !css_scope_limit_excludes(scope, root, el);
}

static gboolean
css_scope_applies_to(const ns_css_scope *scope, const ns_node *el)
{
    for (const ns_node *root = el; root; root = root->parent) {
        if (root->kind != NS_NODE_ELEMENT) continue;
        if (!css_scope_root_matches(scope, root)) continue;
        if (css_scope_contains(scope, root, el)) return TRUE;
    }
    return FALSE;
}

static gboolean
rule_outer_scopes_apply(const ns_css_rule *r, guint upto,
                        const ns_node *root, const ns_node *el)
{
    for (guint i = 0; r && r->scopes && i < upto; i++) {
        const ns_css_scope *scope = g_ptr_array_index(r->scopes, i);
        if (!css_scope_applies_to(scope, root)) return FALSE;
        if (!css_scope_applies_to(scope, el)) return FALSE;
    }
    return TRUE;
}

static gboolean
rule_selector_matches(const ns_css_rule *r, const ns_css_selector *sel,
                      const ns_node *el, ns_css_pseudo_element pe,
                      int *scope_order)
{
    if (scope_order) *scope_order = 0;
    if (!r || !r->scopes || r->scopes->len == 0) {
        return pe == NS_CSS_PE_NONE ? match_selector(sel, el)
                                    : match_selector_for_pe(sel, el, pe);
    }
    guint inner_i = r->scopes->len - 1;
    const ns_css_scope *inner = g_ptr_array_index(r->scopes, inner_i);
    int best = 0;
    for (const ns_node *root = el; root; root = root->parent) {
        if (root->kind != NS_NODE_ELEMENT) continue;
        if (!css_scope_root_matches(inner, root)) continue;
        if (!css_scope_contains(inner, root, el)) continue;
        if (!rule_outer_scopes_apply(r, inner_i, root, el)) continue;
        const ns_node *prev = g_css_match_scope;
        g_css_match_scope = root;
        gboolean matched = pe == NS_CSS_PE_NONE
            ? match_selector(sel, el)
            : match_selector_for_pe(sel, el, pe);
        g_css_match_scope = prev;
        if (!matched) continue;
        int hops = css_scope_hops(root, el);
        if (hops != INT_MAX) {
            int order = INT_MAX - hops;
            if (order > best) best = order;
        }
    }
    if (best <= 0) return FALSE;
    if (scope_order) *scope_order = best;
    return TRUE;
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

static gboolean
value_lerp_lengths(const ns_css_value *a, const ns_css_value *b, double t,
                   ns_css_value *out)
{
    if (a->kind == NS_CSS_V_LENGTH && b->kind == NS_CSS_V_LENGTH) {
        if (a->u.length.unit != b->u.length.unit) {
            if (a->u.length.v == 0 && a->u.length.unit != NS_CSS_UNIT_PERCENT &&
                b->u.length.unit != NS_CSS_UNIT_NUMBER && b->u.length.unit != NS_CSS_UNIT_PERCENT) {
                out->kind = NS_CSS_V_LENGTH;
                out->u.length.unit = b->u.length.unit;
                out->u.length.v = b->u.length.v * t;
                return TRUE;
            }
            if (b->u.length.v == 0 && b->u.length.unit != NS_CSS_UNIT_PERCENT &&
                a->u.length.unit != NS_CSS_UNIT_NUMBER && a->u.length.unit != NS_CSS_UNIT_PERCENT) {
                out->kind = NS_CSS_V_LENGTH;
                out->u.length.unit = a->u.length.unit;
                out->u.length.v = a->u.length.v * (1 - t);
                return TRUE;
            }
            return FALSE;
        }
        out->kind = NS_CSS_V_LENGTH;
        out->u.length.unit = a->u.length.unit;
        out->u.length.v = a->u.length.v + (b->u.length.v - a->u.length.v) * t;
        return TRUE;
    }
    if ((a->kind == NS_CSS_V_LENGTH || a->kind == NS_CSS_V_CALC) &&
        (b->kind == NS_CSS_V_LENGTH || b->kind == NS_CSS_V_CALC)) {
        double apx = 0, apct = 0, aem = 0, bpx = 0, bpct = 0, bem = 0;
        if (a->kind == NS_CSS_V_CALC) { apx = a->u.calc.px; apct = a->u.calc.pct; aem = a->u.calc.em; if (a->u.calc.fn) return FALSE; }
        else if (a->u.length.unit == NS_CSS_UNIT_PX) apx = a->u.length.v;
        else if (a->u.length.unit == NS_CSS_UNIT_PERCENT) apct = a->u.length.v;
        else if (a->u.length.unit == NS_CSS_UNIT_EM) aem = a->u.length.v;
        else return FALSE;
        if (b->kind == NS_CSS_V_CALC) { bpx = b->u.calc.px; bpct = b->u.calc.pct; bem = b->u.calc.em; if (b->u.calc.fn) return FALSE; }
        else if (b->u.length.unit == NS_CSS_UNIT_PX) bpx = b->u.length.v;
        else if (b->u.length.unit == NS_CSS_UNIT_PERCENT) bpct = b->u.length.v;
        else if (b->u.length.unit == NS_CSS_UNIT_EM) bem = b->u.length.v;
        else return FALSE;
        out->kind = NS_CSS_V_CALC;
        out->u.calc.px = apx + (bpx - apx) * t;
        out->u.calc.pct = apct + (bpct - apct) * t;
        out->u.calc.em = aem + (bem - aem) * t;
        return TRUE;
    }
    return FALSE;
}

static guint8
lerp_channel(guint8 a, guint8 b, double t)
{
    double v = a + ((double)b - a) * t;
    if (v < 0) v = 0;
    if (v > 255) v = 255;
    return (guint8)(v + 0.5);
}

static gboolean
keyword_number(const char *kw, double *out, gboolean *is_int)
{
    if (!kw || !*kw) return FALSE;
    if (strcmp(kw, "bold") == 0) { *out = 700; *is_int = TRUE; return TRUE; }
    if (strcmp(kw, "normal") == 0) { *out = 400; *is_int = TRUE; return TRUE; }
    char *end = NULL;
    double v = g_ascii_strtod(kw, &end);
    if (end == kw || *end != '\0') return FALSE;
    *out = v;
    *is_int = strchr(kw, '.') == NULL && strchr(kw, 'e') == NULL;
    return TRUE;
}

static gboolean keyword_lerp(const char *ka, const char *kb, double t, char **out);

static gboolean
keyword_lerp_list(const char *ka, const char *kb, double t, char **out)
{
    char **pa = g_strsplit(ka, " ", -1);
    char **pb = g_strsplit(kb, " ", -1);
    guint na = g_strv_length(pa), nb = g_strv_length(pb);
    gboolean ok = na == nb && na > 1;
    GString *res = g_string_new(NULL);
    for (guint i = 0; i < na && ok; i++) {
        char *part = NULL;
        if (!*pa[i] || !*pb[i]) { ok = *pa[i] == *pb[i]; if (ok) continue; break; }
        if (!keyword_lerp(pa[i], pb[i], t, &part)) { ok = FALSE; break; }
        if (res->len) g_string_append_c(res, ' ');
        g_string_append(res, part);
        g_free(part);
    }
    g_strfreev(pa);
    g_strfreev(pb);
    if (!ok) {
        g_string_free(res, TRUE);
        return FALSE;
    }
    *out = g_string_free(res, FALSE);
    return TRUE;
}

static gboolean
keyword_lerp(const char *ka, const char *kb, double t, char **out)
{
    if (!ka || !kb) return FALSE;
    if (strchr(ka, ' ') || strchr(kb, ' ')) return keyword_lerp_list(ka, kb, t, out);
    double na, nb;
    gboolean ia, ib;
    if (keyword_number(ka, &na, &ia) && keyword_number(kb, &nb, &ib)) {
        double r = na + (nb - na) * t;
        if (ia && ib) r = round(r);
        *out = g_strdup_printf("%g", r);
        return TRUE;
    }
    double va, vb;
    ns_css_unit ua, ub;
    if (ns_css_parse_length(ka, &va, &ua) && ns_css_parse_length(kb, &vb, &ub) &&
        ua == ub && ua != NS_CSS_UNIT_NUMBER) {
        double r = va + (vb - va) * t;
        const char *unit = ka + strlen(ka);
        while (unit > ka && !g_ascii_isdigit((guchar)unit[-1]) && unit[-1] != '.') unit--;
        *out = g_strdup_printf("%g%s", r, unit);
        return TRUE;
    }
    return FALSE;
}

static void
translate_axis_lerp(const ns_css_transform_op *x, const ns_css_transform_op *y,
                    double t, int axis, ns_css_transform_op *o)
{
    gboolean xp = axis == 0 ? x->a_is_percent : x->b_is_percent;
    gboolean yp = axis == 0 ? y->a_is_percent : y->b_is_percent;
    double xv = axis == 0 ? x->a : x->b;
    double yv = axis == 0 ? y->a : y->b;
    double xpct = (xp ? xv : 0) + (axis == 0 ? x->a_pct : x->b_pct);
    double ypct = (yp ? yv : 0) + (axis == 0 ? y->a_pct : y->b_pct);
    double xpx = xp ? 0 : xv;
    double ypx = yp ? 0 : yv;
    gboolean pure_percent = xp && yp;
    double v = pure_percent ? xv + (yv - xv) * t : xpx + (ypx - xpx) * t;
    double pct = pure_percent ? 0 : xpct + (ypct - xpct) * t;
    if (axis == 0) {
        o->a = v;
        o->a_is_percent = pure_percent;
        o->a_pct = pct;
    } else {
        o->b = v;
        o->b_is_percent = pure_percent;
        o->b_pct = pct;
    }
}

static ns_css_value *
transform_identity_like(const ns_css_value *src)
{
    if (!src || src->kind != NS_CSS_V_TRANSFORM) return NULL;
    ns_css_value *v = g_new0(ns_css_value, 1);
    v->kind = NS_CSS_V_TRANSFORM;
    v->u.transform = src->u.transform;
    for (int i = 0; i < v->u.transform.n_ops; i++) {
        ns_css_transform_op *op = &v->u.transform.ops[i];
        switch (op->kind) {
        case NS_CSS_TFN_TRANSLATE:
            op->a = op->b = op->c = op->a_pct = op->b_pct = 0;
            memset(op->em, 0, sizeof op->em);
            memset(op->rem, 0, sizeof op->rem);
            break;
        case NS_CSS_TFN_ROTATE:    op->a = 0; break;
        case NS_CSS_TFN_ROTATE3D:  op->d = 0; break;
        case NS_CSS_TFN_SCALE:     op->a = op->b = op->c = 1; break;
        case NS_CSS_TFN_SKEW:      op->a = op->b = 0; break;
        case NS_CSS_TFN_MATRIX:
            op->a = 1; op->b = 0; op->c = 0; op->d = 1; op->e = 0; op->f = 0;
            break;
        case NS_CSS_TFN_MATRIX3D:
            for (int k = 0; k < 16; k++) op->m3d[k] = (k % 5 == 0) ? 1 : 0;
            break;
        default:
            ns_css_value_free(v);
            return NULL;
        }
    }
    return v;
}

static gboolean
value_is_none_keyword(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "none") == 0;
}

ns_css_value *
ns_css_value_interpolate(const ns_css_value *a, const ns_css_value *b, double t)
{
    if (!a || !b) return NULL;
    if (value_is_none_keyword(a) && b->kind == NS_CSS_V_TRANSFORM) {
        ns_css_value *ida = transform_identity_like(b);
        ns_css_value *r = ida ? ns_css_value_interpolate(ida, b, t) : NULL;
        ns_css_value_free(ida);
        return r;
    }
    if (value_is_none_keyword(b) && a->kind == NS_CSS_V_TRANSFORM) {
        ns_css_value *idb = transform_identity_like(a);
        ns_css_value *r = idb ? ns_css_value_interpolate(a, idb, t) : NULL;
        ns_css_value_free(idb);
        return r;
    }
    ns_css_value *out = g_new0(ns_css_value, 1);
    if (value_lerp_lengths(a, b, t, out)) return out;
    if (a->kind != b->kind) { g_free(out); return NULL; }
    switch (a->kind) {
    case NS_CSS_V_KEYWORD: {
        if (keyword_lerp(a->u.keyword, b->u.keyword, t, &out->u.keyword)) {
            out->kind = NS_CSS_V_KEYWORD;
            return out;
        }
        break;
    }
    case NS_CSS_V_RECT: {
        out->kind = NS_CSS_V_RECT;
        for (int i = 0; i < 4; i++) {
            if (a->u.rect.is_auto[i] != b->u.rect.is_auto[i] ||
                (!a->u.rect.is_auto[i] && a->u.rect.unit[i] != b->u.rect.unit[i])) {
                g_free(out);
                return NULL;
            }
            out->u.rect.is_auto[i] = a->u.rect.is_auto[i];
            out->u.rect.unit[i] = a->u.rect.unit[i];
            out->u.rect.v[i] = a->u.rect.v[i] + (b->u.rect.v[i] - a->u.rect.v[i]) * t;
        }
        return out;
    }
    case NS_CSS_V_COLOR:
        out->kind = NS_CSS_V_COLOR;
        out->u.color.r = lerp_channel(a->u.color.r, b->u.color.r, t);
        out->u.color.g = lerp_channel(a->u.color.g, b->u.color.g, t);
        out->u.color.b = lerp_channel(a->u.color.b, b->u.color.b, t);
        out->u.color.a = lerp_channel(a->u.color.a, b->u.color.a, t);
        return out;
    case NS_CSS_V_SHADOW: {
        const ns_css_shadow_list *sa = &a->u.shadow, *sb = &b->u.shadow;
        if (sa->n != sb->n || sa->is_text != sb->is_text) break;
        for (int i = 0; i < sa->n; i++)
            if (sa->s[i].inset != sb->s[i].inset) { g_free(out); return NULL; }
        out->kind = NS_CSS_V_SHADOW;
        out->u.shadow.n = sa->n;
        out->u.shadow.is_text = sa->is_text;
        for (int i = 0; i < sa->n; i++) {
            const ns_css_shadow *x = &sa->s[i], *y = &sb->s[i];
            ns_css_shadow *o = &out->u.shadow.s[i];
            o->x = x->x + (y->x - x->x) * t;
            o->y = x->y + (y->y - x->y) * t;
            o->blur = x->blur + (y->blur - x->blur) * t;
            o->spread = x->spread + (y->spread - x->spread) * t;
            o->r = lerp_channel(x->r, y->r, t);
            o->g = lerp_channel(x->g, y->g, t);
            o->b = lerp_channel(x->b, y->b, t);
            o->a = lerp_channel(x->a, y->a, t);
            o->inset = x->inset;
        }
        return out;
    }
    case NS_CSS_V_TRANSFORM: {
        const ns_css_transform *ta = &a->u.transform, *tb = &b->u.transform;
        if (ta->n_ops != tb->n_ops) break;
        for (int i = 0; i < ta->n_ops; i++)
            if (ta->ops[i].kind != tb->ops[i].kind) { g_free(out); return NULL; }
        out->kind = NS_CSS_V_TRANSFORM;
        out->u.transform.n_ops = ta->n_ops;
        for (int i = 0; i < ta->n_ops; i++) {
            const ns_css_transform_op *x = &ta->ops[i], *y = &tb->ops[i];
            ns_css_transform_op *o = &out->u.transform.ops[i];
            *o = *x;
            o->a = x->a + (y->a - x->a) * t;
            o->b = x->b + (y->b - x->b) * t;
            o->c = x->c + (y->c - x->c) * t;
            o->d = x->d + (y->d - x->d) * t;
            o->e = x->e + (y->e - x->e) * t;
            o->f = x->f + (y->f - x->f) * t;
            for (int k = 0; k < 16; k++)
                o->m3d[k] = x->m3d[k] + (y->m3d[k] - x->m3d[k]) * t;
            if (x->kind != NS_CSS_TFN_TRANSLATE) continue;
            translate_axis_lerp(x, y, t, 0, o);
            translate_axis_lerp(x, y, t, 1, o);
            for (int k = 0; k < 3; k++) {
                o->em[k] = x->em[k] + (y->em[k] - x->em[k]) * t;
                o->rem[k] = x->rem[k] + (y->rem[k] - x->rem[k]) * t;
            }
        }
        return out;
    }
    default:
        break;
    }
    g_free(out);
    return NULL;
}

gboolean
ns_css_value_equal(const ns_css_value *a, const ns_css_value *b)
{
    if (a == b) return TRUE;
    if (!a || !b) return FALSE;
    char *sa = ns_css_value_serialize(a);
    char *sb = ns_css_value_serialize(b);
    gboolean eq = g_strcmp0(sa, sb) == 0;
    g_free(sa);
    g_free(sb);
    return eq;
}

GArray *
ns_css_parse_declarations(const char *text)
{
    GArray *decls = g_array_new(FALSE, FALSE, sizeof(ns_css_decl));
    if (!text) return decls;
    const char *p = text;
    parse_declaration_block(&p, text + strlen(text), decls, NULL);
    return decls;
}

void
ns_css_declarations_free(GArray *decls)
{
    if (!decls) return;
    for (guint i = 0; i < decls->len; i++)
        ns_css_value_free(g_array_index(decls, ns_css_decl, i).value);
    g_array_free(decls, TRUE);
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

static char *
value_serialize_one(const ns_css_value *v)
{
    if (!v) return g_strdup("");
    switch (v->kind) {
    case NS_CSS_V_KEYWORD:
        return g_strdup(v->u.keyword ? v->u.keyword : "");
    case NS_CSS_V_COLOR:
        return ns_css_color_text(v->u.color.r, v->u.color.g, v->u.color.b,
                                 v->u.color.a);
    case NS_CSS_V_LENGTH: {
        const char *unit = ns_css_unit_suffix(v->u.length.unit);
        return g_strdup_printf("%g%s", v->u.length.v, unit);
    }
    case NS_CSS_V_SIZE: {
        if (v->u.size.w_unit == NS_CSS_UNIT_NUMBER &&
            v->u.size.h_unit == NS_CSS_UNIT_NUMBER && !v->u.size.h_auto)
            return g_strdup_printf(v->u.size.w_auto ? "auto %g / %g" : "%g / %g",
                                   v->u.size.w, v->u.size.h);
        GString *s = g_string_new(NULL);
        if (v->u.size.w_auto) {
            g_string_append(s, "auto");
        } else {
            const char *unit = ns_css_unit_suffix(v->u.size.w_unit);
            g_string_append_printf(s, "%g%s", v->u.size.w, unit);
        }
        g_string_append_c(s, ' ');
        if (v->u.size.h_auto) {
            g_string_append(s, "auto");
        } else {
            const char *unit = ns_css_unit_suffix(v->u.size.h_unit);
            g_string_append_printf(s, "%g%s", v->u.size.h, unit);
        }
        return g_string_free(s, FALSE);
    }
    case NS_CSS_V_CALC: {
        const struct { double v; const char *unit; } part[4] = {
            { v->u.calc.pct, "%" }, { v->u.calc.em, "em" },
            { v->u.calc.px, "px" }, { v->u.calc.rem, "rem" },
        };
        int used = 0, only = 0;
        for (int i = 0; i < 4; i++)
            if (part[i].v != 0) { used++; only = i; }
        if (used == 0) return g_strdup("0px");
        if (used == 1)
            return g_strdup_printf("%g%s", part[only].v, part[only].unit);
        GString *s = g_string_new("calc(");
        gboolean first = TRUE;
        for (int i = 0; i < 4; i++) {
            if (part[i].v == 0) continue;
            if (!first) g_string_append(s, part[i].v < 0 ? " - " : " + ");
            g_string_append_printf(s, "%g%s",
                                   first ? part[i].v : fabs(part[i].v),
                                   part[i].unit);
            first = FALSE;
        }
        g_string_append_c(s, ')');
        return g_string_free(s, FALSE);
    }
    case NS_CSS_V_SHADOW:
        return ns_css_shadow_serialize(&v->u.shadow);
    case NS_CSS_V_GRADIENT:
        return ns_css_gradient_serialize(&v->u.gradient);
    case NS_CSS_V_TRACKS: {
        if (v->u.tracks.subgrid)
            return g_strdup(v->specified ? v->specified : "subgrid");
        GString *s = g_string_new(NULL);
        for (int i = 0; i <= v->u.tracks.n; i++) {
            gboolean open = FALSE;
            for (int k = 0; k < v->u.tracks.n_line_names; k++) {
                if (v->u.tracks.line_names[k].line != i + 1) continue;
                g_string_append(s, open ? " " : (s->len ? " [" : "["));
                g_string_append(s, v->u.tracks.line_names[k].name);
                open = TRUE;
            }
            if (open) g_string_append_c(s, ']');
            if (i == v->u.tracks.n) break;
            if (s->len) g_string_append_c(s, ' ');
            const ns_css_track *t = &v->u.tracks.tracks[i];
            switch (t->kind) {
            case NS_CSS_TRACK_PX:      g_string_append_printf(s, "%gpx", t->v); break;
            case NS_CSS_TRACK_PERCENT: g_string_append_printf(s, "%g%%", t->v); break;
            case NS_CSS_TRACK_FR:      g_string_append_printf(s, "%gfr", t->v); break;
            case NS_CSS_TRACK_AUTO:    g_string_append(s, "auto"); break;
            case NS_CSS_TRACK_MIN_CONTENT:
                g_string_append(s, "min-content"); break;
            case NS_CSS_TRACK_MAX_CONTENT:
                g_string_append(s, "max-content"); break;
            }
        }
        return g_string_free(s, FALSE);
    }
    case NS_CSS_V_URL:
        if (v->image_set_text) return g_strdup(v->image_set_text);
        return g_strdup_printf("url(\"%s\")", v->u.url ? v->u.url : "");
    case NS_CSS_V_AREAS: {
        GString *s = g_string_new(NULL);
        for (int r = 0; r < v->u.areas.n_rows; r++) {
            if (r) g_string_append_c(s, ' ');
            g_string_append_c(s, '"');
            for (int c = 0; c < v->u.areas.n_cols; c++) {
                const char *name = ".";
                for (int k = 0; k < v->u.areas.n_rects; k++) {
                    const ns_css_area_rect *rect = &v->u.areas.rects[k];
                    if (r >= rect->r0 && r <= rect->r1 &&
                        c >= rect->c0 && c <= rect->c1) {
                        name = rect->name; break;
                    }
                }
                if (c) g_string_append_c(s, ' ');
                g_string_append(s, name);
            }
            g_string_append_c(s, '"');
        }
        return g_string_free(s, FALSE);
    }
    case NS_CSS_V_ANIM: {
        GString *s = g_string_new(NULL);
        for (int i = 0; i < v->u.anim.n; i++) {
            if (i) g_string_append(s, ", ");
            const ns_css_anim_entry *e = &v->u.anim.entries[i];
            if (e->name) g_string_append_printf(s, "%s ", e->name);
            g_string_append_printf(s, "%gms", e->duration_ms);
            if (e->delay_ms != 0)
                g_string_append_printf(s, " %gms", e->delay_ms);
        }
        return g_string_free(s, FALSE);
    }
    case NS_CSS_V_TRANSFORM:
        return ns_css_transform_serialize(&v->u.transform);
    case NS_CSS_V_RECT: {
        GString *s = g_string_new("rect(");
        for (int i = 0; i < 4; i++) {
            if (i) g_string_append(s, ", ");
            if (v->u.rect.is_auto[i])
                g_string_append(s, "auto");
            else
                g_string_append_printf(s, "%gpx", v->u.rect.v[i]);
        }
        g_string_append_c(s, ')');
        return g_string_free(s, FALSE);
    }
    }
    return g_strdup("");
}

static char *
value_serialize_layers(const ns_css_value *v, gboolean specified)
{
    if (!v) return g_strdup("");
    if (!v->next_layer && !(specified && v->specified))
        return value_serialize_one(v);
    GString *s = g_string_new(NULL);
    for (const ns_css_value *l = v; l; l = l->next_layer) {
        if (s->len) g_string_append(s, ", ");
        if (specified && l->specified) {
            g_string_append(s, l->specified);
            continue;
        }
        char *one = value_serialize_one(l);
        g_string_append(s, one);
        g_free(one);
    }
    return g_string_free(s, FALSE);
}

char *
ns_css_value_serialize(const ns_css_value *v)
{
    return value_serialize_layers(v, FALSE);
}

char *
ns_css_value_serialize_specified(const ns_css_value *v)
{
    return value_serialize_layers(v, TRUE);
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
                                      !g_css_match_scope;
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
                matched = rule_selector_matches(r, sel, el, pe,
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
        ns_custom_prop_wide prev_kind = custom_prop_wide_kind(prev->text);
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
    ns_custom_prop_wide kind = custom_prop_wide_kind(match->text);
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
            custom_prop_wide_kind(vm->text) == NS_CUSTOM_WIDE_NONE &&
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
    ns_custom_prop_wide kind = custom_prop_wide_kind(value_text);
    char *expanded = NULL;
    if (kind == NS_CUSTOM_WIDE_NONE && strstr(resolved->text, "var(")) {
        ns_var_map scope = { .ref = 1, .own = own,
                             .parent = (ns_var_map *)parent };
        expanded = substitute_vars_with(value_text, &scope, 0);
        kind = custom_prop_wide_kind(expanded);
    }
    if (kind == NS_CUSTOM_WIDE_REVERT ||
        kind == NS_CUSTOM_WIDE_REVERT_LAYER) {
        resolved = var_rollback_match(matches, (gint)index - 1, current, kind);
        if (resolved) {
            value_text = resolved->text;
            kind = custom_prop_wide_kind(value_text);
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
    ns_custom_prop_wide kind = custom_prop_wide_kind(value_text);
    char *expanded = NULL;
    if (kind == NS_CUSTOM_WIDE_NONE && strstr(resolved->text, "var(")) {
        ns_var_map scope = { .ref = 1, .own = vars,
                             .parent = (ns_var_map *)parent };
        expanded = substitute_vars_with(value_text, &scope, 0);
        kind = custom_prop_wide_kind(expanded);
    }
    if (kind == NS_CUSTOM_WIDE_REVERT ||
        kind == NS_CUSTOM_WIDE_REVERT_LAYER) {
        resolved = var_rollback_match(matches, (gint)index - 1, current, kind);
        if (resolved) {
            value_text = resolved->text;
            kind = custom_prop_wide_kind(value_text);
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

static char *
css_string_quote(const char *text)
{
    GString *out = g_string_new("\"");
    for (const char *p = text ? text : ""; *p; p++) {
        guchar c = (guchar)*p;
        if (c == '"' || c == '\\') {
            g_string_append_c(out, '\\');
            g_string_append_c(out, (char)c);
        } else if (c < 0x20 || c == 0x7f) {
            g_string_append_printf(out, "\\%x ", c);
        } else {
            g_string_append_c(out, (char)c);
        }
    }
    g_string_append_c(out, '"');
    return g_string_free(out, FALSE);
}

static gboolean
attr_unit_ident_valid(const char *unit)
{
    static const char *const units[] = {
        "px", "em", "rem", "ex", "rex", "ch", "rch", "cap", "rcap", "ic", "ric",
        "lh", "rlh", "vw", "vh", "vi", "vb", "vmin", "vmax", "svw", "svh", "svi",
        "svb", "svmin", "svmax", "lvw", "lvh", "lvi", "lvb", "lvmin", "lvmax",
        "dvw", "dvh", "dvi", "dvb", "dvmin", "dvmax", "cqw", "cqh", "cqi", "cqb",
        "cqmin", "cqmax", "cm", "mm", "q", "in", "pt", "pc", "deg", "grad", "rad",
        "turn", "s", "ms", "hz", "khz", "dpi", "dpcm", "dppx", "x", "fr", "%",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(units); i++)
        if (g_ascii_strcasecmp(unit, units[i]) == 0) return TRUE;
    return FALSE;
}

static char *substitute_attrs(const char *text, const ns_node *node, int depth,
                              gboolean *tainted);

static char *
attr_function_value(const char *args, const ns_node *node, int depth,
                    gboolean *invalid)
{
    const char *end = args + strlen(args);
    const char *comma = css_find_top_level_char(args, end, ',');
    char *head = css_trim_dup_range(args, comma ? comma : end);
    char *fallback = comma ? css_trim_dup_range(comma + 1, end) : NULL;
    char *toks[4] = { 0 };
    int n = ns_css_split_ws_paren(head, toks, 4);
    char *result = NULL;
    *invalid = FALSE;
    if (n < 1 || n > 2 || !ns_css_content_ident_valid(toks[0])) {
        *invalid = TRUE;
        goto done;
    }
    const char *type = n == 2 ? toks[1] : NULL;
    enum { ATTR_STRING, ATTR_ANY, ATTR_SYNTAX, ATTR_UNIT } kind = ATTR_STRING;
    ns_css_syntax_def *syntax = NULL;
    if (type) {
        gsize tlen = strlen(type);
        if (g_ascii_strcasecmp(type, "raw-string") == 0) {
            kind = ATTR_STRING;
        } else if (g_ascii_strncasecmp(type, "type(", 5) == 0 && type[tlen - 1] == ')') {
            char *inner = g_strstrip(g_strndup(type + 5, tlen - 6));
            if (strcmp(inner, "*") == 0) {
                kind = ATTR_ANY;
            } else {
                syntax = ns_css_syntax_def_parse(inner);
                if (!syntax || strchr(inner, '<') == NULL) {
                    g_free(inner);
                    *invalid = TRUE;
                    goto done;
                }
                kind = ATTR_SYNTAX;
            }
            g_free(inner);
        } else if (attr_unit_ident_valid(type)) {
            kind = ATTR_UNIT;
        } else {
            *invalid = TRUE;
            goto done;
        }
    }
    char *lname = g_ascii_strdown(toks[0], -1);
    const char *raw = node && node->kind == NS_NODE_ELEMENT
        ? ns_element_get_attr(node, lname) : NULL;
    g_free(lname);
    if (raw) {
        char *value = g_strstrip(g_strdup(raw));
        switch (kind) {
        case ATTR_STRING:
            result = css_string_quote(raw);
            break;
        case ATTR_ANY:
            if (*value && css_declaration_value_syntax_valid(value) &&
                !strstr(value, "var(") && !strstr(value, "attr("))
                result = g_strdup(value);
            break;
        case ATTR_SYNTAX:
            if (*value && !strstr(value, "var(") && !strstr(value, "attr(") &&
                ns_css_syntax_def_matches(syntax, value))
                result = g_strdup(value);
            break;
        case ATTR_UNIT: {
            char *e = NULL;
            double num = g_ascii_strtod(value, &e);
            if (e != value && *e == '\0') {
                char *ns = ns_css_number_str(num);
                result = g_strconcat(ns, type, NULL);
                g_free(ns);
            }
            break;
        }
        }
        g_free(value);
    }
    if (syntax) ns_css_syntax_def_free(syntax);
    if (!result) {
        if (fallback && depth < 8) result = substitute_attrs(fallback, node, depth + 1, NULL);
        else if (!fallback && kind == ATTR_STRING) result = g_strdup("\"\"");
        if (!result) *invalid = TRUE;
    }
done:
    for (int i = 0; i < n; i++) g_free(toks[i]);
    g_free(head);
    g_free(fallback);
    return result;
}

static gboolean
url_function_at(const char *text, const char *open_paren)
{
    const char *q = open_paren;
    while (q > text && (is_ident(q[-1]) || q[-1] == '-')) q--;
    gsize len = (gsize)(open_paren - q);
    return (len == 3 && g_ascii_strncasecmp(q, "url", 3) == 0) ||
           (len == 3 && g_ascii_strncasecmp(q, "src", 3) == 0) ||
           (len == 5 && g_ascii_strncasecmp(q, "image", 5) == 0) ||
           (len == 9 && g_ascii_strncasecmp(q, "image-set", 9) == 0) ||
           (len == 17 && g_ascii_strncasecmp(q, "-webkit-image-set", 17) == 0);
}

static char *
substitute_attrs(const char *text, const ns_node *node, int depth,
                 gboolean *tainted)
{
    if (!text) return NULL;
    if (!strstr(text, "attr(")) return g_strdup(text);
    GString *out = g_string_new(NULL);
    const char *p = text;
    const char *end = text + strlen(text);
    int url_depth = 0;
    int depth_stack[64];
    int stack_n = 0;
    while (p < end) {
        if (*p == '"' || *p == '\'') {
            const char *q = ns_css_quoted_end(p + 1, *p);
            if (!q) { g_string_append(out, p); break; }
            g_string_append_len(out, p, q + 1 - p);
            p = q + 1;
            continue;
        }
        if (*p == '(' && stack_n < 64) {
            depth_stack[stack_n++] = url_function_at(text, p);
            if (depth_stack[stack_n - 1]) url_depth++;
        } else if (*p == ')' && stack_n > 0) {
            if (depth_stack[--stack_n]) url_depth--;
        }
        if (g_ascii_strncasecmp(p, "attr(", 5) == 0 &&
            (p == text || !is_ident(p[-1]))) {
            if (url_depth > 0 && tainted) *tainted = TRUE;
            int d = 1;
            const char *q = p + 5;
            while (q < end && d > 0) {
                if (*q == '"' || *q == '\'') {
                    const char *qe = ns_css_quoted_end(q + 1, *q);
                    if (!qe) break;
                    q = qe + 1;
                    continue;
                }
                if (*q == '(') d++;
                else if (*q == ')') d--;
                if (d > 0) q++;
            }
            if (d != 0) { g_string_free(out, TRUE); return NULL; }
            char *args = g_strndup(p + 5, (gsize)(q - (p + 5)));
            gboolean invalid = FALSE;
            char *val = attr_function_value(args, node, depth, &invalid);
            g_free(args);
            if (invalid || !val) {
                g_free(val);
                g_string_free(out, TRUE);
                return NULL;
            }
            if (tainted && (strstr(val, "url(") || strstr(val, "src(") ||
                            strstr(val, "image-set(")))
                *tainted = TRUE;
            g_string_append(out, val);
            g_free(val);
            p = q + 1;
            continue;
        }
        g_string_append_c(out, *p);
        p++;
    }
    return g_string_free(out, FALSE);
}

static gboolean
attr_args_syntax_valid(const char *args)
{
    const char *end = args + strlen(args);
    const char *comma = css_find_top_level_char(args, end, ',');
    char *head = css_trim_dup_range(args, comma ? comma : end);
    char *toks[4] = { 0 };
    int n = ns_css_split_ws_paren(head, toks, 4);
    gboolean ok = n >= 1 && n <= 2 && ns_css_content_ident_valid(toks[0]);
    if (ok && n == 2) {
        const char *type = toks[1];
        gsize tlen = strlen(type);
        if (g_ascii_strcasecmp(type, "raw-string") == 0) ok = TRUE;
        else if (g_ascii_strncasecmp(type, "type(", 5) == 0 && type[tlen - 1] == ')') {
            char *inner = g_strstrip(g_strndup(type + 5, tlen - 6));
            if (strcmp(inner, "*") != 0) {
                ns_css_syntax_def *syntax = ns_css_syntax_def_parse(inner);
                ok = syntax != NULL && strchr(inner, '<') != NULL;
                if (syntax) ns_css_syntax_def_free(syntax);
            }
            g_free(inner);
        } else ok = attr_unit_ident_valid(type);
    }
    if (ok && comma) {
        char *fallback = css_trim_dup_range(comma + 1, end);
        ok = attr_functions_syntax_valid(fallback);
        g_free(fallback);
    }
    for (int i = 0; i < n; i++) g_free(toks[i]);
    g_free(head);
    return ok;
}

static gboolean
attr_functions_syntax_valid(const char *text)
{
    const char *p = text;
    const char *end = text + strlen(text);
    while (p < end) {
        if (*p == '"' || *p == '\'') {
            const char *q = ns_css_quoted_end(p + 1, *p);
            if (!q) return TRUE;
            p = q + 1;
            continue;
        }
        if (g_ascii_strncasecmp(p, "attr(", 5) == 0 && (p == text || !is_ident(p[-1]))) {
            int d = 1;
            const char *q = p + 5;
            while (q < end && d > 0) {
                if (*q == '"' || *q == '\'') {
                    const char *qe = ns_css_quoted_end(q + 1, *q);
                    if (!qe) return FALSE;
                    q = qe + 1;
                    continue;
                }
                if (*q == '(') d++;
                else if (*q == ')') d--;
                if (d > 0) q++;
            }
            if (d != 0) return FALSE;
            char *args = g_strndup(p + 5, (gsize)(q - (p + 5)));
            gboolean ok = attr_args_syntax_valid(args);
            g_free(args);
            if (!ok) return FALSE;
            p = q + 1;
            continue;
        }
        p++;
    }
    return TRUE;
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
    parse_declaration_block(&sp, synth + strlen(synth), temp, NULL);
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
    char *substituted = substitute_vars_with(pm->pd->raw_vtext, vars, 0);
    if (substituted && strstr(substituted, "attr(")) {
        gboolean tainted = FALSE;
        char *with_attrs = substitute_attrs(substituted, node, 0, &tainted);
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

static gboolean
parse_legacy_color(const char *input, guint8 *r_out, guint8 *g_out, guint8 *b_out)
{
    if (!input || !*input) return FALSE;

    GString *s = g_string_new(NULL);
    for (const char *p = input; *p; ) {
        gunichar c = g_utf8_get_char(p);
        const char *next = g_utf8_next_char(p);
        if (c > 0xFFFF) g_string_append(s, "00");
        else            g_string_append_len(s, p, next - p);
        p = next;
    }

    glong m = g_utf8_strlen(s->str, -1);
    if (m > 128) m = 128;

    GString *hex = g_string_new(NULL);
    const char *p = s->str;
    for (glong i = 0; i < m; i++, p = g_utf8_next_char(p)) {
        gunichar c = g_utf8_get_char(p);
        if (i == 0 && c == '#') continue;
        if (c < 128 && g_ascii_isxdigit((char)c))
            g_string_append_c(hex, (char)c);
        else
            g_string_append_c(hex, '0');
    }
    g_string_free(s, TRUE);

    if (hex->len == 0) g_string_append_c(hex, '0');
    while (hex->len % 3 != 0) g_string_append_c(hex, '0');

    gsize comp = hex->len / 3;
    const char *c0 = hex->str, *c1 = hex->str + comp, *c2 = hex->str + 2 * comp;
    gsize off = 0, len = comp;
    if (len > 8) { off = len - 8; len = 8; }
    while (len > 2 && c0[off] == '0' && c1[off] == '0' && c2[off] == '0') {
        off++; len--;
    }
    if (len > 2) len = 2;

    guint rv = 0, gv = 0, bv = 0;
    for (gsize i = 0; i < len; i++) {
        rv = rv * 16 + (guint)g_ascii_xdigit_value(c0[off + i]);
        gv = gv * 16 + (guint)g_ascii_xdigit_value(c1[off + i]);
        bv = bv * 16 + (guint)g_ascii_xdigit_value(c2[off + i]);
    }
    g_string_free(hex, TRUE);

    *r_out = (guint8)rv; *g_out = (guint8)gv; *b_out = (guint8)bv;
    return TRUE;
}

static gboolean
attr_is_color(const char *v, guint8 *r_out, guint8 *g_out, guint8 *b_out, guint8 *a_out)
{
    if (!v) return FALSE;
    while (*v == ' ' || *v == '\t' || *v == '\n' || *v == '\f' || *v == '\r') v++;
    const char *end = v + strlen(v);
    while (end > v && (end[-1] == ' ' || end[-1] == '\t' || end[-1] == '\n' ||
                       end[-1] == '\f' || end[-1] == '\r'))
        end--;
    if (end == v) return FALSE;
    char *stripped = g_strndup(v, (gsize)(end - v));
    gboolean ok = ns_css_parse_color(stripped, r_out, g_out, b_out, a_out);
    if (!ok) {
        *a_out = 255;
        ok = parse_legacy_color(stripped, r_out, g_out, b_out);
    }
    g_free(stripped);
    return ok;
}

static gboolean
html_dimension_value(const char *s, gboolean ignore_zero, double *value,
                     gboolean *percent)
{
    if (!s) return FALSE;
    while (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\f' || *s == '\r')
        s++;
    if (!g_ascii_isdigit(*s)) return FALSE;
    double v = 0;
    while (g_ascii_isdigit(*s)) v = v * 10 + (*s++ - '0');
    if (*s == '.') {
        s++;
        double divisor = 1;
        while (g_ascii_isdigit(*s)) {
            divisor *= 10;
            v += (*s++ - '0') / divisor;
        }
    }
    if (ignore_zero && v == 0) return FALSE;
    *value = v;
    *percent = *s == '%';
    return TRUE;
}

static gboolean
cell_nowrap_quirk(const ns_node *cell)
{
    const ns_node *doc = ns_node_root(cell);
    if (!doc || !(doc->flags & NS_NODE_QUIRKS)) return FALSE;
    double v;
    gboolean pct;
    return html_dimension_value(ns_element_get_attr(cell, "width"), TRUE,
                                &v, &pct) && !pct;
}

static void
append_html_dimension(GString *out, const char *attr, gboolean ignore_zero,
                      const char *prop_a, const char *prop_b)
{
    double v;
    gboolean pct;
    if (!html_dimension_value(attr, ignore_zero, &v, &pct)) return;
    const char *unit = pct ? "%" : "px";
    g_string_append_printf(out, "%s: %.10g%s;", prop_a, v, unit);
    if (prop_b) g_string_append_printf(out, "%s: %.10g%s;", prop_b, v, unit);
}

enum {
    TABLE_RULES_NONE = 1,
    TABLE_RULES_GROUPS,
    TABLE_RULES_ROWS,
    TABLE_RULES_COLS,
    TABLE_RULES_ALL,
};

static int
table_rules_kind(const char *rules)
{
    static const char *const names[] = {
        "none", "groups", "rows", "cols", "all",
    };
    for (gsize i = 0; rules && i < G_N_ELEMENTS(names); i++)
        if (g_ascii_strcasecmp(rules, names[i]) == 0)
            return TABLE_RULES_NONE + (int)i;
    return 0;
}

static const char *
table_frame_border_style(const char *frame)
{
    static const struct { const char *name, *style; } frames[] = {
        { "void", "hidden" },
        { "above", "outset hidden hidden hidden" },
        { "below", "hidden hidden outset hidden" },
        { "hsides", "outset hidden outset hidden" },
        { "lhs", "hidden hidden hidden outset" },
        { "rhs", "hidden outset hidden hidden" },
        { "vsides", "hidden outset" },
        { "box", "outset" },
        { "border", "outset" },
    };
    for (gsize i = 0; frame && i < G_N_ELEMENTS(frames); i++)
        if (g_ascii_strcasecmp(frame, frames[i].name) == 0)
            return frames[i].style;
    return NULL;
}

static const ns_node *
table_of_part(const ns_node *el)
{
    const ns_node *p = el->parent;
    if (ns_node_is_element_named(el, "td") ||
        ns_node_is_element_named(el, "th")) {
        if (!ns_node_is_element_named(p, "tr")) return NULL;
        p = p->parent;
    }
    if ((ns_node_is_element_named(el, "td") ||
         ns_node_is_element_named(el, "th") ||
         ns_node_is_element_named(el, "tr")) &&
        (ns_node_is_element_named(p, "thead") ||
         ns_node_is_element_named(p, "tbody") ||
         ns_node_is_element_named(p, "tfoot")))
        p = p->parent;
    return ns_node_is_element_named(p, "table") ? p : NULL;
}

static const ns_node *
img_dimension_attribute_source(const ns_node *img)
{
    const ns_node *picture = img->parent;
    if (!ns_node_is_element_named(picture, "picture")) return img;
    for (const ns_node *c = picture->first_child; c && c != img;
         c = c->next_sibling) {
        if (!ns_node_is_element_named(c, "source")) continue;
        const char *srcset = ns_element_get_attr(c, "srcset");
        if (!srcset || !*srcset) continue;
        const char *media = ns_element_get_attr(c, "media");
        if (media && *media && !ns_css_media_query_matches(media)) continue;
        const char *type = ns_element_get_attr(c, "type");
        if (type && *type && !ns_image_supports_mime(type)) continue;
        return ns_element_get_attr(c, "width") ||
               ns_element_get_attr(c, "height") ? c : img;
    }
    return img;
}

static const char *
legacy_font_size_keyword(const char *s)
{
    static const char *const keywords[] = {
        "x-small", "small", "medium", "large", "x-large", "xx-large",
        "xxx-large",
    };
    if (!s) return NULL;
    while (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\f' || *s == '\r')
        s++;
    int sign = *s == '+' ? 1 : *s == '-' ? -1 : 0;
    if (sign) s++;
    if (!g_ascii_isdigit(*s)) return NULL;
    int value = 0;
    while (g_ascii_isdigit(*s) && value < 100) value = value * 10 + (*s++ - '0');
    if (sign) value = 3 + sign * value;
    return keywords[CLAMP(value, 1, 7) - 1];
}

static const char *const kSvgPresentationAttrs[] = {
    "fill", "fill-opacity", "fill-rule", "clip-rule",
    "stroke", "stroke-width", "stroke-opacity", "stroke-linecap",
    "stroke-linejoin", "stroke-miterlimit", "stroke-dasharray",
    "stroke-dashoffset", "paint-order", "vector-effect", "text-anchor",
    "stop-color", "stop-opacity", "visibility",
};

static gboolean
is_svg_presentation_attr_name(const char *n)
{
    for (gsize i = 0; i < G_N_ELEMENTS(kSvgPresentationAttrs); i++)
        if (strcmp(n, kSvgPresentationAttrs[i]) == 0) return TRUE;
    return FALSE;
}

static void
append_svg_presentation_hints(GString *out, const ns_node *el)
{
    for (const ns_attr *a = el->attrs; a; a = a->next) {
        if (!a->name || !a->value || !is_svg_presentation_attr_name(a->name))
            continue;
        char *value = g_strstrip(g_strdup(a->value));
        if (*value && !strpbrk(value, ";{}!\\")) {
            char *end = NULL;
            g_ascii_strtod(value, &end);
            gboolean unitless_length = end && end != value && *end == '\0' &&
                (strcmp(a->name, "stroke-width") == 0 ||
                 strcmp(a->name, "stroke-dashoffset") == 0);
            g_string_append_printf(out, "%s: %s%s;", a->name, value,
                                   unitless_length ? "px" : "");
        }
        g_free(value);
    }
}

static gboolean
is_presentational_attr_name(const char *n)
{
    if (!n || !*n) return FALSE;
    if (is_svg_presentation_attr_name(n)) return TRUE;
    switch (g_ascii_tolower((guchar)n[0])) {
    case 'a': return g_ascii_strcasecmp(n, "align") == 0;
    case 'b': return g_ascii_strcasecmp(n, "bgcolor") == 0 ||
                     g_ascii_strcasecmp(n, "bordercolor") == 0 ||
                     g_ascii_strcasecmp(n, "background") == 0 ||
                     g_ascii_strcasecmp(n, "border") == 0;
    case 'c': return g_ascii_strcasecmp(n, "color") == 0 ||
                     g_ascii_strcasecmp(n, "cellspacing") == 0 ||
                     g_ascii_strcasecmp(n, "cellpadding") == 0;
    case 'f': return g_ascii_strcasecmp(n, "face") == 0 ||
                     g_ascii_strcasecmp(n, "frame") == 0 ||
                     g_ascii_strcasecmp(n, "frameborder") == 0;
    case 'h': return g_ascii_strcasecmp(n, "height") == 0 ||
                     g_ascii_strcasecmp(n, "hspace") == 0;
    case 'l': return g_ascii_strcasecmp(n, "leftmargin") == 0;
    case 'm': return g_ascii_strcasecmp(n, "marginheight") == 0 ||
                     g_ascii_strcasecmp(n, "marginwidth") == 0;
    case 'n': return g_ascii_strcasecmp(n, "nowrap") == 0 ||
                     g_ascii_strcasecmp(n, "noshade") == 0;
    case 'r': return g_ascii_strcasecmp(n, "rules") == 0;
    case 's': return g_ascii_strcasecmp(n, "size") == 0;
    case 't': return g_ascii_strcasecmp(n, "text") == 0 ||
                     g_ascii_strcasecmp(n, "topmargin") == 0 ||
                     g_ascii_strcasecmp(n, "type") == 0;
    case 'v': return g_ascii_strcasecmp(n, "valign") == 0 ||
                     g_ascii_strcasecmp(n, "vspace") == 0;
    case 'w': return g_ascii_strcasecmp(n, "width") == 0 ||
                     g_ascii_strcasecmp(n, "wrap") == 0;
    default:  return FALSE;
    }
}

static char *
presentational_hints_css(const ns_node *el)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !el->name) return NULL;
    gboolean any = strcmp(el->name, "td") == 0 || strcmp(el->name, "th") == 0 ||
                   strcmp(el->name, "body") == 0 ||
                   (strcmp(el->name, "img") == 0 &&
                    ns_node_is_element_named(el->parent, "picture")) ||
                   strcmp(el->name, "tr") == 0 ||
                   strcmp(el->name, "thead") == 0 ||
                   strcmp(el->name, "tbody") == 0 ||
                   strcmp(el->name, "tfoot") == 0 ||
                   strcmp(el->name, "colgroup") == 0;
    for (const ns_attr *a = el->attrs; !any && a; a = a->next)
        if (is_presentational_attr_name(a->name)) any = TRUE;
    if (!any) return NULL;
    GString *out = g_string_new(NULL);
    const char *tag = el->name;
    gboolean is_table = strcmp(tag, "table") == 0;
    gboolean is_cell  = strcmp(tag, "td") == 0 || strcmp(tag, "th") == 0;
    gboolean is_row   = strcmp(tag, "tr") == 0;
    gboolean is_table_part = is_cell || is_row ||
        strcmp(tag, "thead") == 0 || strcmp(tag, "tbody") == 0 ||
        strcmp(tag, "tfoot") == 0 || strcmp(tag, "col") == 0 ||
        strcmp(tag, "colgroup") == 0;
    gboolean is_img   = strcmp(tag, "img") == 0;
    gboolean is_hr    = strcmp(tag, "hr") == 0;
    gboolean is_body  = strcmp(tag, "body") == 0;
    gboolean is_font  = strcmp(tag, "font") == 0;
    gboolean is_iframe = strcmp(tag, "iframe") == 0;
    gboolean is_video = strcmp(tag, "video") == 0;
    const char *input_type = strcmp(tag, "input") == 0
        ? ns_element_get_attr(el, "type") : NULL;
    gboolean is_image_input = input_type &&
                              g_ascii_strcasecmp(input_type, "image") == 0;
    gboolean is_embedded = is_img || is_image_input || is_iframe ||
        is_video || strcmp(tag, "object") == 0 ||
        strcmp(tag, "embed") == 0 || strcmp(tag, "marquee") == 0;

    if (strcmp(tag, "ol") == 0 || strcmp(tag, "li") == 0) {
        const char *t = ns_element_get_attr(el, "type");
        const char *lst = NULL;
        if (t) {
            if (strcmp(t, "1") == 0) lst = "decimal";
            else if (strcmp(t, "a") == 0) lst = "lower-alpha";
            else if (strcmp(t, "A") == 0) lst = "upper-alpha";
            else if (strcmp(t, "i") == 0) lst = "lower-roman";
            else if (strcmp(t, "I") == 0) lst = "upper-roman";
        }
        if (lst) g_string_append_printf(out, "list-style-type: %s;", lst);
    }
    if (strcmp(tag, "ul") == 0 || strcmp(tag, "li") == 0) {
        const char *t = ns_element_get_attr(el, "type");
        const char *lst = NULL;
        if (t) {
            if (g_ascii_strcasecmp(t, "disc") == 0) lst = "disc";
            else if (g_ascii_strcasecmp(t, "circle") == 0) lst = "circle";
            else if (g_ascii_strcasecmp(t, "square") == 0) lst = "square";
            else if (g_ascii_strcasecmp(t, "none") == 0) lst = "none";
        }
        if (lst) g_string_append_printf(out, "list-style-type: %s;", lst);
    }

    const char *background = ns_element_get_attr(el, "background");
    if (background && *background && (is_body || is_table || is_table_part)) {
        g_string_append(out, "background-image: url(\"");
        for (const char *p = background; *p; p++) {
            if (*p == '"' || *p == '\\')
                g_string_append_c(out, '\\');
            if (*p == '\n' || *p == '\r' || *p == '\f')
                continue;
            g_string_append_c(out, *p);
        }
        g_string_append(out, "\");");
    }
    const char *bgcolor = ns_element_get_attr(el, "bgcolor");
    if (bgcolor && *bgcolor) {
        guint8 r, g, b, a;
        if (attr_is_color(bgcolor, &r, &g, &b, &a))
            g_string_append_printf(out, "background-color: rgba(%u,%u,%u,%g);",
                                   r, g, b, a / 255.0);
    }
    if (is_body) {
        const ns_node *doc = el;
        while (doc && doc->kind != NS_NODE_DOCUMENT) doc = doc->parent;
        const ns_node *container = doc ? doc->parent : NULL;
        if (!ns_node_is_element_named(container, "iframe") &&
            !ns_node_is_element_named(container, "frame"))
            container = NULL;
        static const struct {
            const char *start, *end, *attr, *alt;
        } body_margins[] = {
            { "margin-top", "margin-bottom", "marginheight", "topmargin" },
            { "margin-left", "margin-right", "marginwidth", "leftmargin" },
        };
        for (gsize i = 0; i < G_N_ELEMENTS(body_margins); i++) {
            const char *v = ns_element_get_attr(el, body_margins[i].attr);
            if (!v) v = ns_element_get_attr(el, body_margins[i].alt);
            if (!v && container)
                v = ns_element_get_attr(container, body_margins[i].attr);
            int px = v ? ns_parse_int(v, -1, -1, G_MAXINT / 2) : -1;
            if (px >= 0)
                g_string_append_printf(out, "%s: %dpx; %s: %dpx;",
                                       body_margins[i].start, px,
                                       body_margins[i].end, px);
        }
        const char *text = ns_element_get_attr(el, "text");
        if (text && *text) {
            guint8 r, g, b, a;
            if (attr_is_color(text, &r, &g, &b, &a))
                g_string_append_printf(out, "color: rgba(%u,%u,%u,%g);",
                                       r, g, b, a / 255.0);
        }
    }
    if (is_font) {
        const char *color = ns_element_get_attr(el, "color");
        if (color && *color) {
            guint8 r, g, b, a;
            if (attr_is_color(color, &r, &g, &b, &a))
                g_string_append_printf(out, "color: rgba(%u,%u,%u,%g);",
                                       r, g, b, a / 255.0);
        }
        const char *face = ns_element_get_attr(el, "face");
        if (face && *face) {
            static const char *const generics[] = {
                "serif", "sans-serif", "monospace", "cursive", "fantasy",
                "system-ui", "ui-serif", "ui-sans-serif", "ui-monospace",
                "ui-rounded", "math", "emoji", "fangsong",
            };
            gboolean is_generic = FALSE;
            for (gsize i = 0; i < G_N_ELEMENTS(generics); i++)
                if (g_ascii_strcasecmp(face, generics[i]) == 0) {
                    is_generic = TRUE;
                    break;
                }
            if (is_generic) {
                g_string_append_printf(out, "font-family: %s;", face);
            } else {
                g_string_append(out, "font-family: \"");
                for (const unsigned char *p = (const unsigned char *)face; *p; p++) {
                    unsigned char c = *p;
                    if (c == '\\' || c == '"')
                        g_string_append_printf(out, "\\%c", c);
                    else if (c < 0x20 || c == 0x7f)
                        g_string_append_printf(out, "\\%X ", c);
                    else
                        g_string_append_c(out, (char)c);
                }
                g_string_append(out, "\";");
            }
        }
        const char *size = legacy_font_size_keyword(
            ns_element_get_attr(el, "size"));
        if (size) g_string_append_printf(out, "font-size: %s;", size);
    }

    const ns_node *dim_source = is_img ? img_dimension_attribute_source(el)
                                       : el;
    const char *width = ns_element_get_attr(dim_source, "width");
    if (width && (is_embedded || is_hr || strcmp(tag, "col") == 0 ||
                  strcmp(tag, "colgroup") == 0 || strcmp(tag, "pre") == 0))
        append_html_dimension(out, width, FALSE, "width", NULL);
    else if (width && (is_table || is_cell))
        append_html_dimension(out, width, TRUE, "width", NULL);
    const char *height = ns_element_get_attr(dim_source, "height");
    if (height && (is_embedded || is_table || is_row))
        append_html_dimension(out, height, FALSE, "height", NULL);
    else if (height && is_cell)
        append_html_dimension(out, height, TRUE, "height", NULL);
    if (is_iframe) {
        const char *frameborder = ns_element_get_attr(el, "frameborder");
        if (frameborder && ns_parse_int(frameborder, 0, G_MININT / 2,
                                        G_MAXINT / 2) == 0)
            g_string_append(out, "border-width: 0;");
    }
    if (width && height && (is_img || is_video || is_image_input)) {
        double aw, ah;
        gboolean apct, hpct;
        if (html_dimension_value(width, FALSE, &aw, &apct) && !apct &&
            html_dimension_value(height, FALSE, &ah, &hpct) && !hpct)
            g_string_append_printf(out, "aspect-ratio: auto %.10g / %.10g;",
                                   aw, ah);
    }
    if (is_embedded && !is_iframe && !is_video) {
        append_html_dimension(out, ns_element_get_attr(el, "hspace"), FALSE,
                              "margin-left", "margin-right");
        append_html_dimension(out, ns_element_get_attr(el, "vspace"), FALSE,
                              "margin-top", "margin-bottom");
    }
    if (strcmp(tag, "canvas") == 0 && width && height) {
        int cw = ns_parse_int(width, 0, 0, G_MAXINT);
        int ch = ns_parse_int(height, 0, 0, G_MAXINT);
        if (cw > 0 && ch > 0)
            g_string_append_printf(out, "aspect-ratio: auto %d / %d;", cw, ch);
    }
    if (is_table) {
        const char *rules = ns_element_get_attr(el, "rules");
        if (table_rules_kind(rules))
            g_string_append(out, "border-style: hidden;"
                                 "border-collapse: collapse;");
        const char *border = ns_element_get_attr(el, "border");
        if (border) {
            int w = ns_parse_int(border, -1, -1, G_MAXINT / 2);
            g_string_append_printf(out, "border-width: %dpx;", w < 0 ? 1 : w);
            if (w != 0) g_string_append(out, "border-style: outset;");
        }
        const char *frame_style = table_frame_border_style(
            ns_element_get_attr(el, "frame"));
        if (frame_style)
            g_string_append_printf(out, "border-style: %s;", frame_style);
        const char *bordercolor = ns_element_get_attr(el, "bordercolor");
        if (bordercolor && *bordercolor) {
            guint8 r, g, b, a;
            if (attr_is_color(bordercolor, &r, &g, &b, &a))
                g_string_append_printf(out, "border-color: rgba(%u,%u,%u,%g);",
                                       r, g, b, a / 255.0);
        }
        const char *cellspacing = ns_element_get_attr(el, "cellspacing");
        int spacing = ns_parse_int(cellspacing, -1, -1, G_MAXINT / 2);
        if (spacing >= 0)
            g_string_append_printf(out, "border-spacing: %dpx;", spacing);
    }
    const ns_node *part_table = is_table_part || strcmp(tag, "colgroup") == 0
        ? table_of_part(el) : NULL;
    int part_rules = part_table
        ? table_rules_kind(ns_element_get_attr(part_table, "rules")) : 0;
    if (is_cell) {
        const ns_node *tbl = el->parent;
        while (tbl && !(tbl->kind == NS_NODE_ELEMENT && tbl->name &&
                        g_ascii_strcasecmp(tbl->name, "table") == 0))
            tbl = tbl->parent;
        const char *cellpadding = tbl
            ? ns_element_get_attr(tbl, "cellpadding") : NULL;
        int padding = ns_parse_int(cellpadding, -1, -1, G_MAXINT / 2);
        if (padding >= 0)
            g_string_append_printf(out, "padding: %dpx;", padding);
        const char *tborder = part_table
            ? ns_element_get_attr(part_table, "border") : NULL;
        if (tborder && ns_parse_int(tborder, -1, -1, G_MAXINT / 2) != 0)
            g_string_append(out, "border-width: 1px; border-style: inset;");
        const char *tcolor = part_table
            ? ns_element_get_attr(part_table, "bordercolor") : NULL;
        guint8 r, g, b, a;
        if (tcolor && *tcolor && (tborder || part_rules) &&
            attr_is_color(tcolor, &r, &g, &b, &a))
            g_string_append_printf(out, "border-color: rgba(%u,%u,%u,%g);",
                                   r, g, b, a / 255.0);
        if (part_rules == TABLE_RULES_COLS)
            g_string_append(out, "border-width: 1px;"
                                 "border-block-style: none;"
                                 "border-inline-style: solid;");
        else if (part_rules == TABLE_RULES_ALL)
            g_string_append(out, "border-width: 1px; border-style: solid;");
        else if (part_rules == TABLE_RULES_ROWS)
            g_string_append(out, "border-width: 1px;"
                                 "border-block-style: solid;"
                                 "border-inline-style: none;");
        else if (part_rules)
            g_string_append(out, "border-width: 1px; border-style: none;");
        if (ns_element_get_attr(el, "nowrap") && !cell_nowrap_quirk(el))
            g_string_append(out, "white-space: nowrap;");
    } else if ((part_rules == TABLE_RULES_GROUPS &&
                strcmp(tag, "tr") != 0 && strcmp(tag, "colgroup") != 0) ||
               (part_rules == TABLE_RULES_ROWS && is_row)) {
        g_string_append(out, "border-block-width: 1px;"
                             "border-block-style: solid;");
    } else if (part_rules == TABLE_RULES_GROUPS &&
               strcmp(tag, "colgroup") == 0) {
        g_string_append(out, "border-inline-width: 1px;"
                             "border-inline-style: solid;");
    }
    if (is_table_part) {
        const char *align = ns_element_get_attr(el, "align");
        if (align && *align) {
            char *lo = g_ascii_strdown(align, -1);
            if (strcmp(lo, "middle") == 0 || strcmp(lo, "absmiddle") == 0)
                g_string_append(out, "text-align: center;");
            else if (strcmp(lo, "left") == 0 || strcmp(lo, "center") == 0 ||
                     strcmp(lo, "right") == 0 || strcmp(lo, "justify") == 0)
                g_string_append_printf(out, "text-align: %s;", lo);
            g_free(lo);
        }
        const char *valign = ns_element_get_attr(el, "valign");
        if (valign && *valign) {
            char *lo = g_ascii_strdown(valign, -1);
            if (strcmp(lo, "top") == 0 || strcmp(lo, "middle") == 0 ||
                strcmp(lo, "bottom") == 0 || strcmp(lo, "baseline") == 0) {
                const char *css = strcmp(lo, "middle") == 0 ? "middle" : lo;
                g_string_append_printf(out, "vertical-align: %s;", css);
            }
            g_free(lo);
        }
    }
    if (strcmp(tag, "p") == 0 ||
        strcmp(tag, "div") == 0 ||
        strcmp(tag, "h1") == 0 || strcmp(tag, "h2") == 0 ||
        strcmp(tag, "h3") == 0 || strcmp(tag, "h4") == 0 ||
        strcmp(tag, "h5") == 0 || strcmp(tag, "h6") == 0 ||
        is_table) {
        const char *align = ns_element_get_attr(el, "align");
        if (align && *align) {
            char *lo = g_ascii_strdown(align, -1);
            if (is_table && (strcmp(lo, "left") == 0 ||
                             strcmp(lo, "right") == 0))
                g_string_append_printf(out, "float: %s;", lo);
            else if (is_table && strcmp(lo, "center") == 0)
                g_string_append(out, "margin-left: auto; margin-right: auto;");
            else if (strcmp(lo, "left") == 0 || strcmp(lo, "center") == 0 ||
                     strcmp(lo, "right") == 0 || strcmp(lo, "justify") == 0)
                g_string_append_printf(out, "text-align: %s;", lo);
            g_free(lo);
        }
    }
    if (is_img) {
        const char *align = ns_element_get_attr(el, "align");
        if (align && *align) {
            char *lo = g_ascii_strdown(align, -1);
            if (strcmp(lo, "left") == 0 || strcmp(lo, "right") == 0)
                g_string_append_printf(out, "float: %s;", lo);
            else if (strcmp(lo, "top") == 0 || strcmp(lo, "bottom") == 0)
                g_string_append_printf(out, "vertical-align: %s;", lo);
            else if (strcmp(lo, "middle") == 0 ||
                     strcmp(lo, "center") == 0 ||
                     strcmp(lo, "absmiddle") == 0)
                g_string_append(out, "vertical-align: middle;");
            g_free(lo);
        }
    }
    if (is_img || is_image_input || strcmp(tag, "object") == 0) {
        const char *iborder = ns_element_get_attr(el, "border");
        int v = ns_parse_int(iborder, 0, 0, G_MAXINT / 2);
        if (v > 0)
            g_string_append_printf(out, "border: %dpx solid;", v);
    }
    if (strcmp(tag, "legend") == 0) {
        const char *align = ns_element_get_attr(el, "align");
        if (align && (g_ascii_strcasecmp(align, "left") == 0 ||
                      g_ascii_strcasecmp(align, "center") == 0 ||
                      g_ascii_strcasecmp(align, "right") == 0)) {
            char *lo = g_ascii_strdown(align, -1);
            g_string_append_printf(out, "justify-self: %s;", lo);
            g_free(lo);
        }
    }
    if (is_hr) {
        const char *align = ns_element_get_attr(el, "align");
        if (align && *align) {
            char *lo = g_ascii_strdown(align, -1);
            if (strcmp(lo, "center") == 0)
                g_string_append(out, "margin-left: auto; margin-right: auto;");
            else if (strcmp(lo, "left") == 0)
                g_string_append(out, "margin-left: 0; margin-right: auto;");
            else if (strcmp(lo, "right") == 0)
                g_string_append(out, "margin-left: auto; margin-right: 0;");
            g_free(lo);
        }
        const char *color = ns_element_get_attr(el, "color");
        if (color && *color) {
            guint8 r, g, b, a;
            if (attr_is_color(color, &r, &g, &b, &a))
                g_string_append_printf(out,
                    "color: rgba(%u,%u,%u,%g);"
                    "background-color: rgba(%u,%u,%u,%g);",
                    r, g, b, a / 255.0, r, g, b, a / 255.0);
        }
        const char *size = ns_element_get_attr(el, "size");
        if (size && *size) {
            int v = ns_parse_int(size, 0, 0, 1000);
            if (v > 0) g_string_append_printf(out, "height: %dpx;", v);
        }
        if (ns_element_get_attr(el, "noshade") && !(color && *color))
            g_string_append(out, "background-color: #808080;");
    }
    if (strcmp(tag, "textarea") == 0) {
        const char *wrap = ns_element_get_attr(el, "wrap");
        if (wrap && g_ascii_strcasecmp(wrap, "off") == 0)
            g_string_append(out, "white-space: pre;");
    }
    if (el->flags & NS_NODE_SVG_NS)
        append_svg_presentation_hints(out, el);

    if (out->len == 0) {
        g_string_free(out, TRUE);
        return NULL;
    }
    return g_string_free(out, FALSE);
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
    ns_attr_pred_clear(data);
    g_free(data);
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
    if (is_presentational_attr_name(name)) return TRUE;
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

        char *pres_css = presentational_hints_css(node);
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
rewrite_host_selectors(const char *css, const char *host_id)
{
    GString *out = g_string_new(NULL);
    char marker[96];
    g_snprintf(marker, sizeof marker, "[" NS_HOST_SCOPE_ATTR "=\"%s\"]", host_id);
    for (const char *p = css; *p; ) {
        if (p[0] == ':' && g_ascii_strncasecmp(p, "::slotted(", 10) == 0) {
            const char *inner = p + 10;
            const char *q = inner;
            int depth = 1;
            while (*q && depth) {
                if (*q == '(') depth++;
                else if (*q == ')') { depth--; if (!depth) break; }
                q++;
            }
            g_string_append(out, marker);
            g_string_append(out, " > ");
            g_string_append_len(out, inner, (gssize)(q - inner));
            p = (*q == ')') ? q + 1 : q;
            continue;
        }
        if (p[0] == ':' && g_ascii_strncasecmp(p, ":host", 5) == 0) {
            const char *after = p + 5;
            if (g_ascii_strncasecmp(after, "-context(", 9) == 0) {
                const char *q = after + 9;
                int depth = 1;
                while (*q && depth) {
                    if (*q == '(') depth++;
                    else if (*q == ')') depth--;
                    q++;
                }
                g_string_append(out, marker);
                p = q;
                continue;
            }
            if (*after == '(') {
                const char *inner = after + 1;
                const char *q = inner;
                int depth = 1;
                while (*q && depth) {
                    if (*q == '(') depth++;
                    else if (*q == ')') { depth--; if (!depth) break; }
                    q++;
                }
                g_string_append(out, marker);
                g_string_append_len(out, inner, (gssize)(q - inner));
                p = (*q == ')') ? q + 1 : q;
                continue;
            }
            if (!is_ident(*after) && *after != '-') {
                g_string_append(out, marker);
                p = after;
                continue;
            }
        }
        g_string_append_c(out, *p);
        p++;
    }
    return g_string_free(out, FALSE);
}

static gsize
selector_first_compound_len(const char *s)
{
    gsize i = 0;
    int depth = 0;
    while (s[i]) {
        char c = s[i];
        if (c == '(' || c == '[') depth++;
        else if (c == ')' || c == ']') { if (depth) depth--; }
        else if (!depth && (is_ws(c) || c == '>' || c == '+' || c == '~' ||
                            c == ','))
            break;
        i++;
    }
    return i;
}

static gboolean
selector_first_compound_targets_root(const char *s, gsize clen)
{
    if (clen >= 4 && g_ascii_strncasecmp(s, "html", 4) == 0 &&
        (clen == 4 || !is_ident(s[4])))
        return TRUE;
    if (clen >= 5 && g_ascii_strncasecmp(s, ":root", 5) == 0 &&
        (clen == 5 || !is_ident(s[5])))
        return TRUE;
    return FALSE;
}

static gsize
selector_compound_simple_len(const char *s, gsize clen)
{
    static const char *const legacy[] = {
        "before", "after", "first-line", "first-letter", NULL
    };
    int depth = 0;
    for (gsize i = 0; i < clen; i++) {
        char c = s[i];
        if (c == '(' || c == '[') depth++;
        else if (c == ')' || c == ']') { if (depth) depth--; }
        else if (!depth && c == ':') {
            if (i + 1 < clen && s[i + 1] == ':') return i;
            for (int k = 0; legacy[k]; k++) {
                gsize n = strlen(legacy[k]);
                if (i + 1 + n <= clen &&
                    g_ascii_strncasecmp(s + i + 1, legacy[k], n) == 0 &&
                    (i + 1 + n == clen || !is_ident(s[i + 1 + n])))
                    return i;
            }
        }
    }
    return clen;
}

static gboolean
selector_first_compound_may_be_root(const char *s, gsize clen)
{
    if (!clen) return FALSE;
    if (s[0] == '*') return TRUE;
    return s[0] == '.' || s[0] == '#' || s[0] == '[' || s[0] == ':';
}

static void
scope_one_selector(GString *out, const char *sel, gsize len,
                   const char *marker, const char *host_id,
                   gboolean frame_scope)
{
    while (len && is_ws(*sel)) { sel++; len--; }
    while (len && is_ws(sel[len - 1])) len--;
    if (!len) return;
    char *s = g_strndup(sel, len);
    gsize clen = selector_first_compound_len(s);
    gsize simple = selector_compound_simple_len(s, clen);
    if (strstr(s, ":host") || strstr(s, "::slotted")) {
        char *r = rewrite_host_selectors(s, host_id);
        g_string_append(out, r);
        g_free(r);
    } else if (selector_first_compound_targets_root(s, clen)) {
        g_string_append_len(out, s, (gssize)simple);
        g_string_append(out, marker);
        g_string_append(out, s + simple);
    } else {
        g_string_append(out, marker);
        g_string_append_c(out, ' ');
        g_string_append(out, s);
        if (frame_scope && selector_first_compound_may_be_root(s, clen)) {
            g_string_append(out, ", ");
            g_string_append_len(out, s, (gssize)simple);
            g_string_append(out, marker);
            g_string_append(out, s + simple);
        }
    }
    g_free(s);
}

static void
scope_rule_list(GString *out, const char *p, const char *end,
                const char *marker, const char *host_id, int depth,
                gboolean frame_scope)
{
    if (depth >= NS_CSS_MAX_AT_NESTING) {
        g_string_append_len(out, p, (gssize)(end - p));
        return;
    }
    while (p < end) {
        while (p < end && is_ws(*p)) p++;
        if (p >= end) break;
        if (p + 1 < end && p[0] == '/' && p[1] == '*') {
            p += 2;
            while (p + 1 < end && !(p[0] == '*' && p[1] == '/')) p++;
            if (p + 1 < end) p += 2;
            continue;
        }
        if (*p == '}') { p++; continue; }
        if (*p == '@') {
            const char *prelude = p;
            char term = 0;
            const char *seg = css_scan_segment(p, end, &term);
            if (term == '{') {
                gboolean group =
                    g_ascii_strncasecmp(prelude, "@media", 6) == 0 ||
                    g_ascii_strncasecmp(prelude, "@supports", 9) == 0 ||
                    g_ascii_strncasecmp(prelude, "@container", 10) == 0 ||
                    g_ascii_strncasecmp(prelude, "@layer", 6) == 0 ||
                    g_ascii_strncasecmp(prelude, "@scope", 6) == 0;
                const char *be = css_skip_to_block_end(seg, end);
                if (group) {
                    g_string_append_len(out, prelude, (gssize)(seg - prelude));
                    g_string_append_c(out, '{');
                    const char *body_s = seg + 1;
                    scope_rule_list(out, body_s, css_block_body_end(body_s, be),
                                    marker, host_id, depth + 1, frame_scope);
                    g_string_append_c(out, '}');
                } else {
                    g_string_append_len(out, prelude, (gssize)(be - prelude));
                }
                p = be;
            } else {
                g_string_append_len(out, prelude, (gssize)(seg - prelude));
                if (term == ';' && seg < end) { g_string_append_c(out, ';'); p = seg + 1; }
                else p = seg;
            }
            continue;
        }
        char term = 0;
        const char *seg = css_scan_segment(p, end, &term);
        if (term != '{') { p = (seg < end) ? seg + 1 : end; continue; }
        const char *be = css_skip_to_block_end(seg, end);
        const char *selend = seg;
        const char *q = p, *segstart = p;
        char quote = 0;
        int paren = 0, bracket = 0;
        gboolean first = TRUE;
        for (; q <= selend; q++) {
            if (q == selend || (!quote && !paren && !bracket && *q == ',')) {
                if (!first) g_string_append(out, ", ");
                first = FALSE;
                scope_one_selector(out, segstart, (gsize)(q - segstart),
                                   marker, host_id, frame_scope);
                segstart = q + 1;
                if (q == selend) break;
            } else if (quote) {
                if (*q == '\\' && q + 1 < selend) q++;
                else if (*q == quote) quote = 0;
            } else if (*q == '\\' && q + 1 < selend) q++;
            else if (*q == '"' || *q == '\'') quote = *q;
            else if (*q == '(') paren++;
            else if (*q == ')') { if (paren) paren--; }
            else if (*q == '[') bracket++;
            else if (*q == ']') { if (bracket) bracket--; }
        }
        g_string_append_c(out, '{');
        const char *body_s = seg + 1;
        const char *body_e = css_block_body_end(body_s, be);
        g_string_append_len(out, body_s, (gssize)(body_e - body_s));
        g_string_append_c(out, '}');
        p = be;
    }
}

static char *
scope_shadow_css(const char *flat_css, const char *host_id, gboolean frame_scope)
{
    GString *out = g_string_new(NULL);
    char marker[96];
    g_snprintf(marker, sizeof marker, "[" NS_HOST_SCOPE_ATTR "=\"%s\"]", host_id);
    scope_rule_list(out, flat_css, flat_css + strlen(flat_css), marker, host_id, 0,
                    frame_scope);
    return g_string_free(out, FALSE);
}

#define NS_SCOPED_CSS_CACHE_MAX 4096

static GHashTable *g_scoped_css_cache;

static char *
scoped_css_cached(const char *css, gsize len, const char *host_id,
                  gboolean frame_scope)
{
    GString *key = g_string_sized_new(len + strlen(host_id) + 3);
    g_string_append_c(key, frame_scope ? 'f' : 's');
    g_string_append(key, host_id);
    g_string_append_c(key, '\n');
    g_string_append_len(key, css, (gssize)len);
    const char *hit = g_scoped_css_cache
        ? g_hash_table_lookup(g_scoped_css_cache, key->str) : NULL;
    if (hit) {
        g_string_free(key, TRUE);
        return g_strdup(hit);
    }
    char *flat = css_flatten_nesting(css, (gssize)len);
    char *scoped = scope_shadow_css(flat, host_id, frame_scope);
    g_free(flat);
    if (!g_scoped_css_cache)
        g_scoped_css_cache = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                   g_free, g_free);
    if (g_hash_table_size(g_scoped_css_cache) >= NS_SCOPED_CSS_CACHE_MAX)
        g_hash_table_remove_all(g_scoped_css_cache);
    g_hash_table_insert(g_scoped_css_cache, g_string_free(key, FALSE),
                        g_strdup(scoped));
    return scoped;
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
        char *rewritten = scoped_css_cached(buf->str, buf->len, host_id,
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
    char *scoped = scoped_css_cached(css, strlen(css), host_id, FALSE);
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

    g_pragma_valid = FALSE;

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
    g_has_memo = g_hash_table_new_full(has_memo_hash, has_memo_equal,
                                       g_free, NULL);
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
        && g_css_focus_node == g_incr_prev_focus
        && g_css_hover_node == g_incr_prev_hover
        && g_css_active_node == g_incr_prev_active
        && g_css_fullscreen_node == g_incr_prev_fullscreen;
    g_incr_reused = 0;
    g_incr_recomputed = 0;

    incr_ensure_struct_keys(cached_ua, author_sheets, n_sheets, sig);

    memset(g_ancestor_filter, 0, sizeof g_ancestor_filter);
    g_ancestor_filter_active = TRUE;
    g_ancestor_filter_attrs = g_css_attr_ancestor_hashes;
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
        g_incr_prev_focus = g_css_focus_node;
        g_incr_prev_hover = g_css_hover_node;
        g_incr_prev_active = g_css_active_node;
        g_incr_prev_fullscreen = g_css_fullscreen_node;
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

    g_hash_table_destroy(g_has_memo);
    g_has_memo = NULL;
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
