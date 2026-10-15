//! Southstar — the compact locale data Intl.NumberFormat formats with: symbols, patterns, compact names, currencies, units and digits.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[derive(Clone, Copy)]
pub(crate) struct Affix {
    pub before: bool,
    pub space: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct CompactStep {
    pub from: i64,
    pub exponent: i64,
    pub one: &'static str,
    pub other: &'static str,
}

pub(crate) struct LocaleData {
    pub decimal: &'static str,
    pub group: &'static str,
    pub minus: &'static str,
    pub plus: &'static str,
    pub percent_sign: &'static str,
    pub nan: &'static str,
    pub percent: Affix,
    pub currency: Affix,
    pub accounting_parentheses: bool,
    pub secondary_group: usize,
    pub minimum_grouping: usize,
    pub range_separator: &'static str,
    pub approximately: &'static str,
    pub compact_short: &'static [CompactStep],
    pub compact_long: &'static [CompactStep],
    pub default_numbering: &'static str,
}

const NBSP: &str = "\u{a0}";
const NNBSP: &str = "\u{202f}";

const fn step(from: i64, exponent: i64, one: &'static str, other: &'static str) -> CompactStep {
    CompactStep {
        from,
        exponent,
        one,
        other,
    }
}

const EN_SHORT: [CompactStep; 4] = [
    step(3, 3, "K", "K"),
    step(6, 6, "M", "M"),
    step(9, 9, "B", "B"),
    step(12, 12, "T", "T"),
];
const EN_LONG: [CompactStep; 4] = [
    step(3, 3, " thousand", " thousand"),
    step(6, 6, " million", " million"),
    step(9, 9, " billion", " billion"),
    step(12, 12, " trillion", " trillion"),
];
const EN_IN_SHORT: [CompactStep; 3] = [
    step(3, 3, "K", "K"),
    step(5, 5, "L", "L"),
    step(7, 7, "Cr", "Cr"),
];
const EN_IN_LONG: [CompactStep; 3] = [
    step(3, 3, " thousand", " thousand"),
    step(5, 5, " lakh", " lakh"),
    step(7, 7, " crore", " crore"),
];
const DE_SHORT: [CompactStep; 4] = [
    step(3, 0, "", ""),
    step(6, 6, "\u{a0}Mio.", "\u{a0}Mio."),
    step(9, 9, "\u{a0}Mrd.", "\u{a0}Mrd."),
    step(12, 12, "\u{a0}Bio.", "\u{a0}Bio."),
];
const DE_LONG: [CompactStep; 4] = [
    step(3, 3, " Tausend", " Tausend"),
    step(6, 6, " Million", " Millionen"),
    step(9, 9, " Milliarde", " Milliarden"),
    step(12, 12, " Billion", " Billionen"),
];
const FR_SHORT: [CompactStep; 4] = [
    step(3, 3, "\u{a0}k", "\u{a0}k"),
    step(6, 6, "\u{a0}M", "\u{a0}M"),
    step(9, 9, "\u{a0}Md", "\u{a0}Md"),
    step(12, 12, "\u{a0}Bn", "\u{a0}Bn"),
];
const FR_LONG: [CompactStep; 4] = [
    step(3, 3, " mille", " mille"),
    step(6, 6, " million", " millions"),
    step(9, 9, " milliard", " milliards"),
    step(12, 12, " billion", " billions"),
];
const ES_SHORT: [CompactStep; 4] = [
    step(3, 3, "\u{a0}mil", "\u{a0}mil"),
    step(6, 6, "\u{a0}M", "\u{a0}M"),
    step(9, 9, "\u{a0}mil\u{a0}M", "\u{a0}mil\u{a0}M"),
    step(12, 12, "\u{a0}B", "\u{a0}B"),
];
const ES_LONG: [CompactStep; 4] = [
    step(3, 3, " mil", " mil"),
    step(6, 6, " millón", " millones"),
    step(9, 9, " mil millones", " mil millones"),
    step(12, 12, " billón", " billones"),
];
const JA_COMPACT: [CompactStep; 4] = [
    step(4, 4, "万", "万"),
    step(8, 8, "億", "億"),
    step(12, 12, "兆", "兆"),
    step(16, 16, "京", "京"),
];
const KO_COMPACT: [CompactStep; 5] = [
    step(3, 3, "천", "천"),
    step(4, 4, "만", "만"),
    step(8, 8, "억", "억"),
    step(12, 12, "조", "조"),
    step(16, 16, "경", "경"),
];
const ZH_COMPACT: [CompactStep; 3] = [
    step(4, 4, "万", "万"),
    step(8, 8, "亿", "亿"),
    step(12, 12, "万亿", "万亿"),
];
const ZH_HANT_COMPACT: [CompactStep; 3] = [
    step(4, 4, "萬", "萬"),
    step(8, 8, "億", "億"),
    step(12, 12, "兆", "兆"),
];

const PREFIX: Affix = Affix {
    before: true,
    space: "",
};
const PREFIX_SPACED: Affix = Affix {
    before: true,
    space: NBSP,
};
const SUFFIX: Affix = Affix {
    before: false,
    space: "",
};
const SUFFIX_SPACED: Affix = Affix {
    before: false,
    space: NBSP,
};

const EN: LocaleData = LocaleData {
    decimal: ".",
    group: ",",
    minus: "-",
    plus: "+",
    percent_sign: "%",
    nan: "NaN",
    percent: SUFFIX,
    currency: PREFIX,
    accounting_parentheses: true,
    secondary_group: 3,
    minimum_grouping: 1,
    range_separator: "\u{2013}",
    approximately: "~",
    compact_short: &EN_SHORT,
    compact_long: &EN_LONG,
    default_numbering: "latn",
};
const EN_IN: LocaleData = LocaleData {
    secondary_group: 2,
    compact_short: &EN_IN_SHORT,
    compact_long: &EN_IN_LONG,
    ..EN
};
const COMMA: LocaleData = LocaleData {
    decimal: ",",
    group: ".",
    percent: SUFFIX_SPACED,
    currency: SUFFIX_SPACED,
    accounting_parentheses: false,
    ..EN
};
const SPACE_GROUPED: LocaleData = LocaleData {
    group: NBSP,
    ..COMMA
};
const NORDIC: LocaleData = LocaleData {
    minus: "\u{2212}",
    ..SPACE_GROUPED
};
const DE: LocaleData = LocaleData {
    compact_short: &DE_SHORT,
    compact_long: &DE_LONG,
    ..COMMA
};
const FR: LocaleData = LocaleData {
    group: NNBSP,
    percent: Affix {
        before: false,
        space: NNBSP,
    },
    compact_short: &FR_SHORT,
    compact_long: &FR_LONG,
    ..COMMA
};
const ES: LocaleData = LocaleData {
    minimum_grouping: 2,
    range_separator: "-",
    compact_short: &ES_SHORT,
    compact_long: &ES_LONG,
    ..COMMA
};
const IT: LocaleData = LocaleData {
    percent: SUFFIX,
    ..COMMA
};
const NL: LocaleData = LocaleData {
    percent: SUFFIX,
    currency: PREFIX_SPACED,
    accounting_parentheses: true,
    ..COMMA
};
const PT: LocaleData = LocaleData {
    percent: SUFFIX,
    currency: PREFIX_SPACED,
    ..COMMA
};
const PT_PT: LocaleData = LocaleData {
    group: NBSP,
    percent: SUFFIX,
    minimum_grouping: 2,
    range_separator: " - ",
    ..COMMA
};
const PL: LocaleData = LocaleData {
    percent: SUFFIX,
    minimum_grouping: 2,
    ..SPACE_GROUPED
};
const TR: LocaleData = LocaleData {
    percent: PREFIX,
    currency: PREFIX,
    ..COMMA
};
const RU: LocaleData = LocaleData {
    nan: "не\u{a0}число",
    ..SPACE_GROUPED
};
const FI: LocaleData = LocaleData {
    nan: "epäluku",
    ..NORDIC
};
const JA: LocaleData = LocaleData {
    range_separator: "\u{ff5e}",
    compact_short: &JA_COMPACT,
    compact_long: &JA_COMPACT,
    ..EN
};
const KO: LocaleData = LocaleData {
    range_separator: "~",
    compact_short: &KO_COMPACT,
    compact_long: &KO_COMPACT,
    ..EN
};
const ZH: LocaleData = LocaleData {
    range_separator: "-",
    compact_short: &ZH_COMPACT,
    compact_long: &ZH_COMPACT,
    ..EN
};
const ZH_HANT: LocaleData = LocaleData {
    nan: "非數值",
    compact_short: &ZH_HANT_COMPACT,
    compact_long: &ZH_HANT_COMPACT,
    ..ZH
};
const HI: LocaleData = LocaleData {
    secondary_group: 2,
    ..EN
};
const AR: LocaleData = LocaleData {
    minus: "\u{200e}-",
    plus: "\u{200e}+",
    nan: "ليس\u{a0}رقمًا",
    currency: SUFFIX_SPACED,
    accounting_parentheses: false,
    default_numbering: "arab",
    ..EN
};
const AR_ARAB: LocaleData = LocaleData {
    decimal: "\u{66b}",
    group: "\u{66c}",
    minus: "\u{61c}-",
    plus: "\u{61c}+",
    percent_sign: "\u{66a}\u{61c}",
    ..AR
};
const FA: LocaleData = LocaleData {
    minus: "\u{200e}\u{2212}",
    plus: "\u{200e}+",
    ..EN
};

const LOCALES: [(&str, &LocaleData); 53] = [
    ("en", &EN),
    ("en-IN", &EN_IN),
    ("de", &DE),
    ("fr", &FR),
    ("es", &ES),
    ("it", &IT),
    ("nl", &NL),
    ("pt", &PT),
    ("pt-PT", &PT_PT),
    ("pl", &PL),
    ("tr", &TR),
    ("ru", &RU),
    ("uk", &SPACE_GROUPED),
    ("be", &SPACE_GROUPED),
    ("cs", &SPACE_GROUPED),
    ("sk", &SPACE_GROUPED),
    ("hu", &SPACE_GROUPED),
    ("bg", &SPACE_GROUPED),
    ("lv", &SPACE_GROUPED),
    ("af", &SPACE_GROUPED),
    ("sv", &NORDIC),
    ("nb", &NORDIC),
    ("nn", &NORDIC),
    ("no", &NORDIC),
    ("fo", &NORDIC),
    ("et", &NORDIC),
    ("lt", &NORDIC),
    ("sl", &NORDIC),
    ("fi", &FI),
    ("da", &COMMA),
    ("el", &COMMA),
    ("ro", &COMMA),
    ("id", &COMMA),
    ("vi", &COMMA),
    ("ca", &COMMA),
    ("hr", &COMMA),
    ("is", &COMMA),
    ("sr", &COMMA),
    ("gl", &COMMA),
    ("eu", &COMMA),
    ("ja", &JA),
    ("ko", &KO),
    ("zh", &ZH),
    ("zh-Hant", &ZH_HANT),
    ("hi", &HI),
    ("ar", &AR),
    ("ar-arab", &AR_ARAB),
    ("fa", &FA),
    ("he", &EN),
    ("th", &EN),
    ("ms", &EN),
    ("sw", &EN),
    ("ga", &EN),
];

pub(crate) fn is_available(tag: &str) -> bool {
    let mut subtags = tag.split('-');
    let Some(language) = subtags.next() else {
        return false;
    };
    let mut rest: Vec<&str> = subtags.collect();
    if rest.first().is_some_and(|s| s.len() == 4) {
        rest.remove(0);
    }
    if rest.first().is_some_and(|s| s.len() == 2 || s.len() == 3) {
        rest.remove(0);
    }
    rest.is_empty()
        && LOCALES
            .iter()
            .any(|(key, _)| !key.contains('-') && *key == language)
}

fn language_parts(tag: &str) -> (&str, Option<&str>, Option<&str>) {
    let mut subtags = tag.split('-');
    let language = subtags.next().unwrap_or("en");
    let mut script = None;
    let mut region = None;
    for subtag in subtags {
        if subtag.len() == 4 && script.is_none() && region.is_none() {
            script = Some(subtag);
        } else if (subtag.len() == 2 || subtag.len() == 3) && region.is_none() {
            region = Some(subtag);
        }
    }
    (language, script, region)
}

fn lookup(key: &str) -> Option<&'static LocaleData> {
    LOCALES
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, data)| *data)
}

