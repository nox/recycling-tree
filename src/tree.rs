use crate::node::{Node, UnsafeNode};
use std::hash::Hash;

/// A tree of values.
///
/// Children of nodes aren't immediately dropped when their refcount reaches 0,
/// instead they are put on a free list owned by the tree itself, which will
/// only be emptied if the tree is dropped or `Tree::gc` is called.
pub struct Tree<K, V>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
{
    root: Node<K, V>,
}

impl<K, V> Tree<K, V>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
{
    /// Returns a reference to the root node of the tree.
    pub fn root(&self) -> &Node<K, V> {
        &self.root
    }

    /// Creates a new tree from a root node.
    ///
    /// # Safety
    ///
    /// The node should be a root.
    pub(crate) unsafe fn from_root_node(root: Node<K, V>) -> Self {
        debug_assert!(root.as_unsafe_node().root().is_none());
        Self { root }
    }

    /// Returns a reference to the inner unsafe node.
    pub(crate) fn as_unsafe_node(&self) -> &UnsafeNode<K, V> {
        self.root.as_unsafe_node()
    }
}
