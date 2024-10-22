use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, fence};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

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
    //Arcs count
    data_ref_count: AtomicUsize,
    //Arcs and Weaks combined
    alloc_ref_count: AtomicUsize,
    //Data, `None` if there's only weak pointers left.
    data: UnsafeCell<Option<T>>
}

pub struct Arc<T> {
    weak: Weak<T>
}

unsafe impl<T: Send + Sync> Sync for Arc<T> {}
unsafe impl<T: Send + Sync> Send for Arc<T> {}
impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        let ptr = self.weak.data().data.get();
        unsafe {(*ptr).as_ref().unwrap()}
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        let weak = self.weak.clone();
        if weak.data().data_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Arc { weak }
    }
}

impl <T> Drop for Arc<T> {
    fn drop(&mut self) {
        if self.data().alloc_ref_count.fetch_sub(1, Relaxed) == 1 {
            fence(Acquire);

            let ptr = self.weak.data().data.get();

            unsafe {*ptr = None}
        }
    }
}

impl <T>Arc<T> {
    fn new(data: T) -> Arc<T> {
        Arc {
            weak: Weak {
                ptr: NonNull::from(Box::leak(Box::new(ArcData {
                    data_ref_count: AtomicUsize::new(1),
                    alloc_ref_count: AtomicUsize::new(1),
                    data: UnsafeCell::new(Some(data))
                })))
            }
        }
    }

    fn data(&self) -> &ArcData<T> {
        unsafe { &self.weak.data() }
    }

    fn get_mut(arc: &mut Self) -> Option<&mut T> {
        if arc.data().alloc_ref_count.load(Relaxed) == 1 {
            fence(Acquire);

            let arc_data = unsafe {arc.weak.ptr.as_mut()};

            let option = arc_data.data.get_mut();

            let data = option.as_mut().unwrap();

            Some(data)
        } else {
            None
        }
    }
}

pub struct Weak<T> {
    ptr: NonNull<ArcData<T>>
}

impl <T> Weak<T> {
    fn data(&self) -> &ArcData<T> {
        unsafe { self.ptr.as_ref() }
    }
}

impl <T> Clone for Weak<T> {
    fn clone(&self) -> Self {
        if self.data().alloc_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort()
        }

        Weak {ptr: self.ptr}
    }
}

impl <T> Drop for Weak<T> {
    fn drop(&mut self) {
        if self.data().alloc_ref_count.fetch_sub(1, Release) == 1 {
            fence(Acquire);
            unsafe {drop(Box::from_raw(self.ptr.as_ptr())) }
        }
    }
}


unsafe impl<T: Sync + Send> Send for Weak<T> {}
unsafe impl<T: Sync + Send> Sync for Weak<T> {}