pub(crate) fn uses_traditional_chinese(tag: &str) -> bool {
    let (language, script, region) = language_parts(tag);
    language == "zh"
        && (script == Some("Hant")
            || (script.is_none() && matches!(region, Some("TW" | "HK" | "MO"))))
}

pub(crate) fn locale_data(tag: &str, numbering: &str) -> &'static LocaleData {
    let (language, script, region) = language_parts(tag);
    if language == "ar" && numbering == "arab" {
        return &AR_ARAB;
    }
    if uses_traditional_chinese(tag) {
        return &ZH_HANT;
    }
    if let Some(script) = script
        && let Some(data) = lookup(&format!("{language}-{script}"))
    {
        return data;
    }
    if let Some(region) = region
        && let Some(data) = lookup(&format!("{language}-{region}"))
    {
        return data;
    }
    lookup(language).unwrap_or(&EN)
}

pub(crate) fn default_numbering_system(tag: &str) -> &'static str {
    let (language, _, _) = language_parts(tag);
    lookup(language).map_or("latn", |data| data.default_numbering)
}

const NUMBERING_ZEROS: [(&str, u32); 78] = [
    ("adlm", 0x1E950),
    ("ahom", 0x11730),
    ("arab", 0x660),
    ("arabext", 0x6F0),
    ("bali", 0x1B50),
    ("beng", 0x9E6),
    ("bhks", 0x11C50),
    ("brah", 0x11066),
    ("cakm", 0x11136),
    ("cham", 0xAA50),
    ("deva", 0x966),
    ("diak", 0x11950),
    ("fullwide", 0xFF10),
    ("gara", 0x10D40),
    ("gong", 0x11DA0),
    ("gonm", 0x11D50),
    ("gujr", 0xAE6),
    ("gukh", 0x16130),
    ("guru", 0xA66),
    ("hanidec", 0),
    ("hmng", 0x16B50),
    ("hmnp", 0x1E140),
    ("java", 0xA9D0),
    ("kali", 0xA900),
    ("kawi", 0x11F50),
    ("khmr", 0x17E0),
    ("knda", 0xCE6),
    ("krai", 0x16D70),
    ("lana", 0x1A80),
    ("lanatham", 0x1A90),
    ("laoo", 0xED0),
    ("latn", 0x30),
    ("lepc", 0x1C40),
    ("limb", 0x1946),
    ("mathbold", 0x1D7CE),
    ("mathdbl", 0x1D7D8),
    ("mathmono", 0x1D7F6),
    ("mathsanb", 0x1D7EC),
    ("mathsans", 0x1D7E2),
    ("mlym", 0xD66),
    ("modi", 0x11650),
    ("mong", 0x1810),
    ("mroo", 0x16A60),
    ("mtei", 0xABF0),
    ("mymr", 0x1040),
    ("mymrepka", 0x116DA),
    ("mymrpao", 0x116D0),
    ("mymrshan", 0x1090),
    ("mymrtlng", 0xA9F0),
    ("nagm", 0x1E4F0),
    ("newa", 0x11450),
    ("nkoo", 0x7C0),
    ("olck", 0x1C50),
    ("onao", 0x1E5F1),
    ("orya", 0xB66),
    ("osma", 0x104A0),
    ("outlined", 0x1CCF0),
    ("rohg", 0x10D30),
    ("saur", 0xA8D0),
    ("segment", 0x1FBF0),
    ("shrd", 0x111D0),
    ("sind", 0x112F0),
    ("sinh", 0xDE6),
    ("sora", 0x110F0),
    ("sund", 0x1BB0),
    ("sunu", 0x11BF0),
    ("takr", 0x116C0),
    ("talu", 0x19D0),
    ("tamldec", 0xBE6),
    ("telu", 0xC66),
    ("thai", 0xE50),
    ("tibt", 0xF20),
    ("tirh", 0x114D0),
    ("tnsa", 0x16AC0),
    ("tols", 0x11DE0),
    ("vaii", 0xA620),
    ("wara", 0x118E0),
    ("wcho", 0x1E2F0),
];

