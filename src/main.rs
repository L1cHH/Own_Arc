use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

fn main() {
    let mut alloc = Box::new(42);

    let non_null_pointer = NonNull::new(alloc.deref_mut() as *mut i32).unwrap();

    unsafe {
        println!("Value: {}", *non_null_pointer.as_ptr())
    }

    drop(alloc);

    unsafe {
        println!("Value: {}", *non_null_pointer.as_ptr())
    }
}

struct ArcData<T> {
    ref_count: AtomicUsize,
    data: T
}

pub struct Arc<T> {
    ptr: NonNull<ArcData<T>>
}

unsafe impl<T: Send + Sync> Sync for Arc<T> {}
unsafe impl<T: Send + Sync> Send for Arc<T> {}
impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data().data
    }
}

impl <T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        if self.data().ref_count.fetch_add(1, Relaxed) > usize::Max / 2 {
            std::process::abort();
        };

        Arc {
            ptr: self.ptr
        }
    }
}

impl <T> Drop for Arc<T> {
    fn drop(&mut self) {
        // TODO memory ordering
        if self.data().ref_count.fetch_sub(1, todo!()) == 1 {
            unsafe {drop(Box::from_raw(self.ptr.as_ptr())) }
        }
    }
}

impl <T>Arc<T> {
    fn new(data: T) -> Arc<T> {
        Arc {
            ptr: NonNull::from(Box::leak(Box::new(ArcData {
                ref_count: AtomicUsize::new(1),
                data
            })))
        }
    }

    fn data(&self) -> &ArcData<T> {
        unsafe { &self.ptr.as_ref() }
    }
}