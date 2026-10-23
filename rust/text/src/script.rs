//! Southstar text — Unicode scripts: the GLib script codes, per-character lookup and the script run iterator.
//! Copyright 1999-2024 The Pango authors; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Script(pub i32);

impl Script {
    pub const INVALID_CODE: Script = Script(-1);
    pub const COMMON: Script = Script(0);
    pub const INHERITED: Script = Script(1);
    pub const ARABIC: Script = Script(2);
    pub const ARMENIAN: Script = Script(3);
    pub const BENGALI: Script = Script(4);
    pub const BOPOMOFO: Script = Script(5);
    pub const CHEROKEE: Script = Script(6);
    pub const COPTIC: Script = Script(7);
    pub const CYRILLIC: Script = Script(8);
    pub const DESERET: Script = Script(9);
    pub const DEVANAGARI: Script = Script(10);
    pub const ETHIOPIC: Script = Script(11);
    pub const GEORGIAN: Script = Script(12);
    pub const GOTHIC: Script = Script(13);
    pub const GREEK: Script = Script(14);
    pub const GUJARATI: Script = Script(15);
    pub const GURMUKHI: Script = Script(16);
    pub const HAN: Script = Script(17);
    pub const HANGUL: Script = Script(18);
    pub const HEBREW: Script = Script(19);
    pub const HIRAGANA: Script = Script(20);
    pub const KANNADA: Script = Script(21);
    pub const KATAKANA: Script = Script(22);
    pub const KHMER: Script = Script(23);
    pub const LAO: Script = Script(24);
    pub const LATIN: Script = Script(25);
    pub const MALAYALAM: Script = Script(26);
    pub const MONGOLIAN: Script = Script(27);
    pub const MYANMAR: Script = Script(28);
    pub const OGHAM: Script = Script(29);
    pub const OLD_ITALIC: Script = Script(30);
    pub const ORIYA: Script = Script(31);
    pub const RUNIC: Script = Script(32);
    pub const SINHALA: Script = Script(33);
    pub const SYRIAC: Script = Script(34);
    pub const TAMIL: Script = Script(35);
    pub const TELUGU: Script = Script(36);
    pub const THAANA: Script = Script(37);
    pub const THAI: Script = Script(38);
    pub const TIBETAN: Script = Script(39);
    pub const CANADIAN_ABORIGINAL: Script = Script(40);
    pub const YI: Script = Script(41);
    pub const TAGALOG: Script = Script(42);
    pub const HANUNOO: Script = Script(43);
    pub const BUHID: Script = Script(44);
    pub const TAGBANWA: Script = Script(45);
    pub const BRAILLE: Script = Script(46);
    pub const CYPRIOT: Script = Script(47);
    pub const LIMBU: Script = Script(48);
    pub const OSMANYA: Script = Script(49);
    pub const SHAVIAN: Script = Script(50);
    pub const LINEAR_B: Script = Script(51);
    pub const TAI_LE: Script = Script(52);
    pub const UGARITIC: Script = Script(53);
    pub const NEW_TAI_LUE: Script = Script(54);
    pub const BUGINESE: Script = Script(55);
    pub const GLAGOLITIC: Script = Script(56);
    pub const TIFINAGH: Script = Script(57);
    pub const SYLOTI_NAGRI: Script = Script(58);
    pub const OLD_PERSIAN: Script = Script(59);
    pub const KHAROSHTHI: Script = Script(60);
    pub const UNKNOWN: Script = Script(61);
    pub const BALINESE: Script = Script(62);
    pub const CUNEIFORM: Script = Script(63);
    pub const PHOENICIAN: Script = Script(64);
    pub const PHAGS_PA: Script = Script(65);
    pub const NKO: Script = Script(66);
    pub const KAYAH_LI: Script = Script(67);
    pub const LEPCHA: Script = Script(68);
    pub const REJANG: Script = Script(69);
    pub const SUNDANESE: Script = Script(70);
    pub const SAURASHTRA: Script = Script(71);
    pub const CHAM: Script = Script(72);
    pub const OL_CHIKI: Script = Script(73);
    pub const VAI: Script = Script(74);
    pub const CARIAN: Script = Script(75);
    pub const LYCIAN: Script = Script(76);
    pub const LYDIAN: Script = Script(77);
    pub const AVESTAN: Script = Script(78);
    pub const BAMUM: Script = Script(79);
    pub const EGYPTIAN_HIEROGLYPHS: Script = Script(80);
    pub const IMPERIAL_ARAMAIC: Script = Script(81);
    pub const INSCRIPTIONAL_PAHLAVI: Script = Script(82);
    pub const INSCRIPTIONAL_PARTHIAN: Script = Script(83);
    pub const JAVANESE: Script = Script(84);
    pub const KAITHI: Script = Script(85);
    pub const LISU: Script = Script(86);
    pub const MEETEI_MAYEK: Script = Script(87);
    pub const OLD_SOUTH_ARABIAN: Script = Script(88);
    pub const OLD_TURKIC: Script = Script(89);
    pub const SAMARITAN: Script = Script(90);
    pub const TAI_THAM: Script = Script(91);
    pub const TAI_VIET: Script = Script(92);
    pub const BATAK: Script = Script(93);
    pub const BRAHMI: Script = Script(94);
    pub const MANDAIC: Script = Script(95);
    pub const CHAKMA: Script = Script(96);
    pub const MEROITIC_CURSIVE: Script = Script(97);
    pub const MEROITIC_HIEROGLYPHS: Script = Script(98);
    pub const MIAO: Script = Script(99);
    pub const SHARADA: Script = Script(100);
    pub const SORA_SOMPENG: Script = Script(101);
    pub const TAKRI: Script = Script(102);
    pub const BASSA_VAH: Script = Script(103);
    pub const CAUCASIAN_ALBANIAN: Script = Script(104);
    pub const DUPLOYAN: Script = Script(105);
    pub const ELBASAN: Script = Script(106);
    pub const GRANTHA: Script = Script(107);
    pub const KHOJKI: Script = Script(108);
    pub const KHUDAWADI: Script = Script(109);
    pub const LINEAR_A: Script = Script(110);
    pub const MAHAJANI: Script = Script(111);
    pub const MANICHAEAN: Script = Script(112);
    pub const MENDE_KIKAKUI: Script = Script(113);
    pub const MODI: Script = Script(114);
    pub const MRO: Script = Script(115);
    pub const NABATAEAN: Script = Script(116);
    pub const OLD_NORTH_ARABIAN: Script = Script(117);
    pub const OLD_PERMIC: Script = Script(118);
    pub const PAHAWH_HMONG: Script = Script(119);
    pub const PALMYRENE: Script = Script(120);
    pub const PAU_CIN_HAU: Script = Script(121);
    pub const PSALTER_PAHLAVI: Script = Script(122);
    pub const SIDDHAM: Script = Script(123);
    pub const TIRHUTA: Script = Script(124);
    pub const WARANG_CITI: Script = Script(125);
    pub const AHOM: Script = Script(126);
    pub const ANATOLIAN_HIEROGLYPHS: Script = Script(127);
    pub const HATRAN: Script = Script(128);
    pub const MULTANI: Script = Script(129);
    pub const OLD_HUNGARIAN: Script = Script(130);
    pub const SIGNWRITING: Script = Script(131);
    pub const ADLAM: Script = Script(132);
    pub const BHAIKSUKI: Script = Script(133);
    pub const MARCHEN: Script = Script(134);
    pub const NEWA: Script = Script(135);
    pub const OSAGE: Script = Script(136);
    pub const TANGUT: Script = Script(137);
    pub const MASARAM_GONDI: Script = Script(138);
    pub const NUSHU: Script = Script(139);
    pub const SOYOMBO: Script = Script(140);
    pub const ZANABAZAR_SQUARE: Script = Script(141);
    pub const DOGRA: Script = Script(142);
    pub const GUNJALA_GONDI: Script = Script(143);
    pub const HANIFI_ROHINGYA: Script = Script(144);
    pub const MAKASAR: Script = Script(145);
    pub const MEDEFAIDRIN: Script = Script(146);
    pub const OLD_SOGDIAN: Script = Script(147);
    pub const SOGDIAN: Script = Script(148);
    pub const ELYMAIC: Script = Script(149);
    pub const NANDINAGARI: Script = Script(150);
    pub const NYIAKENG_PUACHUE_HMONG: Script = Script(151);
    pub const WANCHO: Script = Script(152);
    pub const CHORASMIAN: Script = Script(153);
    pub const DIVES_AKURU: Script = Script(154);
    pub const KHITAN_SMALL_SCRIPT: Script = Script(155);
    pub const YEZIDI: Script = Script(156);
    pub const CYPRO_MINOAN: Script = Script(157);
    pub const OLD_UYGHUR: Script = Script(158);
    pub const TANGSA: Script = Script(159);
    pub const TOTO: Script = Script(160);
    pub const VITHKUQI: Script = Script(161);
    pub const MATH: Script = Script(162);
    pub const KAWI: Script = Script(163);
    pub const NAG_MUNDARI: Script = Script(164);
    pub const TODHRI: Script = Script(165);
    pub const GARAY: Script = Script(166);
    pub const TULU_TIGALARI: Script = Script(167);
    pub const SUNUWAR: Script = Script(168);
    pub const GURUNG_KHEMA: Script = Script(169);
    pub const KIRAT_RAI: Script = Script(170);
    pub const OL_ONAL: Script = Script(171);
    pub const SIDETIC: Script = Script(172);
    pub const TOLONG_SIKI: Script = Script(173);
    pub const TAI_YO: Script = Script(174);
    pub const BERIA_ERFE: Script = Script(175);
}

