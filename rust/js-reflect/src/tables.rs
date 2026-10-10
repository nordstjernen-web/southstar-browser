//! Southstar — reflection tables: the reflected attribute names, the enumerated keyword sets with their missing and invalid defaults, and the integer and boolean attributes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

pub(crate) const REFLECTED_NAMES: &[&CStr] = &[
    c"title",
    c"name",
    c"alt",
    c"src",
    c"href",
    c"type",
    c"placeholder",
    c"lang",
    c"dir",
    c"action",
    c"method",
    c"enctype",
    c"target",
    c"rel",
    c"accept",
    c"accept-charset",
    c"autocomplete",
    c"list",
    c"min",
    c"max",
    c"step",
    c"pattern",
    c"spellcheck",
    c"crossorigin",
    c"referrerpolicy",
    c"decoding",
    c"loading",
    c"fetchpriority",
    c"sizes",
    c"srcset",
    c"usemap",
    c"inputmode",
    c"size",
    c"cols",
    c"rows",
    c"maxlength",
    c"minlength",
    c"coords",
    c"shape",
    c"formaction",
    c"formmethod",
    c"formenctype",
    c"formtarget",
    c"integrity",
    c"kind",
    c"label",
    c"hreflang",
    c"charset",
    c"content",
    c"http-equiv",
    c"contenteditable",
    c"slot",
    c"is",
    c"role",
    c"aria-label",
    c"aria-hidden",
    c"aria-disabled",
    c"aria-pressed",
    c"aria-expanded",
    c"aria-controls",
    c"aria-describedby",
    c"aria-labelledby",
    c"aria-live",
    c"aria-busy",
    c"aria-checked",
    c"aria-current",
    c"aria-selected",
    c"aria-readonly",
    c"aria-required",
    c"aria-valuenow",
    c"aria-valuemin",
    c"aria-valuemax",
    c"nonce",
    c"accesskey",
    c"datetime",
    c"srcdoc",
    c"popovertarget",
    c"popovertargetaction",
    c"autocapitalize",
    c"enterkeyhint",
    c"low",
    c"high",
    c"optimum",
    c"poster",
    c"preload",
    c"wrap",
    c"scope",
    c"cite",
    c"media",
    c"download",
    c"ping",
    c"rev",
    c"as",
    c"align",
    c"valign",
    c"char",
    c"charoff",
    c"bgcolor",
    c"background",
    c"link",
    c"vlink",
    c"alink",
    c"color",
    c"clear",
    c"summary",
    c"frame",
    c"rules",
    c"border",
    c"cellpadding",
    c"cellspacing",
    c"axis",
    c"abbr",
    c"headers",
    c"scheme",
    c"standby",
    c"codetype",
    c"codebase",
    c"code",
    c"archive",
    c"scrolling",
    c"frameborder",
    c"marginwidth",
    c"marginheight",
    c"longdesc",
    c"lowsrc",
    c"version",
    c"event",
    c"valuetype",
    c"srclang",
    c"dirname",
    c"face",
    c"text",
];

pub(crate) const NAME: usize = 1;
pub(crate) const SRC: usize = 3;
pub(crate) const ACTION: usize = 9;
pub(crate) const FORM_ACTION: usize = 39;
pub(crate) const URL_ATTRS: &[usize] = &[SRC, ACTION, FORM_ACTION, 83, 87, 116, 123, 124];

pub(crate) const GLOBAL_ATTRS: &[&[u8]] = &[
    b"title",
    b"lang",
    b"dir",
    b"spellcheck",
    b"contenteditable",
    b"slot",
    b"is",
    b"role",
    b"nonce",
    b"accesskey",
    b"autocapitalize",
    b"enterkeyhint",
    b"inputmode",
    b"translate",
];

pub(crate) const NAME_REFLECTING_TAGS: &[&[u8]] = &[
    b"a",
    b"button",
    b"details",
    b"embed",
    b"fieldset",
    b"form",
    b"frame",
    b"iframe",
    b"img",
    b"input",
    b"map",
    b"meta",
    b"object",
    b"output",
    b"param",
    b"select",
    b"slot",
    b"textarea",
];

