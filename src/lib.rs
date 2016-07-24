#![cfg_attr(test, deny(warnings))]
#![deny(missing_docs)]

//! # appendbuf
//!
//! A Sync append-only buffer with Send views.
//!

extern crate block_allocator;
use std::sync::atomic::{self, AtomicUsize, Ordering};
use std::ops::Deref;
use std::io::Read;
use std::{io, mem, fmt};
use block_allocator::Allocator;

/// An append-only, atomically reference counted buffer.
pub struct AppendBuf<'a> {
    alloc: *mut AllocInfo<'a>,
    position: usize
}

unsafe impl<'a> Send for AppendBuf<'a> {}
unsafe impl<'a> Sync for AppendBuf<'a> {}

struct AllocInfo<'a> {
    refcount: AtomicUsize,
    allocator: &'a Allocator<'a>,
    buf: [u8]
}

unsafe impl<'a> Send for AllocInfo<'a> {}
unsafe impl<'a> Sync for AllocInfo<'a> {}

/// A read-only view into an AppendBuf<'a>.
pub struct Slice<'a> {
    alloc: *mut AllocInfo<'a>,
    offset: usize,
    len: usize
}

unsafe impl<'a> Send for Slice<'a> {}
unsafe impl<'a> Sync for Slice<'a> {}

impl<'a> Slice<'a> {
    /// Get a subslice starting from the passed offset.
    pub fn slice_from(&self, offset: usize) -> Slice<'a> {
        if self.len < offset {
            panic!("Slice<'a>d past the end of an appendbuf::Slice<'a>,
                   the length was {:?} and the desired offset was {:?}",
                   self.len, offset);
        }

        self.allocinfo().increment();

        Slice {
            alloc: self.alloc,
            offset: self.offset + offset,
            len: self.len - offset
        }
    }

    /// Get a subslice of the first len elements.
    pub fn slice_to(&self, len: usize) -> Slice<'a> {
        if self.len < len {
            panic!("Slice<'a>d past the end of an appendbuf::Slice<'a>,
                   the length was {:?} and the desired length was {:?}",
                   self.len, len);
        }

        self.allocinfo().increment();

        Slice {
            alloc: self.alloc,
            offset: self.offset,
            len: len
        }
    }

    /// Get a subslice starting at the passed `start` offset and ending at
    /// the passed `end` offset.
    pub fn slice(&self, start: usize, end: usize) -> Slice<'a> {
        let slice = self.slice_from(start);
        slice.slice_to(end - start)
    }

    fn allocinfo(&self) -> &AllocInfo {
        unsafe { mem::transmute(self.alloc) }
    }
}

impl<'a> AppendBuf<'a> {
    /// Create a new, empty AppendBuf<'a> with the given capacity.
    pub fn new(allocator: &'a Allocator) -> AppendBuf<'a> {
        AppendBuf {
            alloc: unsafe { AllocInfo::allocate(allocator) },
            position: 0
        }
    }

    /// Create a new Slice<'a> of the entire AppendBuf<'a> so far.
    pub fn slice(&self) -> Slice<'a> {
        self.allocinfo().increment();

        Slice {
            alloc: self.alloc,
            offset: 0,
            len: self.position
        }
    }

    /// Retrieve the amount of remaining space in the AppendBuf<'a>.
    pub fn remaining(&self) -> usize {
        self.allocinfo().buf.len() - self.position
    }

    /// Write the data in the passed buffer onto the AppendBuf<'a>.
    ///
    /// This is an alternative to using the implementation of `std::io::Write`
    /// which does not unnecessarily use `Result`.
    pub fn fill(&mut self, buf: &[u8]) -> usize {
        use std::io::Write;

        // FIXME: Use std::slice::bytes::copy_memory when it is stabilized.
        let amount = self.get_write_buf().write(buf).unwrap();
        self.position += amount;

        amount
    }

    /// Get the remaining space in the AppendBuf<'a> for writing.
    ///
    /// If you wish the see the data written in subsequent Slice<'a>s,
    /// you must also call `advance` with the amount written.
    ///
    /// Reads from this buffer are reads into uninitalized memory,
    /// and so should be carefully avoided.
    pub fn get_write_buf(&mut self) -> &mut [u8] {
        let position = self.position;
         &mut self.allocinfo_mut().buf[position..]
    }