const HANIDEC: [char; 10] = [
    '\u{3007}', '\u{4E00}', '\u{4E8C}', '\u{4E09}', '\u{56DB}', '\u{4E94}', '\u{516D}', '\u{4E03}',
    '\u{516B}', '\u{4E5D}',
];

pub(crate) fn is_numbering_system(name: &str) -> bool {
    NUMBERING_ZEROS.iter().any(|(known, _)| *known == name)
}

pub(crate) fn numbering_systems() -> impl Iterator<Item = &'static str> {
    NUMBERING_ZEROS.iter().map(|(name, _)| *name)
}

pub(crate) fn transliterate(ascii_digits: &str, numbering: &str) -> String {
    if numbering == "latn" {
        return ascii_digits.to_owned();
    }
    let zero = NUMBERING_ZEROS
        .iter()
        .find(|(known, _)| *known == numbering)
        .map_or(0x30, |(_, zero)| *zero);
    ascii_digits
        .chars()
        .map(|c| match c.to_digit(10) {
            Some(digit) if numbering == "hanidec" => HANIDEC[digit as usize],
            Some(digit) => char::from_u32(zero + digit).unwrap_or(c),
            None => c,
        })
        .collect()
}

const CURRENCY_DIGITS: [(&str, u32); 27] = [
    ("BHD", 3),
    ("BIF", 0),
    ("CLF", 4),
    ("CLP", 0),
    ("DJF", 0),
    ("GNF", 0),
    ("IQD", 3),
    ("ISK", 0),
    ("JOD", 3),
    ("JPY", 0),
    ("KMF", 0),
    ("KRW", 0),
    ("KWD", 3),
    ("LYD", 3),
    ("OMR", 3),
    ("PYG", 0),
    ("RWF", 0),
    ("TND", 3),
    ("UGX", 0),
    ("UYI", 0),
    ("UYW", 4),
    ("VND", 0),
    ("VUV", 0),
    ("XAF", 0),
    ("XOF", 0),
    ("XPF", 0),
    ("XXX", 0),
];

