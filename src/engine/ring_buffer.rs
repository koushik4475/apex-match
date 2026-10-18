//! Cacheline-padded, lock-free ring buffer for ultra-low latency command ingestion.
//! Eliminates false sharing via 64-byte alignment and uses atomic Acquire/Release semantics.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// 64-byte cacheline padding to avoid false sharing on modern x86/ARM processors
#[repr(align(64))]
struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    pub const fn new(value: T) -> Self {
        Self { value }
    }
}

/// Ultra-low latency lock-free Single-Producer Single-Consumer (SPSC) Ring Buffer
pub struct SpscRingBuffer<T> {
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    capacity: usize,
    mask: usize,
    // Producer cacheline
    head: CachePadded<AtomicUsize>,
    // Consumer cacheline
    tail: CachePadded<AtomicUsize>,
}

unsafe impl<T: Send> Send for SpscRingBuffer<T> {}
unsafe impl<T: Send> Sync for SpscRingBuffer<T> {}

impl<T> SpscRingBuffer<T> {
    /// Create a new SpscRingBuffer with the given capacity (rounded up to next power of 2)
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(16);
        let mut vec = Vec::with_capacity(cap);
        for _ in 0..cap {
            vec.push(UnsafeCell::new(MaybeUninit::uninit()));
        }

        Self {
            buffer: vec.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head.saturating_sub(tail)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head == tail
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head.saturating_sub(tail) >= self.capacity
    }

    /// Try pushing an item into the ring buffer (non-blocking)
    #[inline(always)]
    pub fn try_push(&self, item: T) -> Result<(), T> {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);

        if head.wrapping_sub(tail) >= self.capacity {
            return Err(item); // Buffer full
        }

        let idx = head & self.mask;
        unsafe {
            let slot = &mut *self.buffer[idx].get();
            slot.write(item);
        }

        self.head
            .value
            .store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Try popping an item from the ring buffer (non-blocking)
    #[inline(always)]
    pub fn try_pop(&self) -> Option<T> {
        let tail = self.tail.value.load(Ordering::Relaxed);
        let head = self.head.value.load(Ordering::Acquire);

        if tail == head {
            return None; // Buffer empty
        }

        let idx = tail & self.mask;
        let item = unsafe {
            let slot = &mut *self.buffer[idx].get();
            slot.assume_init_read()
        };

        self.tail
            .value
            .store(tail.wrapping_add(1), Ordering::Release);
        Some(item)
    }

    /// Drain up to `max_items` into a recipient vector for batch processing
    pub fn drain_batch(&self, max_items: usize, out: &mut Vec<T>) -> usize {
        let mut count = 0;
        while count < max_items {
            if let Some(item) = self.try_pop() {
                out.push(item);
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

impl<T> Drop for SpscRingBuffer<T> {
    fn drop(&mut self) {
        while self.try_pop().is_some() {}
    }
}

/// Producer handle for SPSC ring buffer
pub struct Producer<T> {
    ring: Arc<SpscRingBuffer<T>>,
}

/// Consumer handle for SPSC ring buffer
pub struct Consumer<T> {
    ring: Arc<SpscRingBuffer<T>>,
}

pub fn make_spsc_ring<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let ring = Arc::new(SpscRingBuffer::new(capacity));
    (Producer { ring: ring.clone() }, Consumer { ring })
}

impl<T> Producer<T> {
    #[inline(always)]
    pub fn try_push(&self, item: T) -> Result<(), T> {
        self.ring.try_push(item)
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.ring.is_full()
    }
}

impl<T> Consumer<T> {
    #[inline(always)]
    pub fn try_pop(&self) -> Option<T> {
        self.ring.try_pop()
    }

    #[inline(always)]
    pub fn drain_batch(&self, max_items: usize, out: &mut Vec<T>) -> usize {
        self.ring.drain_batch(max_items, out)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}
