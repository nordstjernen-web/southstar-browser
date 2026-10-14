//! Southstar — unit tests for the tokenizer and tree builder, comparing html5lib-style tree dumps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{
    Attrs, Context, ElementName, Ns, Position, QuirksMode, TreeSink, parse_document, parse_fragment,
};

enum Data {
    Document,
    Element {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Text(String),
    Comment(String),
    Doctype(String),
    Fragment,
}

struct Node {
    data: Data,
    parent: Option<usize>,
    children: Vec<usize>,
    contents: Option<usize>,
}

struct Tree {
    nodes: Vec<Node>,
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Tree {
    fn new() -> Tree {
        Tree {
            nodes: vec![Node {
                data: Data::Document,
                parent: None,
                children: Vec::new(),
                contents: None,
            }],
        }
    }

    fn add(&mut self, data: Data) -> usize {
        self.nodes.push(Node {
            data,
            parent: None,
            children: Vec::new(),
            contents: None,
        });
        self.nodes.len() - 1
    }

    fn detach(&mut self, node: usize) {
        if let Some(parent) = self.nodes[node].parent.take() {
            self.nodes[parent].children.retain(|&c| c != node);
        }
    }

    fn adjacent_text(&mut self, parent: usize, index: usize, more: &[u8]) -> bool {
        let Some(&previous) = self.nodes[parent].children.get(index) else {
            return false;
        };
        if let Data::Text(existing) = &mut self.nodes[previous].data {
            existing.push_str(&text(more));
            return true;
        }
        false
    }

    fn dump(&self) -> String {
        let mut out = String::new();
        for &child in &self.nodes[0].children {
            self.dump_node(child, 0, &mut out);
        }
        out
    }

    fn dump_node(&self, node: usize, depth: usize, out: &mut String) {
        let pad = "  ".repeat(depth);
        match &self.nodes[node].data {
            Data::Element { name, attrs } => {
                out.push_str(&format!("| {pad}<{name}>\n"));
                let mut attrs = attrs.clone();
                attrs.sort();
                for (n, v) in attrs {
                    out.push_str(&format!("| {pad}  {n}=\"{v}\"\n"));
                }
            }
            Data::Text(t) => out.push_str(&format!("| {pad}\"{t}\"\n")),
            Data::Comment(c) => out.push_str(&format!("| {pad}<!-- {c} -->\n")),
            Data::Doctype(d) => out.push_str(&format!("| {pad}<!DOCTYPE {d}>\n")),
            Data::Document | Data::Fragment => {}
        }
        if let Some(contents) = self.nodes[node].contents {
            out.push_str(&format!("| {pad}  content\n"));
            for &child in &self.nodes[contents].children {
                self.dump_node(child, depth + 2, out);
            }
        }
        for &child in &self.nodes[node].children {
            self.dump_node(child, depth + 1, out);
        }
    }
}

impl TreeSink for Tree {
    type Handle = usize;

    fn document(&self) -> usize {
        0
    }

    fn create_element(&mut self, name: ElementName<'_>, attrs: Attrs<'_>, _: Position) -> usize {
        let prefix = match name.ns {
            Ns::Html => "",
            Ns::Svg => "svg ",
            Ns::MathMl => "math ",
        };
        let attrs = attrs.map(|a| (text(a.name), text(a.value))).collect();
        self.add(Data::Element {
            name: format!("{prefix}{}", text(name.local.bytes())),
            attrs,
        })
    }

    fn create_comment(&mut self, t: &[u8]) -> usize {
        self.add(Data::Comment(text(t)))
    }

    fn create_processing_instruction(&mut self, target: &[u8], data: &[u8]) -> usize {
        self.add(Data::Comment(format!("?{} {}", text(target), text(data))))
    }

    fn append_doctype(&mut self, name: &[u8], _: &[u8], _: &[u8]) {
        let doctype = self.add(Data::Doctype(text(name)));
        self.append(0, doctype);
    }