pub(crate) const NULL_IS_EMPTY: &[(&[u8], &[&[u8]])] = &[
    (b"body", &[b"text", b"link", b"vlink", b"alink", b"bgcolor"]),
    (b"font", &[b"color"]),
    (b"frame", &[b"marginheight", b"marginwidth"]),
    (b"iframe", &[b"marginheight", b"marginwidth"]),
    (b"img", &[b"border"]),
    (b"object", &[b"border"]),
    (b"table", &[b"bgcolor", b"cellpadding", b"cellspacing"]),
    (b"tr", &[b"bgcolor"]),
    (b"td", &[b"bgcolor"]),
    (b"th", &[b"bgcolor"]),
];

pub(crate) struct Keywords {
    pub attr: &'static CStr,
    pub keywords: &'static [&'static CStr],
    pub missing: Option<&'static CStr>,
    pub invalid: Option<&'static CStr>,
    pub nullable: bool,
}

const ENCTYPE: &[&CStr] = &[
    c"application/x-www-form-urlencoded",
    c"multipart/form-data",
    c"text/plain",
];
const METHOD: &[&CStr] = &[c"get", c"post", c"dialog"];
const SCOPE: &[&CStr] = &[c"row", c"col", c"rowgroup", c"colgroup"];
const INPUTMODE: &[&CStr] = &[
    c"none", c"text", c"tel", c"url", c"email", c"numeric", c"decimal", c"search",
];
const KIND: &[&CStr] = &[
    c"subtitles",
    c"captions",
    c"descriptions",
    c"chapters",
    c"metadata",
];
const AS: &[&CStr] = &[
    c"fetch",
    c"audio",
    c"document",
    c"embed",
    c"font",
    c"image",
    c"manifest",
    c"object",
    c"report",
    c"script",
    c"sharedworker",
    c"style",
    c"track",
    c"video",
    c"worker",
    c"xslt",
];
const PRELOAD: &[&CStr] = &[c"none", c"metadata", c"auto"];
const POPOVER_TARGET_ACTION: &[&CStr] = &[c"toggle", c"show", c"hide"];

const fn plain(
    attr: &'static CStr,
    keywords: &'static [&'static CStr],
    missing: &'static CStr,
    invalid: &'static CStr,
) -> Keywords {
    Keywords {
        attr,
        keywords,
        missing: Some(missing),
        invalid: Some(invalid),
        nullable: false,
    }
}

const fn nullable(
    attr: &'static CStr,
    keywords: &'static [&'static CStr],
    invalid: Option<&'static CStr>,
) -> Keywords {
    Keywords {
        attr,
        keywords,
        missing: None,
        invalid,
        nullable: true,
    }
}

pub(crate) const NORMALIZED: &[Keywords] = &[
    plain(c"enctype", ENCTYPE, ENCTYPE[0], ENCTYPE[0]),
    plain(c"formenctype", ENCTYPE, c"", ENCTYPE[0]),
    plain(c"formmethod", METHOD, c"", c"get"),
    plain(c"scope", SCOPE, c"", c""),
    plain(c"inputmode", INPUTMODE, c"", c""),
    plain(c"kind", KIND, c"subtitles", c"metadata"),
    plain(c"as", AS, c"", c""),
    plain(c"preload", PRELOAD, c"auto", c"auto"),
    plain(
        c"popovertargetaction",
        POPOVER_TARGET_ACTION,
        c"toggle",
        c"toggle",
    ),
];

const LOADING: &[&CStr] = &[c"lazy", c"eager"];
const DECODING: &[&CStr] = &[c"sync", c"async", c"auto"];
const CROSSORIGIN: &[&CStr] = &[c"anonymous", c"use-credentials"];
const REFERRER: &[&CStr] = &[
    c"no-referrer",
    c"no-referrer-when-downgrade",
    c"same-origin",
    c"origin",
    c"strict-origin",
    c"origin-when-cross-origin",
    c"strict-origin-when-cross-origin",
    c"unsafe-url",
];
const ENTERKEYHINT: &[&CStr] = &[
    c"enter",
    c"done",
    c"go",
    c"next",
    c"previous",
    c"search",
    c"send",
];
const TRUE_FALSE: &[&CStr] = &[c"true", c"false"];
const TRISTATE: &[&CStr] = &[c"true", c"false", c"mixed"];
const ARIA_AUTOCOMPLETE: &[&CStr] = &[c"inline", c"list", c"both", c"none"];
const ARIA_CURRENT: &[&CStr] = &[
    c"page",
    c"step",
    c"location",
    c"date",
    c"time",
    c"true",
    c"false",
];
const ARIA_HASPOPUP: &[&CStr] = &[
    c"true", c"false", c"menu", c"dialog", c"listbox", c"tree", c"grid",
];
const ARIA_INVALID: &[&CStr] = &[c"true", c"false", c"spelling", c"grammar"];
const ARIA_LIVE: &[&CStr] = &[c"polite", c"assertive", c"off"];
const ARIA_ORIENTATION: &[&CStr] = &[c"horizontal", c"vertical"];
const ARIA_SORT: &[&CStr] = &[c"ascending", c"descending", c"other", c"none"];

