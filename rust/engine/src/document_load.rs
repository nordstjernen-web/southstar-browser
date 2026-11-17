//! Southstar — parsing a navigation's HTML while its body downloads: the chunks of a UTF-8 text/html response go to the streaming parser, and the tree is handed over only when it covers exactly the final body.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Tree;
use southstar_html_parser::DocumentStream;
use southstar_html_util::{declares_charset_other_than_utf8, is_plain_html_type};
use southstar_net::{Chunk, Response};

const PRESCAN: usize = 1024;
const NODE_SCRIPTING_DISABLED: u32 = 1 << 15;

enum State {
    Idle,
    Undecided(Vec<u8>),
    Streaming(Box<DocumentStream>),
    Off,
}

pub struct DocumentLoad {
    state: State,
    scripting: bool,
    content_type: Option<Vec<u8>>,
    carry: Vec<u8>,
    seen: usize,
}

pub struct LoadedDocument {
    tree: Tree,
    bytes: usize,
}

impl LoadedDocument {
    pub fn take_for(
        self,
        body_len: usize,
        content_type: Option<&[u8]>,
        scripting: bool,
    ) -> Option<Tree> {
        let parsed_with_scripting = self.tree.root().flags() & NODE_SCRIPTING_DISABLED == 0;
        (self.bytes == body_len
            && content_type.is_some_and(is_plain_html_type)
            && parsed_with_scripting == scripting)
            .then_some(self.tree)
    }
}

pub fn enabled() -> bool {
    std::env::var_os("NS_HTML_STREAM_LOAD").is_none_or(|value| value != "0")
}

impl DocumentLoad {
    pub fn new(scripting: bool) -> DocumentLoad {
        DocumentLoad {
            state: State::Idle,
            scripting,
            content_type: None,
            carry: Vec::new(),
            seen: 0,
        }
    }

    fn restart(&mut self, content_type: Option<Vec<u8>>) {
        self.state = if content_type.as_deref().is_some_and(is_plain_html_type) {
            State::Undecided(Vec::new())
        } else {
            State::Off
        };
        self.content_type = content_type;
        self.carry.clear();
        self.seen = 0;
    }

    fn feed(&mut self, data: &[u8]) {
        let State::Streaming(stream) = &mut self.state else {
            return;
        };
        let joined;
        let data = if self.carry.is_empty() {
            data
        } else {
            joined = [&self.carry[..], data].concat();
            self.carry.clear();
            &joined[..]
        };
        let valid = match core::str::from_utf8(data) {
            Ok(_) => data.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => {
                self.state = State::Off;
                return;
            }
        };
        stream.feed(&data[..valid]);
        self.carry.extend_from_slice(&data[valid..]);
    }

    fn decide(&mut self, head: &[u8]) {
        let utf8_bom = head.starts_with(b"\xef\xbb\xbf");
        let utf16_bom = head.starts_with(b"\xff\xfe") || head.starts_with(b"\xfe\xff");
        let content_type = self.content_type.as_deref().unwrap_or_default();
        if utf16_bom || (!utf8_bom && declares_charset_other_than_utf8(content_type, head)) {
            self.state = State::Off;
            return;
        }
        self.state = State::Streaming(Box::new(DocumentStream::new(self.scripting)));
        self.feed(&head[if utf8_bom { 3 } else { 0 }..]);
    }

    pub fn chunk(&mut self, chunk: &Chunk) {
        if chunk.first {
            self.restart(chunk.content_type.clone());
        }
        self.seen += chunk.data.len();
        match &mut self.state {
            State::Undecided(head) => {
                head.extend_from_slice(&chunk.data);
                if head.len() >= PRESCAN {
                    let head = core::mem::take(head);
                    self.decide(&head);
                }
            }
            State::Streaming(_) => self.feed(&chunk.data),
            State::Idle | State::Off => {}
        }
    }

    pub fn finish(mut self, response: &Response) -> Option<LoadedDocument> {
        let body_len = response.body().map_or(0, <[u8]>::len);
        let content_type = response.content_type().map(|ct| ct.to_bytes());
        let whole = response.error().is_none()
            && (200..300).contains(&response.status())
            && self.seen == body_len
            && content_type.is_some_and(is_plain_html_type)
            && content_type == self.content_type.as_deref();
        if !whole {
            return None;
        }
        if let State::Undecided(head) = &mut self.state {
            let head = core::mem::take(head);
            self.decide(&head);
        }
        match self.state {
            State::Streaming(stream) if self.carry.is_empty() => Some(LoadedDocument {
                tree: stream.finish(),
                bytes: body_len,
            }),
            _ => None,
        }
    }
}
