use crate::logger::{Log, NoopLogger};
use crate::unsafe_box::UnsafeBox;
use crate::core::NodeInner;
use std::hash::Hash;
use std::mem;

/// A node in the tree.
pub struct Node<K, V, Logger = NoopLogger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    inner: UnsafeNode<K, V, Logger>,
}

pub(crate) type UnsafeNode<K, V, Logger> = UnsafeBox<NodeInner<K, V, Logger>>;

impl<K, V, Logger> Node<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    /// Returns a reference to the inner unsafe node.
    pub(crate) fn as_unsafe_node(&self) -> &UnsafeNode<K, V, Logger> {
        &self.inner
    }

    /// Returns a mutable reference to the inner unsafe node.
    pub(crate) fn as_unsafe_node_mut(&mut self) -> &mut UnsafeNode<K, V, Logger> {
        &mut self.inner
    }

    /// Consumes this node and converts it to its inner unsafe node.
    pub(crate) fn into_unsafe_node(self) -> UnsafeNode<K, V, Logger> {
        let inner = unsafe { UnsafeNode::clone(&self.inner) };
        mem::forget(self);
        inner
    }

    /// Unsafely creates a node from an unsafe node.
    ///
    /// # Safety
    ///
    /// The unsafe node should still be valid.
    pub(crate) unsafe fn from_unsafe_node(inner: UnsafeNode<K, V, Logger>) -> Self {
        Self { inner }
    }
}