pub(crate) const ENUMERATED: &[Keywords] = &[
    plain(c"loading", LOADING, c"eager", c"eager"),
    plain(c"decoding", DECODING, c"auto", c"auto"),
    plain(c"method", METHOD, c"get", c"get"),
    nullable(c"crossorigin", CROSSORIGIN, Some(c"anonymous")),
    plain(c"referrerpolicy", REFERRER, c"", c""),
    plain(c"enterkeyhint", ENTERKEYHINT, c"", c""),
    nullable(c"aria-atomic", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-autocomplete", ARIA_AUTOCOMPLETE, Some(c"none")),
    nullable(c"aria-busy", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-checked", TRISTATE, None),
    nullable(c"aria-current", ARIA_CURRENT, Some(c"true")),
    nullable(c"aria-disabled", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-expanded", TRUE_FALSE, None),
    nullable(c"aria-haspopup", ARIA_HASPOPUP, Some(c"false")),
    nullable(c"aria-hidden", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-invalid", ARIA_INVALID, Some(c"true")),
    nullable(c"aria-live", ARIA_LIVE, Some(c"off")),
    nullable(c"aria-modal", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-multiline", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-multiselectable", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-orientation", ARIA_ORIENTATION, None),
    nullable(c"aria-pressed", TRISTATE, None),
    nullable(c"aria-readonly", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-required", TRUE_FALSE, Some(c"false")),
    nullable(c"aria-selected", TRUE_FALSE, None),
    nullable(c"aria-sort", ARIA_SORT, Some(c"none")),
];

const INPUT_TYPES: &[&CStr] = &[
    c"text",
    c"search",
    c"tel",
    c"url",
    c"email",
    c"password",
    c"date",
    c"month",
    c"week",
    c"time",
    c"datetime-local",
    c"number",
    c"range",
    c"color",
    c"checkbox",
    c"radio",
    c"file",
    c"submit",
    c"image",
    c"reset",
    c"button",
    c"hidden",
];
const BUTTON_TYPES: &[&CStr] = &[c"submit", c"reset", c"button"];

pub(crate) const INPUT_TYPE: Keywords = plain(c"type", INPUT_TYPES, c"text", c"text");
pub(crate) const BUTTON_TYPE: Keywords = plain(c"type", BUTTON_TYPES, c"button", c"button");

pub(crate) const ARIA_STRINGS: &[&CStr] = &[
    c"role",
    c"aria-label",
    c"aria-braillelabel",
    c"aria-brailleroledescription",
    c"aria-colcount",
    c"aria-colindex",
    c"aria-colindextext",
    c"aria-colspan",
    c"aria-description",
    c"aria-keyshortcuts",
    c"aria-level",
    c"aria-placeholder",
    c"aria-posinset",
    c"aria-relevant",
    c"aria-roledescription",
    c"aria-rowcount",
    c"aria-rowindex",
    c"aria-rowindextext",
    c"aria-rowspan",
    c"aria-setsize",
    c"aria-valuemax",
    c"aria-valuemin",
    c"aria-valuenow",
    c"aria-valuetext",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntKind {
    Long,
    LimitedLong,
    ULong,
    LimitedULong,
    Clamped,
}

pub(crate) struct IntAttr {
    pub attr: &'static CStr,
    pub default: i32,
    pub lo: i32,
    pub hi: i32,
    pub kind: IntKind,
}

const fn int(attr: &'static CStr, default: i32, lo: i32, hi: i32, kind: IntKind) -> IntAttr {
    IntAttr {
        attr,
        default,
        lo,
        hi,
        kind,
    }
}

pub(crate) const INT_ATTRS: &[IntAttr] = &[
    int(c"maxlength", -1, i32::MIN, i32::MAX, IntKind::LimitedLong),
    int(c"minlength", -1, i32::MIN, i32::MAX, IntKind::LimitedLong),
    int(c"size", 0, i32::MIN, i32::MAX, IntKind::LimitedULong),
    int(c"cols", 20, i32::MIN, i32::MAX, IntKind::LimitedULong),
    int(c"rows", 2, i32::MIN, i32::MAX, IntKind::LimitedULong),
    int(c"span", 1, 1, 1000, IntKind::Clamped),
    int(c"colspan", 1, 1, 1000, IntKind::Clamped),
    int(c"rowspan", 1, 0, 65534, IntKind::Clamped),
    int(c"width", 0, i32::MIN, i32::MAX, IntKind::ULong),
    int(c"height", 0, i32::MIN, i32::MAX, IntKind::ULong),
    int(c"start", 1, i32::MIN, i32::MAX, IntKind::Long),
    int(c"hspace", 0, 0, i32::MAX, IntKind::ULong),
    int(c"vspace", 0, 0, i32::MAX, IntKind::ULong),
    int(c"scrollamount", 6, 0, i32::MAX, IntKind::ULong),
    int(c"scrolldelay", 85, 0, i32::MAX, IntKind::ULong),
];

pub(crate) const PLAIN_BOOLEANS: &[&CStr] = &[
    c"allowfullscreen",
    c"declare",
    c"muted",
    c"default",
    c"nohref",
    c"noshade",
    c"compact",
    c"nowrap",
    c"truespeed",
    c"noresize",
];

pub(crate) const BOOLEANS: &[&CStr] = &[
    c"open",
    c"selected",
    c"multiple",
    c"readonly",
    c"autofocus",
    c"controls",
    c"loop",
    c"muted",
    c"autoplay",
    c"defer",
    c"async",
    c"novalidate",
    c"ismap",
    c"draggable",
    c"reversed",
    c"playsinline",
    c"default",
    c"inert",
    c"nomodule",
    c"formnovalidate",
    c"required",
];

pub(crate) const FOCUSABLE_TAGS: &[&[u8]] = &[
    b"a",
    b"area",
    b"button",
    b"frame",
    b"iframe",
    b"input",
    b"object",
    b"select",
    b"textarea",
];

pub(crate) const STRING_DIMENSION_TAGS: &[&[u8]] = &[
    b"iframe",
    b"embed",
    b"object",
    b"marquee",
    b"table",
    b"colgroup",
    b"col",
    b"td",
    b"th",
    b"hr",
];

pub(crate) const AUTOCAPITALIZE_INHERITING: &[&[u8]] = &[
    b"button",
    b"fieldset",
    b"input",
    b"output",
    b"select",
    b"textarea",
];

pub(crate) const AUTOFILL_FIELDS: &[&[u8]] = &[
    b"name",
    b"honorific-prefix",
    b"given-name",
    b"additional-name",
    b"family-name",
    b"honorific-suffix",
    b"nickname",
    b"username",
    b"new-password",
    b"current-password",
    b"one-time-code",
    b"organization-title",
    b"organization",
    b"street-address",
    b"address-line1",
    b"address-line2",
    b"address-line3",
    b"address-level4",
    b"address-level3",
    b"address-level2",
    b"address-level1",
    b"country",
    b"country-name",
    b"postal-code",
    b"cc-name",
    b"cc-given-name",
    b"cc-additional-name",
    b"cc-family-name",
    b"cc-number",
    b"cc-exp",
    b"cc-exp-month",
    b"cc-exp-year",
    b"cc-csc",
    b"cc-type",
    b"transaction-currency",
    b"transaction-amount",
    b"language",
    b"bday",
    b"bday-day",
    b"bday-month",
    b"bday-year",
    b"sex",
    b"url",
    b"photo",
    b"tel",
    b"tel-country-code",
    b"tel-national",
    b"tel-area-code",
    b"tel-local",
    b"tel-local-prefix",
    b"tel-local-suffix",
    b"tel-extension",
    b"email",
    b"impp",
    b"webauthn",
];

pub(crate) const AUTOFILL_CONTACTS: &[&[u8]] = &[b"home", b"work", b"mobile", b"fax", b"pager"];
