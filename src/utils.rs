use std::sync::{Mutex, MutexGuard};

pub trait MutexUtils<T> {
    /// Lock a mutex and execute a function with its lock.
    /// This makes sure that the mutex is locked only during the function execution.
    fn with_lock<R, F>(&self, f: F) -> R
    where
        F: FnOnce(MutexGuard<'_, T>) -> R;
}

impl<T> MutexUtils<T> for Mutex<T> {
    fn with_lock<R, F>(&self, f: F) -> R
    where
        F: FnOnce(MutexGuard<'_, T>) -> R,
    {
        f(self.lock().unwrap())
    }
}
