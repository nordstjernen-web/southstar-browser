# The DOM's ownership model

`rust/dom` (`southstar-dom`) holds the document tree every other crate reads
and edits. This page describes who owns a node, its strings and its
attributes, how long a handle to it stays good, and what happens when it is
freed.

## Nodes live in a store

Every node is a slot in the node store (`rust/dom/src/store.rs`), and every
attribute a slot in the attribute store beside it. Each thread has its own
stores: slots are allocated in chunks of 256 that are never handed back to
the allocator, and a freed slot goes on the thread's vacant list to be reused
by the next node or attribute. When a thread exits, its vacant slots go to a
process-wide spare list for the next thread that needs one, so the
image-decoding threads that parse SVG documents do not leak their slots.
Parsing allocates nothing per node or attribute beyond the strings that do
not fit inline.

Because a slot is never deallocated, a slot reference is good for the whole
process, so the node handle is a plain Rust reference:

- `Node<'a>` wraps `&'static NodeSlot`. It is `Copy`, compares and hashes by
  address, and is neither `Send` nor `Sync` (the slot is full of `Cell`s), so
  the compiler keeps every node on the thread whose store holds it.
- The tree links (`parent`, `first_child`, `last_child`, `prev_sibling`,
  `next_sibling`, the template contents) are `Cell<Option<&'static
  NodeSlot>>`, and the kind, flags, bloom bits and source position are
  `Cell`s too. Walking and relinking the tree is safe code; the store's
  spare list and the string accessors are the only `unsafe` in the crate
  apart from the C ABI.

## Generations and weak handles

Each slot carries a generation count that goes up when the slot is occupied
and again when it is vacated, so a live slot's generation is odd.

- `Node::downgrade()` gives a `WeakNode`: the slot and the generation it had.
  `WeakNode::get()` returns the node while it is the same node, and `None`
  once it has been freed, even if the slot already holds another node.
- Anything that remembers a node beyond the current call keeps a `WeakNode`,
  not a `Node` or a raw pointer: the stylesheet matcher's hover, active,
  focus and fullscreen elements, and the active modal dialog. A freed node
  simply stops matching; nothing has to be told that it went away.
- `Node::from_ptr`, the way back from the `*const NsNode` pointers of the
  remaining C ABI, returns `None` for a vacant slot.

## A node owns its data

A node owns everything it points to except its tree links. There is no
"borrowed" string, no per-document string arena and no free callback:

- Its **name** is a `Name`: one of about 220 common element and attribute
  names (a static table searched by binary search), or an owned `Box<CStr>`.
  Most names cost no allocation.
- Its **text** is an owned, NUL-terminated byte string, kept inline when
  shorter than 24 bytes; text may hold NUL bytes, which `text_with_len()`
  returns and `text()` stops at.
- Its **attributes** are a `Vec` of attribute slots, each owning its name,
  value (inline when short) and, for a namespaced attribute, its namespace,
  prefix and local name. `Attr` is a plain reference to its slot, so it
  stays put while the list grows; removing the attribute returns the slot to
  the store.
- Its **indexes** (a document's id, class and tag indexes) are `HashMap`s in
  a `RefCell`, and its **class tokens** are an owned list rebuilt when the
  class attribute changes.

Moving a node from one document to another, as `innerHTML`, `DOMParser`,
`document.write` and frame loading all do, cannot leave it pointing into the
document it came from.

A string or attribute borrowed from a node (`name()`, `text()`, `attr()`,
`Attr::value()`) stays good until that same field of that node is replaced
or the node is freed: don't hold one across a call that edits the node.

## Owning a subtree

A parent owns its children, and a template owns its contents. A subtree with
no parent is owned by exactly one `Tree` value (`rust/dom/src/owned.rs`):

- `Tree::new(kind)` makes a new parentless node, and `Tree::adopt(node)`
  detaches a node and takes ownership of it.
- Dropping a `Tree` frees its subtree, unless the root was given a parent in
  the meantime, in which case the parent owns it now; a `Tree` can never
  free a node that is still in someone else's tree.
- `Tree::into_root()` gives ownership away explicitly: to a parent, to a
  template's contents or to the script runtime's orphan set, which keeps a
  detached subtree alive while script can still reach it.
  `into_raw`/`from_raw` carry ownership across the remaining C ABI.
- The HTML parser returns documents and fragments as `Tree`s
  (`southstar_html_parser::parse_document`, `parse_fragment`,
  `parse_fragment_for_tag`), `Node::take_tpl_content` hands over a
  template's contents, and `node::clone` returns the copy as a `Tree`.

So a fragment parsed for `innerHTML` or `document.write`, a frame document
that fails to load, the page document in `rust/browser`, the headless
engine's and the SVG renderer's documents are freed by going out of scope
instead of by a `free` call that every early return must remember.

## Freeing

`Node::free_tree` unlinks the node from its parent and vacates it and its
whole subtree, template contents included. Before a slot is vacated, its
JavaScript wrapper is told through the node's invalidator, a plain
`fn(Node<'static>)` the script runtime installs on the nodes it wraps. After
that, the slot is empty: weak handles to it return `None` and its old
pointer no longer resolves.

## The C ABI

`NsNode` and `NsAttr` are opaque `#[repr(C)]` types used only as the pointee
of the `*const NsNode` and `*const NsAttr` pointers the remaining `ns_*`
exports pass around; `Node::as_ptr` and `Node::from_ptr` convert. Crates move
off those exports onto the Rust API as they are rewritten.

## Tests

`rust/dom/src/tests.rs` covers the store (weak handles after a free and after
slot reuse, other threads), the links, owned names, text and attributes,
the indexes and cloning; `scripts/rust-tests.sh` runs them.
