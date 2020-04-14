use std::ffi::c_void;

/// A trait to log creation and destruction of nodes in a tree.
pub trait Log {
    /// Logs the creation of a new node.
    fn log_new(ptr: *const c_void);

    /// Logs the destruction of a node.
    fn log_drop(ptr: *const c_void);
}

/// A logger that doesn't actually log anything.
pub struct NoopLogger;

impl Log for NoopLogger {
    fn log_new(_ptr: *const c_void) {}
    fn log_drop(_ptr: *const c_void) {}
}
