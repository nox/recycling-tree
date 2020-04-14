use crate::ancestors::Ancestors;
use crate::logger::Log;
use crate::map::Map;
use crate::node::{Node, UnsafeNode};
use crate::tree::Tree;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use std::ffi::c_void;
use std::hash::Hash;
use std::marker::PhantomData as marker;
use std::ptr::{self, NonNull};
use std::sync::atomic::{self, AtomicPtr, AtomicUsize, Ordering};

/// The inner contents of a node.
pub(crate) struct NodeInner<K, V, Logger> {
    value: V,
    marker: marker<(K, Logger)>,
    ancestors: Option<Ancestors<K, V, Logger>>,
    /// The children of this node. Children remove themselves from this map
    /// on drop either after the free list is gone or when the tree is GC'd.
    children: RwLock<Map<K, UnsafeNode<K, V, Logger>>>,
    /// The reference counter of this node. Starts at 1, is reincremented back
    /// to 1 after reaching 0 if it is put on the free list.
    refcount: AtomicUsize,
    /// This field has two different meanings depending on whether this node
    /// is the root of the tree or not.
    ///
    /// In the case of the root node, it can be:
    ///  * null if the last GC is ongoing or has completed already;
    ///  * `NodeInner::DANGLING_PTR` if the free list is empty;
    ///  * a pointer with its lowest bit set if the free list is locked;
    ///  * or just a plain old aligned pointer to the free list's first item.
    ///
    /// In the case of a non-root node, it can be:
    ///  * null if the node is not on the free list yet;
    ///  * `NodeInner::DANGLING_PTR` if it is the last item in the free list;
    ///  * or just a plain old aligned pointer to the free list's next item.
    ///
    /// Starts as `NodeInner::DANGLING_PTR` for root nodes and the null pointer
    /// for non-root nodes.
    next_free: AtomicPtr<NodeInner<K, V, Logger>>,
    /// The length of the free list. Only used on root nodes.
    free_count: AtomicUsize,
}

/// The threshold over which `Tree::maybe_gc` will trigger a GC. Nobody knows
/// why it is this value, not even Gecko people.
const GC_COUNT_THRESHOLD: usize = 300;

impl<K, V> Tree<K, V>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
{
    /// Creates a new tree from a root value.
    ///
    /// Note that the root value is never going to be accessed by either the
    /// crate or the caller.
    pub fn new(root: V) -> Self {
        Self::with_logger(root)
    }
}

impl<K, V, Logger> Tree<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    /// Creates a new tree from a root value.
    ///
    /// Note that the root value is never going to be accessed by either the
    /// crate or the caller.
    pub fn with_logger(root: V) -> Self {
        let tree = unsafe {
            Tree::from_root_node(Node::from_unsafe_node(UnsafeNode::new(NodeInner {
                value: root,
                marker,
                ancestors: None,
                children: Default::default(),
                refcount: AtomicUsize::new(1),
                next_free: AtomicPtr::new(NodeInner::DANGLING_PTR),
                free_count: Default::default(),
            })))
        };
        Logger::log_new(&**tree.as_unsafe_node() as *const NodeInner<K, V, Logger> as *const c_void);
        tree
    }
}