pub(crate) fn currency_digits(code: &str) -> u32 {
    CURRENCY_DIGITS
        .iter()
        .find(|(known, _)| *known == code)
        .map_or(2, |(_, digits)| *digits)
}

const SYMBOLS: [(&str, &str, &str); 22] = [
    ("USD", "$", "$"),
    ("EUR", "\u{20ac}", "\u{20ac}"),
    ("GBP", "\u{a3}", "\u{a3}"),
    ("JPY", "\u{a5}", "\u{a5}"),
    ("CNY", "CN\u{a5}", "\u{a5}"),
    ("INR", "\u{20b9}", "\u{20b9}"),
    ("KRW", "\u{20a9}", "\u{20a9}"),
    ("CAD", "CA$", "$"),
    ("AUD", "A$", "$"),
    ("NZD", "NZ$", "$"),
    ("HKD", "HK$", "$"),
    ("MXN", "MX$", "$"),
    ("BRL", "R$", "R$"),
    ("ILS", "\u{20aa}", "\u{20aa}"),
    ("VND", "\u{20ab}", "\u{20ab}"),
    ("TWD", "NT$", "$"),
    ("PHP", "\u{20b1}", "\u{20b1}"),
    ("XAF", "FCFA", "FCFA"),
    ("XOF", "F\u{202f}CFA", "F\u{202f}CFA"),
    ("RUB", "RUB", "\u{20bd}"),
    ("SEK", "SEK", "kr"),
    ("NOK", "NOK", "kr"),
];

