//! Southstar — turning the tab's key presses into the DOM key, code and keyCode the renderer dispatches, and the editing shortcuts the shell handles itself.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const KEY_SPACE: u32 = 0x020;
pub const KEY_PLUS: u32 = 0x02b;
pub const KEY_MINUS: u32 = 0x02d;
pub const KEY_0: u32 = 0x030;
pub const KEY_9: u32 = 0x039;
pub const KEY_EQUAL: u32 = 0x03d;
pub const KEY_UPPER_A: u32 = 0x041;
pub const KEY_UPPER_F: u32 = 0x046;
pub const KEY_UPPER_G: u32 = 0x047;
pub const KEY_UPPER_J: u32 = 0x04a;
pub const KEY_UPPER_P: u32 = 0x050;
pub const KEY_UPPER_Z: u32 = 0x05a;
pub const KEY_A: u32 = 0x061;
pub const KEY_C: u32 = 0x063;
pub const KEY_F: u32 = 0x066;
pub const KEY_G: u32 = 0x067;
pub const KEY_J: u32 = 0x06a;
pub const KEY_P: u32 = 0x070;
pub const KEY_V: u32 = 0x076;
pub const KEY_X: u32 = 0x078;
pub const KEY_Z: u32 = 0x07a;
pub const KEY_ISO_LEFT_TAB: u32 = 0xfe20;
pub const KEY_BACKSPACE: u32 = 0xff08;
pub const KEY_TAB: u32 = 0xff09;
pub const KEY_RETURN: u32 = 0xff0d;
pub const KEY_ESCAPE: u32 = 0xff1b;
pub const KEY_HOME: u32 = 0xff50;
pub const KEY_LEFT: u32 = 0xff51;
pub const KEY_UP: u32 = 0xff52;
pub const KEY_RIGHT: u32 = 0xff53;
pub const KEY_DOWN: u32 = 0xff54;
pub const KEY_PAGE_UP: u32 = 0xff55;
pub const KEY_PAGE_DOWN: u32 = 0xff56;
pub const KEY_END: u32 = 0xff57;
pub const KEY_INSERT: u32 = 0xff63;
pub const KEY_KP_ENTER: u32 = 0xff8d;
pub const KEY_KP_INSERT: u32 = 0xff9e;
pub const KEY_KP_ADD: u32 = 0xffab;
pub const KEY_KP_SUBTRACT: u32 = 0xffad;
pub const KEY_KP_0: u32 = 0xffb0;
pub const KEY_F12: u32 = 0xffc9;
pub const KEY_SHIFT_L: u32 = 0xffe1;
pub const KEY_SHIFT_R: u32 = 0xffe2;
pub const KEY_CONTROL_L: u32 = 0xffe3;
pub const KEY_CONTROL_R: u32 = 0xffe4;
pub const KEY_ALT_L: u32 = 0xffe9;
pub const KEY_ALT_R: u32 = 0xffea;
pub const KEY_DELETE: u32 = 0xffff;

pub const SHIFT: u32 = 1 << 0;
pub const CONTROL: u32 = 1 << 2;
pub const ALT: u32 = 1 << 3;
pub const META: u32 = 1 << 28;

#[cfg(target_os = "macos")]
pub const PRIMARY: u32 = CONTROL | META;
#[cfg(not(target_os = "macos"))]
pub const PRIMARY: u32 = CONTROL;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditOp {
    None = 0,
    Cut = 1,
    Copy = 2,
    Paste = 3,
    SelectAll = 4,
}

pub fn modifier_bits(state: u32) -> i32 {
    i32::from(state & SHIFT != 0)
        | (i32::from(state & CONTROL != 0) << 1)
        | (i32::from(state & ALT != 0) << 2)
        | (i32::from(state & META != 0) << 3)
}

fn named_key(keyval: u32) -> Option<&'static str> {
    Some(match keyval {
        KEY_UP => "ArrowUp",
        KEY_DOWN => "ArrowDown",
        KEY_LEFT => "ArrowLeft",
        KEY_RIGHT => "ArrowRight",
        KEY_RETURN | KEY_KP_ENTER => "Enter",
        KEY_ESCAPE => "Escape",
        KEY_BACKSPACE => "Backspace",
        KEY_TAB | KEY_ISO_LEFT_TAB => "Tab",
        KEY_DELETE => "Delete",
        KEY_INSERT => "Insert",
        KEY_HOME => "Home",
        KEY_END => "End",
        KEY_PAGE_UP => "PageUp",
        KEY_PAGE_DOWN => "PageDown",
        KEY_SHIFT_L | KEY_SHIFT_R => "Shift",
        KEY_CONTROL_L | KEY_CONTROL_R => "Control",
        KEY_ALT_L | KEY_ALT_R => "Alt",
        _ => return None,
    })
}

pub fn js_key(keyval: u32, unicode: u32, name: impl FnOnce() -> Vec<u8>) -> Vec<u8> {
    if unicode >= 0x20
        && unicode != 0x7f
        && let Some(c) = char::from_u32(unicode)
    {
        let mut buf = [0u8; 4];
        return c.encode_utf8(&mut buf).as_bytes().to_vec();
    }
    named_key(keyval).map_or_else(name, |key| key.as_bytes().to_vec())
}

