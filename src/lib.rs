extern crate alloc;

mod ancestors;
mod map;
mod node;
mod tree;
mod unsafe_box;

// Comes last so that rustdoc doesn't show `Tree::gc` before `Tree::new` etc.
mod core;

pub use self::node::Node;
pub use self::tree::Tree;