    /// Advance the position of the AppendBuf<'a>.
    ///
    /// You should only advance the buffer if you have written to a
    /// buffer returned by `get_write_buf`.
    pub unsafe fn advance(&mut self, amount: usize) {
         self.position += amount;
    }

    /// Read from the given io::Read into the AppendBuf<'a>.
    ///
    /// Safety note: it is possible to read uninitalized memory if the
    /// passed io::Read incorrectly reports the number of bytes written to
    /// buffers passed to it.
    pub fn read_from<R: Read>(&mut self, reader: &mut R) -> io::Result<usize> {
        reader.read(self.get_write_buf()).map(|n| {
            unsafe { self.advance(n) };
            n
        })
    }

    fn allocinfo(&self) -> &AllocInfo {
        unsafe { mem::transmute(self.alloc) }
    }

    fn allocinfo_mut(&mut self) -> &mut AllocInfo {
        unsafe { mem::transmute(self.alloc) }
    }
}

impl<'a> fmt::Debug for AppendBuf<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a> fmt::Debug for Slice<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a> Deref for AppendBuf<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.allocinfo().buf[..self.position]
    }
}

impl<'a> AsRef<[u8]> for AppendBuf<'a> {
    fn as_ref(&self) -> &[u8] { self }
}

impl<'a> io::Write for AppendBuf<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(self.fill(buf))
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl<'a> Deref for Slice<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        unsafe { &(*self.alloc).buf[self.offset..self.offset + self.len] }
    }
}

impl<'a> AsRef<[u8]> for Slice<'a> {
    fn as_ref(&self) -> &[u8] { self }
}

impl<'a> Clone for Slice<'a> {
    fn clone(&self) -> Slice<'a> {
        self.allocinfo().increment();

        Slice {
            alloc: self.alloc,
            offset: self.offset,
            len: self.len
        }
    }
}

impl<'a> AllocInfo<'a> {
    unsafe fn allocate(allocator : &'a Allocator) -> *mut Self {
        //TODO Handle this error
        let buf = allocator.alloc_raw().unwrap();
        let raw_size = allocator.get_block_size() as usize;
        let usable_size = raw_size - (mem::size_of::<AtomicUsize>() + mem::size_of::<&Allocator>());
        let this = mem::transmute::<_, *mut Self>((buf, usable_size));
        (*this).refcount = AtomicUsize::new(1);
        (*this).allocator = allocator;
        this
    }