const LOCAL_SYMBOLS: [(&str, &str, &str); 22] = [
    ("ko", "USD", "US$"),
    ("zh", "USD", "US$"),
    ("pt", "USD", "US$"),
    ("es", "USD", "US$"),
    ("fr", "USD", "$US"),
    ("ja", "JPY", "\u{ffe5}"),
    ("ja", "CNY", "\u{5143}"),
    ("zh", "CNY", "\u{a5}"),
    ("zh", "JPY", "JP\u{a5}"),
    ("sv", "SEK", "kr"),
    ("nb", "NOK", "kr"),
    ("nn", "NOK", "kr"),
    ("no", "NOK", "kr"),
    ("da", "DKK", "kr."),
    ("ru", "RUB", "\u{20bd}"),
    ("pl", "PLN", "z\u{142}"),
    ("cs", "CZK", "K\u{10d}"),
    ("hu", "HUF", "Ft"),
    ("tr", "TRY", "\u{20ba}"),
    ("uk", "UAH", "\u{20b4}"),
    ("ko", "KRW", "\u{20a9}"),
    ("hi", "INR", "\u{20b9}"),
];

pub(crate) fn currency_symbol(tag: &str, code: &str, narrow: bool) -> String {
    let (language, _, region) = language_parts(tag);
    if language == "zh" && code == "TWD" && uses_traditional_chinese(tag) {
        return "$".to_owned();
    }
    if language == "en" && code == "USD" && matches!(region, Some("CA" | "AU" | "NZ" | "SG")) {
        return if narrow { "$" } else { "US$" }.to_owned();
    }
    let global = SYMBOLS.iter().find(|(known, _, _)| *known == code);
    if narrow && let Some((_, _, narrow)) = global {
        return (*narrow).to_owned();
    }
    if let Some((_, _, symbol)) = LOCAL_SYMBOLS
        .iter()
        .find(|(lang, known, _)| *lang == language && *known == code)
    {
        return (*symbol).to_owned();
    }
    global.map_or_else(|| code.to_owned(), |(_, symbol, _)| (*symbol).to_owned())
}

