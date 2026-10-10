//! Southstar — the name grammars the DOM factories check: element local names, attribute names, qualified names, XML names and doctype names.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) const XML_NS: &[u8] = b"http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS_NS: &[u8] = b"http://www.w3.org/2000/xmlns/";
pub(crate) const HTML_NS: &[u8] = b"http://www.w3.org/1999/xhtml";
pub(crate) const SVG_NS: &[u8] = b"http://www.w3.org/2000/svg";

fn forbidden(c: u32) -> bool {
    matches!(c, 0 | 0x20 | 0x09 | 0x0A | 0x0D | 0x0C)
        || c == u32::from(b'>')
        || c == u32::from(b'/')
}

fn decoded(s: &[u8]) -> Option<&str> {
    core::str::from_utf8(s).ok()
}

fn no_forbidden(s: &[u8]) -> bool {
    let ascii = s.iter().take_while(|&&b| b < 0x80).count();
    if s[..ascii].iter().any(|&b| forbidden(u32::from(b))) {
        return false;
    }
    if ascii == s.len() {
        return true;
    }
    decoded(&s[ascii..]).is_some_and(|rest| !rest.chars().any(|c| forbidden(c as u32)))
}

pub(crate) fn valid_element_local_name(s: &[u8]) -> bool {
    let Some(&first_byte) = s.first() else {
        return false;
    };
    if first_byte.is_ascii_alphabetic() {
        return no_forbidden(s);
    }
    let Some(text) = decoded(s) else {
        return false;
    };
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != ':' && first != '_' && first.is_ascii() {
        return false;
    }
    chars.all(|c| !c.is_ascii() || c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | ':' | '_'))
}

pub(crate) fn valid_attr_name(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    decoded(s).is_some_and(|text| !text.chars().any(|c| forbidden(c as u32) || c == '='))
}

fn split_qname(s: &[u8], local_valid: fn(&[u8]) -> bool) -> bool {
    if s.is_empty() {
        return false;
    }
    let Some(colon) = s.iter().position(|&b| b == b':') else {
        return local_valid(s);
    };
    if colon == 0 || colon + 1 == s.len() || !no_forbidden(&s[..colon]) {
        return false;
    }
    local_valid(&s[colon + 1..])
}

pub(crate) fn is_xml_qname(s: &[u8]) -> bool {
    split_qname(s, valid_element_local_name)
}

pub(crate) fn is_attr_qname(s: &[u8]) -> bool {
    split_qname(s, valid_attr_name)
}

pub(crate) fn valid_doctype_name(s: &[u8]) -> bool {
    !s.iter()
        .any(|&b| matches!(b, 0 | b'>' | b' ' | b'\t' | b'\n' | b'\x0c' | b'\r'))
}

fn xml_name_start(c: char) -> bool {
    matches!(c as u32,
        0x3A | 0x5F
        | 0x41..=0x5A
        | 0x61..=0x7A
        | 0xC0..=0xD6
        | 0xD8..=0xF6
        | 0xF8..=0x2FF
        | 0x370..=0x37D
        | 0x37F..=0x1FFF
        | 0x200C..=0x200D
        | 0x2070..=0x218F
        | 0x2C00..=0x2FEF
        | 0x3001..=0xD7FF
        | 0xF900..=0xFDCF
        | 0xFDF0..=0xFFFD
        | 0x10000..=0xEFFFF)
}

fn xml_name_char(c: char) -> bool {
    xml_name_start(c)
        || matches!(c as u32,
            0x2D | 0x2E | 0xB7
            | 0x30..=0x39
            | 0x300..=0x36F
            | 0x203F..=0x2040)
}

pub(crate) fn is_xml_name(s: &[u8]) -> bool {
    let Some(text) = decoded(s) else {
        return false;
    };
    let mut chars = text.chars();
    chars.next().is_some_and(xml_name_start) && chars.all(xml_name_char)
}

pub(crate) struct NameError {
    pub name: &'static core::ffi::CStr,
    pub code: i32,
    pub message: &'static core::ffi::CStr,
}

pub(crate) const INVALID_CHARACTER_ERR: i32 = 5;
pub(crate) const NOT_SUPPORTED_ERR: i32 = 9;
pub(crate) const NAMESPACE_ERR: i32 = 14;

pub(crate) fn invalid_character(message: &'static core::ffi::CStr) -> NameError {
    NameError {
        name: c"InvalidCharacterError",
        code: INVALID_CHARACTER_ERR,
        message,
    }
}

fn namespace_error(message: &'static core::ffi::CStr) -> NameError {
    NameError {
        name: c"NamespaceError",
        code: NAMESPACE_ERR,
        message,
    }
}

pub(crate) fn validate_attr_ns(ns_uri: Option<&[u8]>, qname: &[u8]) -> Result<(), NameError> {
    if !is_attr_qname(qname) {
        return Err(invalid_character(c"invalid qualified name"));
    }
    let prefix = qname
        .iter()
        .position(|&b| b == b':')
        .map(|colon| &qname[..colon]);
    let ns_is_xmlns = ns_uri == Some(XMLNS_NS);
    let ns_is_xml = ns_uri == Some(XML_NS);
    let name_is_xmlns = qname == b"xmlns" || prefix == Some(b"xmlns");
    if prefix.is_some() && ns_uri.is_none() {
        return Err(namespace_error(c"a prefix requires a namespace"));
    }
    if prefix == Some(b"xml") && !ns_is_xml {
        return Err(namespace_error(
            c"the xml prefix requires the XML namespace",
        ));
    }
    if name_is_xmlns != ns_is_xmlns {
        return Err(namespace_error(c"xmlns must use the XMLNS namespace"));
    }
    Ok(())
}

pub(crate) fn validate_element_ns(ns: Option<&[u8]>, name: &[u8]) -> Result<(), NameError> {
    let prefix = name
        .iter()
        .position(|&b| b == b':')
        .map(|colon| &name[..colon]);
    let prefix_is_xmlns = prefix == Some(b"xmlns");
    let name_is_xmlns = name == b"xmlns";
    let message = c"invalid qualified name";
    if name.contains(&0) || !is_xml_qname(name) {
        return Err(invalid_character(message));
    }
    let violation = (prefix.is_some() && ns.is_none())
        || (prefix == Some(b"xml") && ns != Some(XML_NS))
        || ((name_is_xmlns || prefix_is_xmlns) && ns != Some(XMLNS_NS))
        || (ns == Some(XMLNS_NS) && !name_is_xmlns && !prefix_is_xmlns);
    if violation {
        return Err(namespace_error(message));
    }
    Ok(())
}