    #[inline(always)]
    fn increment(&self) {
         self.refcount.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    unsafe fn decrement(&self) {
        // Adapted from the implementation of Drop for std::sync::Arc.

        // Because `fetch_sub` is already atomic, we do not need to synchronize
        // with other threads unless we are going to deallocate the buffer.
        if self.refcount.fetch_sub(1, Ordering::Release) != 1 { return }

        // This fence is needed to prevent reordering of use of the data and
        // deletion of the data. Because it is marked `Release`, the decreasing
        // of the reference count synchronizes with this `Acquire` fence. This
        // means that use of the data happens before decreasing the reference
        // count, which happens before this fence, which happens before the
        // deletion of the data.
        //
        // As explained in the [Boost documentation][1],
        //
        // > It is important to enforce any possible access to the object in one
        // > thread (through an existing reference) to *happen before* deleting
        // > the object in a different thread. This is achieved by a "release"
        // > operation after dropping a reference (any access to the object
        // > through this reference must obviously happened before), and an
        // > "acquire" operation before deleting the object.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        atomic::fence(Ordering::Acquire);

        let alloc  = self.allocator;
        let (ptr, _) : (*mut u8, usize) = mem::transmute(self);
        alloc.free_raw(mem::transmute(ptr)).unwrap(); // TODO handle this result better
    }
}

impl<'a> Drop for Slice<'a> {
    fn drop(&mut self) {
        unsafe { (*self.alloc).decrement() }
    }
}

impl<'a> Drop for AppendBuf<'a> {
    fn drop(&mut self) {
        unsafe { (*self.alloc).decrement() }
    }
}

fn _compile_test() {
    fn _is_send_sync<T: Send + Sync>() {}
    _is_send_sync::<AppendBuf>();
    _is_send_sync::<Slice>();
}

#[test]
fn test_write_and_slice() {
    let alloc = Allocator::new(128, 100).unwrap();
    let mut buf = AppendBuf::new(&alloc);
    assert_eq!(buf.fill(&[1, 2, 3]), 3);
    let slice = buf.slice();
    assert_eq!(&*slice, &[1, 2, 3]);

    assert_eq!(&*buf, &[1, 2, 3]);
}

#[test]
fn test_overlong_write() {
    let alloc = Allocator::new(32, 100).unwrap();
    let mut buf = AppendBuf::new(&alloc);
    assert_eq!(buf.fill(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]), 16);
    let slice = buf.slice();
    assert_eq!(&*slice, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
}

#[test]
fn test_slice_slicing() {
    let alloc = Allocator::new(32, 10).unwrap();
    let data = &[1, 2, 3, 4, 5, 6, 7, 8];

    let mut buf = AppendBuf::new(&alloc);
    assert_eq!(buf.fill(data), 8);

    assert_eq!(&*buf.slice(), data);
    assert_eq!(&*buf.slice().slice_to(5), &data[..5]);
    assert_eq!(&*buf.slice().slice_from(6), &data[6..]);
    assert_eq!(&*buf.slice().slice(2, 7), &data[2..7]);
}

#[test]
fn test_many_writes() {
    let alloc = Allocator::new(128, 10).unwrap();
    let mut buf = AppendBuf::new(&alloc);

    assert_eq!(buf.fill(&[1, 2, 3, 4]), 4);
    assert_eq!(buf.fill(&[10, 12, 13, 14, 15]), 5);
    assert_eq!(buf.fill(&[34, 35]), 2);

    assert_eq!(&*buf.slice(), &[1, 2, 3, 4, 10, 12, 13, 14, 15, 34, 35]);
}

#[test]
fn test_slice_then_write() {
    let alloc = Allocator::new(32, 10).unwrap();
    let mut buf = AppendBuf::new(&alloc);
    let empty = buf.slice();
    assert_eq!(&*empty, &[]);

    assert_eq!(buf.fill(&[5, 6, 7, 8]), 4);

    let not_empty = buf.slice();
    assert_eq!(&*empty, &[]);
    assert_eq!(&*not_empty, &[5, 6, 7, 8]);

    assert_eq!(buf.fill(&[9, 10, 11, 12, 13]), 5);
    assert_eq!(&*empty, &[]);
    assert_eq!(&*not_empty, &[5, 6, 7, 8]);
    assert_eq!(&*buf.slice(), &[5, 6, 7, 8, 9, 10, 11, 12, 13]);
}

#[test]
fn test_slice_bounds_edge_cases() {
    let alloc = Allocator::new(32, 10).unwrap();
    let data = &[1, 2, 3, 4, 5, 6, 7, 8];

    let mut buf = AppendBuf::new(&alloc);
    assert_eq!(buf.fill(data), data.len());

    let slice = buf.slice().slice_to(data.len());
    assert_eq!(&*slice, data);

    let slice = buf.slice().slice_from(0);
    assert_eq!(&*slice, data);
}

#[test]
#[should_panic = "the desired offset"]
fn test_slice_from_bounds_checks() {
    let alloc = Allocator::new(32, 10).unwrap();
    let data = &[1, 2, 3, 4, 5, 6, 7, 8];

    let mut buf = AppendBuf::new(&alloc);
    assert_eq!(buf.fill(data), 8);

    buf.slice().slice_from(100);
}

#[test]
#[should_panic = "the desired length"]
fn test_slice_to_bounds_checks() {
    let alloc = Allocator::new(32, 10).unwrap();
    let data = &[1, 2, 3, 4, 5, 6, 7, 8];

    let mut buf = AppendBuf::new(&alloc);
    assert_eq!(buf.fill(data), 8);

    buf.slice().slice_to(100);
}