const CURRENCY_NAMES: [(&str, &str, &str); 14] = [
    ("USD", "US dollar", "US dollars"),
    ("EUR", "euro", "euros"),
    ("GBP", "British pound", "British pounds"),
    ("JPY", "Japanese yen", "Japanese yen"),
    ("CNY", "Chinese yuan", "Chinese yuan"),
    ("INR", "Indian rupee", "Indian rupees"),
    ("KRW", "South Korean won", "South Korean won"),
    ("CAD", "Canadian dollar", "Canadian dollars"),
    ("AUD", "Australian dollar", "Australian dollars"),
    ("CHF", "Swiss franc", "Swiss francs"),
    ("SEK", "Swedish krona", "Swedish kronor"),
    ("NOK", "Norwegian krone", "Norwegian kroner"),
    ("DKK", "Danish krone", "Danish kroner"),
    ("MXN", "Mexican peso", "Mexican pesos"),
];

pub(crate) fn currency_name(tag: &str, code: &str, plural_one: bool) -> String {
    let (language, _, _) = language_parts(tag);
    if language != "en" {
        return code.to_owned();
    }
    CURRENCY_NAMES
        .iter()
        .find(|(known, _, _)| *known == code)
        .map_or_else(
            || code.to_owned(),
            |(_, one, other)| if plural_one { *one } else { *other }.to_owned(),
        )
}

pub(crate) struct UnitNames {
    pub short: &'static str,
    pub short_plural: &'static str,
    pub narrow: &'static str,
    pub long: &'static str,
    pub long_plural: &'static str,
    pub per: &'static str,
}

const fn unit(
    short: &'static str,
    short_plural: &'static str,
    narrow: &'static str,
    long: &'static str,
    long_plural: &'static str,
    per: &'static str,
) -> UnitNames {
    UnitNames {
        short,
        short_plural,
        narrow,
        long,
        long_plural,
        per,
    }
}