impl Script {
    pub fn for_char(ch: u32) -> Script {
        crate::unicode::script(ch)
    }

    pub fn is_real(self) -> bool {
        self > Script::INHERITED && self != Script::UNKNOWN
    }
}

const PAREN_STACK_DEPTH: usize = 128;

const PAIRED_CHARS: [u32; 116] = [
    0x0028, 0x0029, 0x003c, 0x003e, 0x005b, 0x005d, 0x007b, 0x007d, 0x00ab, 0x00bb, 0x0f3a, 0x0f3b,
    0x0f3c, 0x0f3d, 0x169b, 0x169c, 0x2018, 0x2019, 0x201c, 0x201d, 0x2039, 0x203a, 0x2045, 0x2046,
    0x207d, 0x207e, 0x208d, 0x208e, 0x27e6, 0x27e7, 0x27e8, 0x27e9, 0x27ea, 0x27eb, 0x27ec, 0x27ed,
    0x27ee, 0x27ef, 0x2983, 0x2984, 0x2985, 0x2986, 0x2987, 0x2988, 0x2989, 0x298a, 0x298b, 0x298c,
    0x298d, 0x298e, 0x298f, 0x2990, 0x2991, 0x2992, 0x2993, 0x2994, 0x2995, 0x2996, 0x2997, 0x2998,
    0x29fc, 0x29fd, 0x2e02, 0x2e03, 0x2e04, 0x2e05, 0x2e09, 0x2e0a, 0x2e0c, 0x2e0d, 0x2e1c, 0x2e1d,
    0x2e20, 0x2e21, 0x2e22, 0x2e23, 0x2e24, 0x2e25, 0x2e26, 0x2e27, 0x2e28, 0x2e29, 0x3008, 0x3009,
    0x300a, 0x300b, 0x300c, 0x300d, 0x300e, 0x300f, 0x3010, 0x3011, 0x3014, 0x3015, 0x3016, 0x3017,
    0x3018, 0x3019, 0x301a, 0x301b, 0xfe59, 0xfe5a, 0xfe5b, 0xfe5c, 0xfe5d, 0xfe5e, 0xff08, 0xff09,
    0xff3b, 0xff3d, 0xff5b, 0xff5d, 0xff5f, 0xff60, 0xff62, 0xff63,
];