pub fn js_code(keyval: u32) -> String {
    match keyval {
        KEY_A..=KEY_Z => format!("Key{}", char::from(b'A' + (keyval - KEY_A) as u8)),
        KEY_UPPER_A..=KEY_UPPER_Z => {
            format!("Key{}", char::from(b'A' + (keyval - KEY_UPPER_A) as u8))
        }
        KEY_0..=KEY_9 => format!("Digit{}", char::from(b'0' + (keyval - KEY_0) as u8)),
        _ => match keyval {
            KEY_UP => "ArrowUp",
            KEY_DOWN => "ArrowDown",
            KEY_LEFT => "ArrowLeft",
            KEY_RIGHT => "ArrowRight",
            KEY_RETURN => "Enter",
            KEY_KP_ENTER => "NumpadEnter",
            KEY_ESCAPE => "Escape",
            KEY_BACKSPACE => "Backspace",
            KEY_TAB | KEY_ISO_LEFT_TAB => "Tab",
            KEY_DELETE => "Delete",
            KEY_HOME => "Home",
            KEY_END => "End",
            KEY_PAGE_UP => "PageUp",
            KEY_PAGE_DOWN => "PageDown",
            KEY_SPACE => "Space",
            _ => "",
        }
        .to_string(),
    }
}

pub fn js_keycode(keyval: u32) -> i32 {
    match keyval {
        KEY_A..=KEY_Z => 65 + (keyval - KEY_A) as i32,
        KEY_UPPER_A..=KEY_UPPER_Z => 65 + (keyval - KEY_UPPER_A) as i32,
        KEY_0..=KEY_9 => 48 + (keyval - KEY_0) as i32,
        KEY_BACKSPACE => 8,
        KEY_TAB => 9,
        KEY_RETURN | KEY_KP_ENTER => 13,
        KEY_ESCAPE => 27,
        KEY_SPACE => 32,
        KEY_PAGE_UP => 33,
        KEY_PAGE_DOWN => 34,
        KEY_END => 35,
        KEY_HOME => 36,
        KEY_LEFT => 37,
        KEY_UP => 38,
        KEY_RIGHT => 39,
        KEY_DOWN => 40,
        KEY_DELETE => 46,
        _ => 0,
    }
}

pub fn edit_op(keyval: u32, state: u32, lower: u32) -> EditOp {
    let mods = state & (SHIFT | CONTROL | ALT | META);
    if keyval == KEY_INSERT || keyval == KEY_KP_INSERT {
        return match mods {
            SHIFT => EditOp::Paste,
            CONTROL => EditOp::Copy,
            _ => EditOp::None,
        };
    }
    if mods & PRIMARY == 0 || mods & !(PRIMARY | SHIFT) != 0 {
        return EditOp::None;
    }
    match lower {
        KEY_X => EditOp::Cut,
        KEY_C => EditOp::Copy,
        KEY_V => EditOp::Paste,
        KEY_A => EditOp::SelectAll,
        _ => EditOp::None,
    }
}

pub enum ControlKey {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Find,
    FindNext { backwards: bool },
    Print,
    Other,
}

pub fn control_key(keyval: u32, state: u32) -> ControlKey {
    match keyval {
        KEY_PLUS | KEY_EQUAL | KEY_KP_ADD => ControlKey::ZoomIn,
        KEY_MINUS | KEY_KP_SUBTRACT => ControlKey::ZoomOut,
        KEY_0 | KEY_KP_0 => ControlKey::ZoomReset,
        KEY_F | KEY_UPPER_F => ControlKey::Find,
        KEY_G | KEY_UPPER_G => ControlKey::FindNext {
            backwards: state & SHIFT != 0,
        },
        KEY_P | KEY_UPPER_P => ControlKey::Print,
        _ => ControlKey::Other,
    }
}

pub enum ScrollKey {
    Tab,
    To { x: f64, y: f64 },
    Unhandled,
}

pub struct ScrollFrame {
    pub x: f64,
    pub y: f64,
    pub page: f64,
    pub bottom: f64,
}

pub const LINE_PX: f64 = 60.0;

pub fn scroll_key(keyval: u32, frame: &ScrollFrame) -> ScrollKey {
    let (x, y) = (frame.x, frame.y);
    match keyval {
        KEY_TAB | KEY_ISO_LEFT_TAB => ScrollKey::Tab,
        KEY_DOWN => ScrollKey::To { x, y: y + LINE_PX },
        KEY_UP => ScrollKey::To { x, y: y - LINE_PX },
        KEY_RIGHT => ScrollKey::To { x: x + LINE_PX, y },
        KEY_LEFT => ScrollKey::To { x: x - LINE_PX, y },
        KEY_PAGE_DOWN | KEY_SPACE => ScrollKey::To {
            x,
            y: y + frame.page,
        },
        KEY_PAGE_UP => ScrollKey::To {
            x,
            y: y - frame.page,
        },
        KEY_HOME => ScrollKey::To { x, y: 0.0 },
        KEY_END => ScrollKey::To { x, y: frame.bottom },
        _ => ScrollKey::Unhandled,
    }
}