impl<K, V, Logger> Tree<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    /// Runs the garbage collector of the tree's free list if needed according
    /// to some heuristics.
    pub fn maybe_gc(&self) {
        if self.as_unsafe_node().free_count.load(Ordering::Relaxed) > GC_COUNT_THRESHOLD {
            unsafe { self.swap_free_list_and_gc(NodeInner::DANGLING_PTR) }
        }
    }

    /// Runs the garbage collector of the tree's free list.
    pub fn gc(&self) {
        unsafe { self.swap_free_list_and_gc(NodeInner::DANGLING_PTR) }
    }

    /// Swaps the tree's free list's head with a given pointer and collects
    /// the free list, taking care of not swapping any pointer with its lowest
    /// bit set, given that would break the lock currently held by another
    /// thread.
    unsafe fn swap_free_list_and_gc(&self, ptr: *mut NodeInner<K, V, Logger>) {
        let root = self.as_unsafe_node();
        let mut head = root.next_free.load(Ordering::Relaxed);
        loop {
            // This is only ever called when the tree is dropped or when
            // a GC is manually requested, so the free list's head should
            // never be null already.
            debug_assert!(!head.is_null());
            if head == ptr {
                // In the case of swapping the pointer with
                // `NodeInner::DANGLING_PTR`, we can return immediately
                // because the free list is already empty so there is nothing
                // to GC.
                return;
            }
            // Unmask the lock bit from the current head, this is the most
            // probable value `compare_exchange_weak` will read when the other
            // thread currently locking the free list unlocks it.
            head = (head as usize & !1) as *mut NodeInner<K, V, Logger>;
            // This could fail if the free list head is
            // `NodeInner::DANGLING_PTR` with the lowest bit set, which
            // makes no sense.
            debug_assert!(head != ptr);
            match root.next_free.compare_exchange_weak(
                head,
                ptr,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(old_head) => {
                    head = old_head;
                    break;
                }
                Err(current_head) => head = current_head,
            }
        }
        loop {
            if head == NodeInner::DANGLING_PTR {
                // We reached the end of the free list.
                return;
            }
            let node = UnsafeNode::from_raw(head);
            let next = node.next_free.swap(ptr::null_mut(), Ordering::Relaxed);
            // This fails if we found a node on the free list with a next
            // free pointer that got its lowest bit set, that makes no sense.
            debug_assert!(head as usize & 1 == 0);
            // It wouldn't make sense for a node on the free list to have
            // a null next free pointer.
            debug_assert!(!head.is_null());
            drop(Node::from_unsafe_node(node));
            // Iterates on the next item in the free list.
            head = next;
        }
    }
}

impl<K, V, Logger> Drop for Tree<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    fn drop(&mut self) {
        unsafe { self.swap_free_list_and_gc(ptr::null_mut()) }
    }
}

impl<K, V, Logger> Node<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    /// Ensures that a child exists in this node with the given value.
    ///
    /// If a child with this value was already created since last GC happened,
    /// that child is returned instead.
    pub fn ensure_child(&self, value: V) -> Node<K, V, Logger> {
        let this = self.as_unsafe_node();
        let children = this.children.upgradable_read();
        let key = (&value).into();
        if let Some(child) = children.get(&key, |node| node.key()) {
            if child.refcount.fetch_add(1, Ordering::Relaxed) == 0 {
                // Consider a node with a refcount of 1 being dropped on a different
                // thread A while not already on the free list. A decrements the
                // refcount to 0 while we increment it back to 1 for the upgrade.
                //
                // Now, if the newly upgraded node value is dropped, its refcount
                // again reaches 0 again and the node is put on the free list.
                //
                // If a GC is then triggered before the destructor on thread A
                // finishes executing, we have a use after free vulnerability.
                //
                // To avoid that, we try to push the child on the free list
                // ourselves.
                unsafe {
                    UnsafeNode::push_on_free_list(child);
                }
            }
            return unsafe { Node::from_unsafe_node(UnsafeNode::clone(child)) };
        }
        let mut children = RwLockUpgradableReadGuard::upgrade(children);
        let unsafe_node = children.get_or_insert_with(
            key,
            |node| node.key(),
            || {
                let root = unsafe { UnsafeNode::clone(this.root().unwrap()) };
                UnsafeNode::new(NodeInner {
                    value,
                    marker,
                    ancestors: Some(unsafe { Ancestors::new(root, self.clone()) }),
                    children: Default::default(),
                    refcount: AtomicUsize::new(1),
                    next_free: Default::default(),
                    free_count: Default::default(),
                })
            },
        );
        let node = unsafe { Node::from_unsafe_node(UnsafeNode::clone(unsafe_node)) };
        Logger::log_new(&**node.as_unsafe_node() as *const NodeInner<K, V, Logger> as *const c_void);
        node
    }
}

impl<K, V, Logger> Clone for Node<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    #[inline]
    fn clone(&self) -> Self {
        let this = self.as_unsafe_node();
        this.refcount.fetch_add(1, Ordering::Relaxed);
        unsafe { Node::from_unsafe_node(UnsafeNode::clone(this)) }
    }
}