const SIMPLE_UNITS: [(&str, UnitNames); 45] = [
    ("acre", unit(" ac", " ac", "ac", " acre", " acres", "ac")),
    ("bit", unit(" bit", " bit", "bit", " bit", " bits", "bit")),
    ("byte", unit(" byte", " byte", "B", " byte", " bytes", "B")),
    (
        "celsius",
        unit(
            "\u{b0}C",
            "\u{b0}C",
            "\u{b0}C",
            " degree Celsius",
            " degrees Celsius",
            "\u{b0}C",
        ),
    ),
    (
        "centimeter",
        unit(" cm", " cm", "cm", " centimeter", " centimeters", "cm"),
    ),
    ("day", unit(" day", " days", "d", " day", " days", "d")),
    (
        "degree",
        unit(" deg", " deg", "\u{b0}", " degree", " degrees", "deg"),
    ),
    (
        "fahrenheit",
        unit(
            "\u{b0}F",
            "\u{b0}F",
            "\u{b0}",
            " degree Fahrenheit",
            " degrees Fahrenheit",
            "\u{b0}F",
        ),
    ),
    (
        "fluid-ounce",
        unit(
            " fl oz",
            " fl oz",
            "fl oz",
            " fluid ounce",
            " fluid ounces",
            "fl oz",
        ),
    ),
    (
        "foot",
        unit(" ft", " ft", "\u{2032}", " foot", " feet", "ft"),
    ),
    (
        "gallon",
        unit(" gal", " gal", "gal", " gallon", " gallons", "gal"),
    ),
    (
        "gigabit",
        unit(" Gb", " Gb", "Gb", " gigabit", " gigabits", "Gb"),
    ),
    (
        "gigabyte",
        unit(" GB", " GB", "GB", " gigabyte", " gigabytes", "GB"),
    ),
    ("gram", unit(" g", " g", "g", " gram", " grams", "g")),
    (
        "hectare",
        unit(" ha", " ha", "ha", " hectare", " hectares", "ha"),
    ),
    ("hour", unit(" hr", " hr", "h", " hour", " hours", "h")),
    (
        "inch",
        unit(" in", " in", "\u{2033}", " inch", " inches", "in"),
    ),
    (
        "kilobit",
        unit(" kb", " kb", "kb", " kilobit", " kilobits", "kb"),
    ),
    (
        "kilobyte",
        unit(" kB", " kB", "kB", " kilobyte", " kilobytes", "kB"),
    ),
    (
        "kilogram",
        unit(" kg", " kg", "kg", " kilogram", " kilograms", "kg"),
    ),
    (
        "kilometer",
        unit(" km", " km", "km", " kilometer", " kilometers", "km"),
    ),
    ("liter", unit(" L", " L", "L", " liter", " liters", "L")),
    (
        "megabit",
        unit(" Mb", " Mb", "Mb", " megabit", " megabits", "Mb"),
    ),
    (
        "megabyte",
        unit(" MB", " MB", "MB", " megabyte", " megabytes", "MB"),
    ),
    ("meter", unit(" m", " m", "m", " meter", " meters", "m")),
    (
        "microsecond",
        unit(
            " \u{3bc}s",
            " \u{3bc}s",
            "\u{3bc}s",
            " microsecond",
            " microseconds",
            "\u{3bc}s",
        ),
    ),
    ("mile", unit(" mi", " mi", "mi", " mile", " miles", "mi")),
    (
        "mile-scandinavian",
        unit(
            " smi",
            " smi",
            "smi",
            " mile-scandinavian",
            " miles-scandinavian",
            "smi",
        ),
    ),
    (
        "milliliter",
        unit(" mL", " mL", "mL", " milliliter", " milliliters", "mL"),
    ),
    (
        "millimeter",
        unit(" mm", " mm", "mm", " millimeter", " millimeters", "mm"),
    ),
    (
        "millisecond",
        unit(" ms", " ms", "ms", " millisecond", " milliseconds", "ms"),
    ),
    (
        "minute",
        unit(" min", " min", "m", " minute", " minutes", "min"),
    ),
    (
        "month",
        unit(" mth", " mths", "m", " month", " months", "m"),
    ),
    (
        "nanosecond",
        unit(" ns", " ns", "ns", " nanosecond", " nanoseconds", "ns"),
    ),
    ("ounce", unit(" oz", " oz", "oz", " ounce", " ounces", "oz")),
    ("percent", unit("%", "%", "%", " percent", " percent", "%")),
    (
        "petabyte",
        unit(" PB", " PB", "PB", " petabyte", " petabytes", "PB"),
    ),
    ("pound", unit(" lb", " lb", "lb", " pound", " pounds", "lb")),
    (
        "second",
        unit(" sec", " sec", "s", " second", " seconds", "s"),
    ),
    ("stone", unit(" st", " st", "st", " stone", " stones", "st")),
    (
        "terabit",
        unit(" Tb", " Tb", "Tb", " terabit", " terabits", "Tb"),
    ),
    (
        "terabyte",
        unit(" TB", " TB", "TB", " terabyte", " terabytes", "TB"),
    ),
    ("week", unit(" wk", " wks", "w", " week", " weeks", "w")),
    ("yard", unit(" yd", " yd", "yd", " yard", " yards", "yd")),
    ("year", unit(" yr", " yrs", "y", " year", " years", "y")),
];

pub(crate) fn simple_units() -> impl Iterator<Item = &'static str> {
    SIMPLE_UNITS.iter().map(|(name, _)| *name)
}

pub(crate) fn simple_unit(name: &str) -> Option<&'static UnitNames> {
    SIMPLE_UNITS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, names)| names)
}

pub(crate) struct UnitPattern {
    pub prefix: String,
    pub suffix: String,
}

