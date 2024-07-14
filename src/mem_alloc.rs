/*
 * Modified from:
 * https://stackoverflow.com/questions/74012369/no-global-memory-allocator-found-but-one-is-required-link-to-std-or-add-glob/74012832#74012832
 */

extern crate alloc;

use alloc::alloc::*;
use core::ffi::c_void;
use core::ffi::c_size_t;

/// The static global allocator.
#[global_allocator]
static GLOBAL_ALLOCATOR: Allocator = Allocator;

/// The global allocator type.
#[derive(Default)]
pub struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        malloc(layout.size() as c_size_t) as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        free(ptr as *mut c_void);
    }
    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        realloc(ptr as *mut c_void, new_size) as *mut u8
    }
}

/// If there is an out of memory error, just panic.
#[alloc_error_handler]
fn allocator_error(_layout: Layout) -> ! {
    panic!("out of memory");
}

#[link(name = "c")]
extern "C" {
    fn malloc(size: c_size_t) -> *mut c_void;
    fn realloc(ptr: *mut c_void, size: c_size_t) -> *mut c_void;
    fn free(ptr: *mut c_void);
}