fn pair_index(ch: u32) -> Option<usize> {
    PAIRED_CHARS.binary_search(&ch).ok()
}

fn is_cluster_extender(ch: u32) -> bool {
    use crate::unicode::UnicodeType;
    let category = crate::unicode::unichar_type(ch);
    (category >= UnicodeType::SpacingMark && category <= UnicodeType::NonSpacingMark)
        || (0x200C..=0x200D).contains(&ch)
        || (0xFF9E..=0xFF9F).contains(&ch)
        || (0x1F3FB..=0x1F3FF).contains(&ch)
        || (0xE0020..=0xE007F).contains(&ch)
}

fn same_script(script1: Script, script2: Script, ch: u32) -> bool {
    !script1.is_real() || !script2.is_real() || script1 == script2 || is_cluster_extender(ch)
}

#[derive(Clone, Copy)]
struct ParenStackEntry {
    pair_index: usize,
    script_code: Script,
}

#[derive(Clone)]
pub struct ScriptIter<'a> {
    text: &'a str,
    script_start: usize,
    script_end: usize,
    script_code: Script,
    paren_stack: [ParenStackEntry; PAREN_STACK_DEPTH],
    paren_sp: isize,
    exhausted: bool,
}

impl<'a> ScriptIter<'a> {
    pub fn new(text: &'a str) -> ScriptIter<'a> {
        let mut iter = ScriptIter {
            text,
            script_start: 0,
            script_end: 0,
            script_code: Script::COMMON,
            paren_stack: [ParenStackEntry {
                pair_index: 0,
                script_code: Script::COMMON,
            }; PAREN_STACK_DEPTH],
            paren_sp: -1,
            exhausted: false,
        };
        iter.next_range();
        iter
    }

    pub fn range(&self) -> (usize, usize, Script) {
        (self.script_start, self.script_end, self.script_code)
    }

    fn stack_slot(&mut self, sp: isize) -> &mut ParenStackEntry {
        &mut self.paren_stack[sp.unsigned_abs()]
    }

    pub fn next_range(&mut self) -> bool {
        if self.script_end == self.text.len() {
            return false;
        }

        let mut start_sp = self.paren_sp;
        self.script_code = Script::COMMON;
        self.script_start = self.script_end;

        let text = self.text;
        for (offset, character) in text[self.script_end..].char_indices() {
            self.script_end = self.script_start + offset;
            let ch = u32::from(character);
            let mut sc = Script::for_char(ch);
            let pair = if sc != Script::COMMON {
                None
            } else {
                pair_index(ch)
            };

            if let Some(pair) = pair {
                if pair & 1 == 0 {
                    self.paren_sp += 1;
                    if self.paren_sp >= PAREN_STACK_DEPTH as isize {
                        self.paren_sp = 0;
                    }
                    let script_code = self.script_code;
                    let sp = self.paren_sp;
                    *self.stack_slot(sp) = ParenStackEntry {
                        pair_index: pair,
                        script_code,
                    };
                } else if self.paren_sp >= 0 {
                    let opening = pair & !1;
                    while self.paren_sp >= 0
                        && self.paren_stack[self.paren_sp.unsigned_abs()].pair_index != opening
                    {
                        self.paren_sp -= 1;
                    }
                    if self.paren_sp < start_sp {
                        start_sp = self.paren_sp;
                    }
                    if self.paren_sp >= 0 {
                        sc = self.paren_stack[self.paren_sp.unsigned_abs()].script_code;
                    }
                }
            }

            if same_script(self.script_code, sc, ch) {
                if !self.script_code.is_real() && sc.is_real() {
                    self.script_code = sc;
                    while start_sp < self.paren_sp {
                        start_sp += 1;
                        self.stack_slot(start_sp).script_code = sc;
                    }
                }
                if pair.is_some_and(|p| p & 1 == 1) && self.paren_sp >= 0 {
                    self.paren_sp -= 1;
                    if self.paren_sp < start_sp {
                        start_sp = self.paren_sp;
                    }
                }
            } else {
                return true;
            }
        }
        self.script_end = self.text.len();
        true
    }
}

impl Iterator for ScriptIter<'_> {
    type Item = (usize, usize, Script);

    fn next(&mut self) -> Option<(usize, usize, Script)> {
        if self.exhausted {
            return None;
        }
        let range = self.range();
        self.exhausted = !self.next_range();
        Some(range)
    }
}
