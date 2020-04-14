use crate::node::{Node, UnsafeNode};
use std::hash::Hash;
use std::mem;

/// Represents ancestors of a non-root node, i.e. its root and its parent.
///
/// This type exists solely to panic on drop, to nudge us into being careful
/// when dropping nodes recursively.
pub(crate) struct Ancestors<K, V> {
    root: UnsafeNode<K, V>,
    /// This should be a `Node<K, V>` but then we need the same bounds on
    /// `Self` than on `Node<K, V>`, and then the bounds need to be propagated
    /// to `NodeInner<K, V>`. Instead we store an `UnsafeNode<K, V>` and
    /// rely on the fact that we panic anyway in this type's destructor.
    parent: UnsafeNode<K, V>,
}

impl<K, V> Ancestors<K, V>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
{
    /// Creates a new value from a root and a parent.
    ///
    /// # Safety
    ///
    /// The parent should indeed be a node in the tree dominated by the
    /// given root.
    pub(crate) unsafe fn new(root: UnsafeNode<K, V>, parent: Node<K, V>) -> Self {
        Self { root, parent: parent.into_unsafe_node() }
    }

    /// Converts this value into the parent node, consuming it and avoiding
    /// the drop on panic.
    pub(crate) fn into_parent(self) -> Node<K, V> {
        let parent = unsafe { Node::from_unsafe_node(UnsafeNode::clone(&self.parent)) };
        mem::forget(self);
        parent
    }
}

impl<K, V> Ancestors<K, V> {
    /// Returns a reference to the root.
    pub(crate) fn root(&self) -> &UnsafeNode<K, V> {
        &self.root
    }

    /// Returns a reference to the parent.
    pub(crate) fn parent(&self) -> &UnsafeNode<K, V> {
        &self.parent
    }
}

#[cfg(debug_assertions)]
impl<K, V> Drop for Ancestors<K, V> {
    fn drop(&mut self) {
        panic!("values of this type should never be dropped, only consumed through Ancestors::into_parent");
    }
}
