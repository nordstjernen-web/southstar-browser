//! Southstar — `Tree`, the one owner of a subtree that has no parent: dropping it frees the subtree unless it was given a parent first.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::fmt;
use core::mem::ManuallyDrop;

use crate::{Kind, Node, NsNode, WeakNode};

pub struct Tree {
    root: WeakNode,
}

impl Tree {
    pub fn new(kind: Kind) -> Tree {
        Tree::adopt(Node::alloc(kind))
    }

    pub fn adopt(root: Node<'_>) -> Tree {
        root.detach();
        Tree {
            root: root.downgrade(),
        }
    }

    pub fn root(&self) -> Node<'static> {
        self.root.node()
    }

    pub fn into_root(self) -> Node<'static> {
        ManuallyDrop::new(self).root()
    }

    pub fn into_raw(self) -> *mut NsNode {
        self.into_root().as_mut_ptr()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        if let Some(root) = self.root.get()
            && root.parent().is_none()
        {
            root.free_tree();
        }
    }
}

impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Tree").field(&self.root.get()).finish()
    }
}