    fn append(&mut self, parent: usize, child: usize) {
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    fn insert_before(&mut self, sibling: usize, child: usize) {
        self.detach(child);
        let parent = self.nodes[sibling].parent.expect("sibling has a parent");
        let at = self.nodes[parent]
            .children
            .iter()
            .position(|&c| c == sibling)
            .expect("sibling");
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.insert(at, child);
    }

    fn append_text(&mut self, parent: usize, t: &[u8]) {
        let last = self.nodes[parent].children.len().wrapping_sub(1);
        if !self.adjacent_text(parent, last, t) {
            let node = self.add(Data::Text(text(t)));
            self.append(parent, node);
        }
    }

    fn insert_text_before(&mut self, sibling: usize, t: &[u8]) {
        let parent = self.nodes[sibling].parent.expect("sibling has a parent");
        let at = self.nodes[parent]
            .children
            .iter()
            .position(|&c| c == sibling)
            .expect("sibling");
        if at == 0 || !self.adjacent_text(parent, at - 1, t) {
            let node = self.add(Data::Text(text(t)));
            self.insert_before(sibling, node);
        }
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
        if let Some(contents) = self.nodes[template].contents {
            return contents;
        }
        let contents = self.add(Data::Fragment);
        self.nodes[template].contents = Some(contents);
        contents
    }

    fn add_missing_attributes(&mut self, element: usize, attrs: Attrs<'_>) {
        if let Data::Element {
            attrs: existing, ..
        } = &mut self.nodes[element].data
        {
            for attr in attrs {
                let name = text(attr.name);
                if !existing.iter().any(|(n, _)| *n == name) {
                    existing.push((name, text(attr.value)));
                }
            }
        }
    }
}

fn document(html: &str) -> (String, QuirksMode) {
    let mut tree = Tree::new();
    let quirks = parse_document(html.as_bytes(), true, &mut tree);
    (tree.dump(), quirks)
}

fn assert_tree(html: &str, expected: &str) {
    let (dump, _) = document(html);
    let expected: String = expected
        .lines()
        .map(|l| format!("{}\n", l.trim_start()))
        .collect();
    assert_eq!(dump, expected, "\n{html}");
}

#[test]
fn implies_html_head_and_body() {
    assert_tree(
        "<!DOCTYPE html><title>T</title><p>One<p>Two",
        "| <!DOCTYPE html>
         | <html>
         |   <head>
         |     <title>
         |       \"T\"
         |   <body>
         |     <p>
         |       \"One\"
         |     <p>
         |       \"Two\"",
    );
}

#[test]
fn decodes_character_references() {
    assert_tree(
        "<p title='&lt;&#x41;&amp'>&copy; &notit; &#0; &#x110000;",
        "| <html>
         |   <head>
         |   <body>
         |     <p>
         |       title=\"<A&\"
         |       \"© ¬it; \u{fffd} \u{fffd}\"",
    );
}

#[test]
fn runs_the_adoption_agency() {
    assert_tree(
        "<b>1<p>2</b>3</p>",
        "| <html>
         |   <head>
         |   <body>
         |     <b>
         |       \"1\"
         |     <p>
         |       <b>
         |         \"2\"
         |       \"3\"",
    );
}

#[test]
fn foster_parents_out_of_tables() {
    assert_tree(
        "<table>x<tr><td>y</td></tr></table>",
        "| <html>
         |   <head>
         |   <body>
         |     \"x\"
         |     <table>
         |       <tbody>
         |         <tr>
         |           <td>
         |             \"y\"",
    );
}

#[test]
fn keeps_template_contents_apart() {
    assert_tree(
        "<template><td>cell</td></template>",
        "| <html>
         |   <head>
         |     <template>
         |       content
         |         <td>
         |           \"cell\"
         |   <body>",
    );
}

#[test]
fn builds_foreign_content() {
    assert_tree(
        "<svg viewBox='0 0 1 1'><foreignObject><p>x</p></foreignObject></svg><math><mi>y</mi></math>",
        "| <html>
         |   <head>
         |   <body>
         |     <svg svg>
         |       viewBox=\"0 0 1 1\"
         |       <svg foreignObject>
         |         <p>
         |           \"x\"
         |     <math math>
         |       <math mi>
         |         \"y\"",
    );
}

#[test]
fn treats_script_and_rawtext_contents_as_text() {
    assert_tree(
        "<script>if (a < b) { x = '</p>'; }</script><textarea>\n<b>t</b></textarea>",
        "| <html>
         |   <head>
         |     <script>
         |       \"if (a < b) { x = '</p>'; }\"
         |   <body>
         |     <textarea>
         |       \"<b>t</b>\"",
    );
}

#[test]
fn selects_quirks_mode_from_the_doctype() {
    assert_eq!(document("<!DOCTYPE html><p>").1, QuirksMode::NoQuirks);
    assert_eq!(document("<p>no doctype").1, QuirksMode::Quirks);
    assert_eq!(
        document("<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01 Transitional//EN\" \"http://www.w3.org/TR/html4/loose.dtd\">").1,
        QuirksMode::LimitedQuirks
    );
}

#[test]
fn parses_fragments_in_context() {
    let mut tree = Tree::new();
    let context = Context {
        ns: Ns::Html,
        name: b"tr",
    };
    parse_fragment(context, b"<td>a<td>b", true, &mut tree);
    let dump = tree.dump();
    assert_eq!(dump.matches("<td>").count(), 2, "{dump}");
    assert!(
        dump.contains("\"a\"") && dump.contains("\"b\"") && !dump.contains("<tr>"),
        "{dump}"
    );
}

#[test]
fn survives_hostile_nesting() {
    let deep = "<div>".repeat(5000) + &"<b><i>".repeat(2000) + &"</div>".repeat(5000);
    let mut tree = Tree::new();
    parse_document(deep.as_bytes(), true, &mut tree);
    assert!(tree.nodes.len() > 5000);
}
