use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
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
    //Arcs and Weak combined
    alloc_ref_count: AtomicUsize,
    //Data, Dropped if there are only weak pointers left...
    data: UnsafeCell<std::mem::ManuallyDrop<T>>
}

pub struct Arc<T> {
    ptr: NonNull<ArcData<T>>
}

unsafe impl<T: Send + Sync> Sync for Arc<T> {}
unsafe impl<T: Send + Sync> Send for Arc<T> {}

impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.data().data.get() }
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        if self.data().data_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Arc { ptr: self.ptr}
    }
}

impl <T> Drop for Arc<T> {
    fn drop(&mut self) {
        if self.data().data_ref_count.fetch_sub(1, Relaxed) == 1 {
            fence(Acquire);

            unsafe {
                ManuallyDrop::drop(&mut *self.data().data.get())
            }

            drop(Weak {ptr: self.ptr});

        }
    }
}

impl <T>Arc<T> {
    fn new(data: T) -> Arc<T> {
        Arc {
            ptr: NonNull::from(Box::leak(Box::new(ArcData {
                data_ref_count: AtomicUsize::new(1),
                alloc_ref_count: AtomicUsize::new(1),
                data: UnsafeCell::new(ManuallyDrop::new(data))
            })))
        }
    }

    fn data(&self) -> &ArcData<T> {
        unsafe { &self.ptr.as_ref() }
    }

    fn get_mut(arc: &mut Self) -> Option<&mut T> {
        if arc.data().alloc_ref_count.compare_exchange(1, usize::MAX, Acquire, Relaxed).is_err() {
            return None
        }

        let is_unique = arc.data().data_ref_count.load(Relaxed) == 1;

        arc.data().alloc_ref_count.store(1, Release);

        if !is_unique {
            return None
        }

        fence(Acquire);

        unsafe {Some(&mut *arc.data().data.get())}
    }

    fn downgrade(arc: &Self) -> Weak<T> {
        let mut n = arc.data().alloc_ref_count.load(Relaxed);

        loop {
            if n == usize::MAX {
                std::hint::spin_loop();
                n = arc.data().alloc_ref_count.load(Relaxed);
                continue;
            }

            if let Err(e) =
                arc.data()
                    .alloc_ref_count
                    .compare_exchange_weak(n, n + 1, Acquire, Relaxed)
            {
                n = e;
                continue;
            }

            return Weak {ptr: arc.ptr}
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

    pub fn upgrade(&self) -> Option<Arc<T>> {

        let mut n = self.data().data_ref_count.load(Relaxed);

        loop {
            if n == 0 {
                return None
            }

            if let Err(current_ref_count) =
                self
                    .data()
                    .data_ref_count
                    .compare_exchange_weak(n, n + 1, Relaxed, Relaxed)
            {
                n = current_ref_count;
                continue
            }

            return Some(Arc {ptr: self.ptr})
        }
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