impl<K, V, Logger> UnsafeNode<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    unsafe fn drop_without_free_list(this: &mut Self) {
        let mut this = UnsafeNode::clone(this);
        loop {
            if let Some(parent) = this.ancestors.as_ref().map(Ancestors::parent) {
                let mut children = parent.children.write();
                // Another thread may have resurrected this node while
                // we were trying to drop it, leave it alone. The
                // operation can be relaxed because we are currently
                // write-locking its parent children list and
                // resurrection is only done from `Node::ensure_child`
                // which read-locks that same children list.
                if this.refcount.load(Ordering::Relaxed) != 0 {
                    return;
                }
                this.next_free.store(ptr::null_mut(), Ordering::Relaxed);
                children.remove(&this.key(), |node| node.key());
            }
            atomic::fence(Ordering::Acquire);
            debug_assert_eq!(this.refcount.load(Ordering::Relaxed), 0);
            debug_assert!(this.next_free.load(Ordering::Relaxed).is_null());
            // Remove the parent reference from the child to avoid
            // recursively dropping it.
            let parent = {
                UnsafeNode::deref_mut(&mut this)
                    .ancestors
                    .take()
                    .map(Ancestors::into_parent)
            };
            Logger::log_drop(&*this as *const NodeInner<K, V, Logger> as *const c_void);
            UnsafeNode::drop(&mut this);
            if let Some(parent) = parent {
                this = parent.into_unsafe_node();
                if this.refcount.fetch_sub(1, Ordering::Release) == 1 {
                    // The node had a parent and its refcount reached
                    // zero, we reiterate the loop to drop it too.
                    continue;
                }
            }
            // The node didn't have a parent or the parent has other
            // live reference elsewhere, we don't have anything to do
            // anymore.
            return;
        }
    }

    /// Pushes this node on the tree's free list. Returns false if the free list
    /// is gone.
    unsafe fn push_on_free_list(this: &Self) -> bool {
        let root = this.root().unwrap();
        let mut old_head = root.next_free.load(Ordering::Relaxed);
        let this_ptr = &**this as *const NodeInner<K, V, Logger> as *mut NodeInner<K, V, Logger>;
        let this_lock = (this_ptr as usize | 1) as *mut NodeInner<K, V, Logger>;
        loop {
            if old_head.is_null() {
                // Tree was dropped and free list has been destroyed.
                return false;
            }
            // Unmask the lock bit from the current head, this is the most
            // probable value `compare_exchange_weak` will read when the other
            // thread currently locking the free list unlocks it.
            old_head = (old_head as usize & !1) as *mut NodeInner<K, V, Logger>;
            if old_head == this_ptr {
                // The free list is currently locked or has finished being
                // locked to put this very node at the head of the free list,
                // which means we don't need to do it ourselves.
                return true;
            }
            match root.next_free.compare_exchange_weak(
                old_head,
                this_lock,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current_head) => old_head = current_head,
            }
        }
        if !this.next_free.load(Ordering::Relaxed).is_null() {
            // Another thread managed to resurrect this node and put it on
            // the free list while we were still busy trying to lock the free
            // list. That means we are done and we just need to unlock the free
            // list with whatever head we read last.
            root.next_free.store(old_head, Ordering::Release);
        } else {
            // We increment the refcount of this node to account for its presence
            // in the tree's free list.
            this.refcount.fetch_add(1, Ordering::Relaxed);

            // The free count is only ever written to when the free list is
            // locked so we don't need an atomic increment here.
            let old_free_count = root.free_count.load(Ordering::Relaxed);
            root.free_count.store(old_free_count + 1, Ordering::Relaxed);

            // Finally, we store the old free list head into this node's next free
            // slot and we unlock the guard with the new head.
            this.next_free.store(old_head, Ordering::Relaxed);
            root.next_free.store(this_ptr, Ordering::Release);
        }
        true
    }
}

impl<K, V, Logger> Drop for Node<K, V, Logger>
where
    K: Eq + Hash,
    for<'a> &'a V: Into<K>,
    Logger: Log,
{
    fn drop(&mut self) {
        let this = self.as_unsafe_node();
        if this.refcount.fetch_sub(1, Ordering::Release) != 1 {
            // This wasn't the last reference to this node, nothing to do
            // anymore.
            return;
        }

        unsafe {
            if this.root().is_none() || !UnsafeNode::push_on_free_list(this) {
                UnsafeNode::drop_without_free_list(self.as_unsafe_node_mut())
            }
        }
    }
}

impl<K, V, Logger> NodeInner<K, V, Logger>
where
    for<'a> &'a V: Into<K>,
{
    pub(crate) fn key(&self) -> K {
        (&self.value).into()
    }
}

impl<K, V, Logger> NodeInner<K, V, Logger> {
    const DANGLING_PTR: *mut NodeInner<K, V, Logger> = NonNull::dangling().as_ptr();

    pub(crate) fn root(&self) -> Option<&UnsafeNode<K, V, Logger>> {
        self.ancestors.as_ref().map(Ancestors::root)
    }
}
