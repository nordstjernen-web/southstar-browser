//! Southstar — fuzz target: parses arbitrary bytes as an HTML document and as a fragment in several contexts into an arena tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_html5::{Attrs, Context, ElementName, Ns, Position, TreeSink};

#[derive(Default)]
struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    template_contents: Option<usize>,
}

struct Arena {
    nodes: Vec<Node>,
}

impl Arena {
    fn new() -> Arena {
        Arena {
            nodes: vec![Node::default()],
        }
    }

    fn create(&mut self) -> usize {
        self.nodes.push(Node::default());
        self.nodes.len() - 1
    }

    fn detach(&mut self, node: usize) {
        if let Some(parent) = self.nodes[node].parent.take() {
            self.nodes[parent].children.retain(|&c| c != node);
        }
    }
}

impl TreeSink for Arena {
    type Handle = usize;

    fn document(&self) -> usize {
        0
    }

    fn create_element(&mut self, name: ElementName<'_>, attrs: Attrs<'_>, _: Position) -> usize {
        let _ = name.local.bytes();
        for attr in attrs {
            let _ = (attr.prefix(), attr.local_name());
        }
        self.create()
    }

    fn create_comment(&mut self, _: &[u8]) -> usize {
        self.create()
    }

    fn create_processing_instruction(&mut self, _: &[u8], _: &[u8]) -> usize {
        self.create()
    }

    fn append_doctype(&mut self, _: &[u8], _: &[u8], _: &[u8]) {}

    fn append(&mut self, parent: usize, child: usize) {
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    fn insert_before(&mut self, sibling: usize, child: usize) {
        self.detach(child);
        let parent = self.nodes[sibling]
            .parent
            .expect("insert_before a node without a parent");
        let at = self.nodes[parent]
            .children
            .iter()
            .position(|&c| c == sibling)
            .expect("sibling");
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.insert(at, child);
    }

    fn append_text(&mut self, parent: usize, _: &[u8]) {
        let text = self.create();
        self.append(parent, text);
    }

    fn insert_text_before(&mut self, sibling: usize, _: &[u8]) {
        let text = self.create();
        self.insert_before(sibling, text);
    }

    fn parent(&self, node: usize) -> Option<usize> {
        self.nodes[node].parent
    }

    fn remove(&mut self, node: usize) {
        self.detach(node);
    }

    fn reparent_children(&mut self, from: usize, to: usize) {
        for child in std::mem::take(&mut self.nodes[from].children) {
            self.nodes[child].parent = Some(to);
            self.nodes[to].children.push(child);
        }
    }

    fn template_contents(&mut self, template: usize) -> usize {
        if let Some(contents) = self.nodes[template].template_contents {
            return contents;
        }
        let contents = self.create();
        self.nodes[template].template_contents = Some(contents);
        contents
    }

    fn add_missing_attributes(&mut self, _: usize, attrs: Attrs<'_>) {
        for attr in attrs {
            let _ = attr.local_name();
        }
    }
}

const CONTEXTS: [(Ns, &[u8]); 8] = [
    (Ns::Html, b"body"),
    (Ns::Html, b"table"),
    (Ns::Html, b"tr"),
    (Ns::Html, b"select"),
    (Ns::Html, b"template"),
    (Ns::Html, b"textarea"),
    (Ns::Svg, b"svg"),
    (Ns::MathMl, b"math"),
];

fuzz_target!(|data: &[u8]| {
    let scripting = data.first().is_some_and(|b| b & 1 == 1);
    southstar_html5::parse_document(data, scripting, &mut Arena::new());
    let (ns, name) = CONTEXTS[data
        .first()
        .map_or(0, |b| usize::from(b >> 1) % CONTEXTS.len())];
    southstar_html5::parse_fragment(Context { ns, name }, data, scripting, &mut Arena::new());
});
