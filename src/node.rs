use crate::unsafe_box::UnsafeBox;
use crate::core::NodeInner;
use std::hash::Hash;
use std::mem;

/// A node in the tree.
pub struct Node<K, V>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
{
    inner: UnsafeNode<K, V>,
}

pub(crate) type UnsafeNode<K, V> = UnsafeBox<NodeInner<K, V>>;

impl<K, V> Node<K, V>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
{
    /// Returns a reference to the inner unsafe node.
    pub(crate) fn as_unsafe_node(&self) -> &UnsafeNode<K, V> {
        &self.inner
    }

    /// Returns a mutable reference to the inner unsafe node.
    pub(crate) fn as_unsafe_node_mut(&mut self) -> &mut UnsafeNode<K, V> {
        &mut self.inner
    }

    /// Consumes this node and converts it to its inner unsafe node.
    pub(crate) fn into_unsafe_node(self) -> UnsafeNode<K, V> {
        let inner = unsafe { UnsafeNode::clone(&self.inner) };
        mem::forget(self);
        inner
    }

    /// Unsafely creates a node from an unsafe node.
    ///
    /// # Safety
    ///
    /// The unsafe node should still be valid.
    pub(crate) unsafe fn from_unsafe_node(inner: UnsafeNode<K, V>) -> Self {
        Self { inner }
    }
}