fn pattern(prefix: &str, suffix: &str) -> UnitPattern {
    UnitPattern {
        prefix: prefix.to_owned(),
        suffix: suffix.to_owned(),
    }
}

const NAMED_UNITS: [(&str, &str, [&str; 4]); 10] = [
    (
        "en",
        "kilometer-per-hour",
        [
            "{0} km/h",
            "{0}km/h",
            "{0} kilometer per hour",
            "{0} kilometers per hour",
        ],
    ),
    (
        "en",
        "mile-per-hour",
        [
            "{0} mph",
            "{0}mph",
            "{0} mile per hour",
            "{0} miles per hour",
        ],
    ),
    (
        "en",
        "meter-per-second",
        [
            "{0} m/s",
            "{0}m/s",
            "{0} meter per second",
            "{0} meters per second",
        ],
    ),
    (
        "en",
        "mile-per-gallon",
        [
            "{0} mpg",
            "{0}mpg",
            "{0} mile per gallon",
            "{0} miles per gallon",
        ],
    ),
    (
        "en",
        "liter-per-kilometer",
        [
            "{0} L/km",
            "{0}L/km",
            "{0} liter per kilometer",
            "{0} liters per kilometer",
        ],
    ),
    (
        "de",
        "kilometer-per-hour",
        [
            "{0} km/h",
            "{0} km/h",
            "{0} Kilometer pro Stunde",
            "{0} Kilometer pro Stunde",
        ],
    ),
    (
        "ja",
        "kilometer-per-hour",
        [
            "{0} km/h",
            "{0}km/h",
            "時速 {0} キロメートル",
            "時速 {0} キロメートル",
        ],
    ),
    (
        "ko",
        "kilometer-per-hour",
        ["{0}km/h", "{0}km/h", "시속 {0}킬로미터", "시속 {0}킬로미터"],
    ),
    (
        "zh",
        "kilometer-per-hour",
        [
            "{0} 公里/小时",
            "{0}公里/小时",
            "每小时 {0} 公里",
            "每小时 {0} 公里",
        ],
    ),
    (
        "zh-Hant",
        "kilometer-per-hour",
        [
            "{0} 公里/小時",
            "{0}公里/小時",
            "每小時 {0} 公里",
            "每小時 {0} 公里",
        ],
    ),
];

fn named_unit(language: &str, unit: &str) -> Option<&'static [&'static str; 4]> {
    let find = |wanted: &str| {
        NAMED_UNITS
            .iter()
            .find(|(lang, known, _)| *lang == wanted && *known == unit)
            .map(|(_, _, patterns)| patterns)
    };
    find(language).or_else(|| find("en"))
}

pub(crate) fn unit_pattern(tag: &str, unit: &str, display: &str, plural_one: bool) -> UnitPattern {
    let width = match (display, plural_one) {
        ("narrow", _) => 1,
        ("long", true) => 2,
        ("long", false) => 3,
        _ => 0,
    };
    let language = if uses_traditional_chinese(tag) {
        "zh-Hant"
    } else {
        language_parts(tag).0
    };
    if let Some(patterns) = named_unit(language, unit) {
        let (prefix, suffix) = patterns[width].split_once("{0}").unwrap_or(("", ""));
        return pattern(prefix, suffix);
    }
    if let Some((numerator, denominator)) = unit.split_once("-per-") {
        let (Some(top), Some(bottom)) = (simple_unit(numerator), simple_unit(denominator)) else {
            return pattern("", unit);
        };
        let suffix = match width {
            1 => format!("{}/{}", top.narrow, bottom.per),
            2 | 3 => {
                let top = if plural_one {
                    top.long
                } else {
                    top.long_plural
                };
                format!("{top} per{}", bottom.long)
            }
            _ => {
                let top = if plural_one {
                    top.short
                } else {
                    top.short_plural
                };
                format!("{top}/{}", bottom.per)
            }
        };
        return UnitPattern {
            prefix: String::new(),
            suffix,
        };
    }
    let Some(names) = simple_unit(unit) else {
        return pattern("", unit);
    };
    let suffix = match (width, plural_one) {
        (1, _) => names.narrow,
        (2, _) => names.long,
        (3, _) => names.long_plural,
        (_, true) => names.short,
        (_, false) => names.short_plural,
    };
    pattern("", suffix)
}
