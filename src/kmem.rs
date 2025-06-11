use crate::page::{zalloc_page, Table, PAGE_SIZE};
use core::ptr::null_mut;

#[repr(u64)]
enum AllocListFlags {
    Taken = 1 << 63,
}

impl AllocListFlags {
    pub fn val(self) -> usize {
        self as usize
    }
}

struct AllocList {
    pub flags_size: usize,
}

impl AllocList {
    pub fn is_taken(&self) -> bool {
        self.flags_size & AllocListFlags::Taken.val() != 0
    }

    pub fn is_free(&self) -> bool {
        !self.is_taken()
    }

    pub fn set_taken(&mut self) {
        self.flags_size |= AllocListFlags::Taken.val()
    }

    pub fn set_free(&mut self) {
        self.flags_size &= !AllocListFlags::Taken.val()
    }

    pub fn set_size(&mut self, size: usize) {
        let k = self.is_taken();
        self.flags_size = size & !AllocListFlags::Taken.val();
        if k {
            self.set_taken();
        }
    }
}

// This is the head of the allocation. We start here when we search for a free memory location.
static mut KMEM_HEAD: *mut AllocList = null_mut();
// In the future this will have on-demand pages, so we need to keep track of memory footprint to
// see if we need to allocate more.
static mut KMEM_ALLOC: usize = 0;
static mut KMEM_PAGE_TABLE: *mut Table = null_mut();

pub fn get_head() -> *mut u8 {
    unsafe { KMEM_HEAD as *mut u8 }
}

pub fn get_page_table() -> *mut Table {
    unsafe { KMEM_PAGE_TABLE }
}

pub fn get_num_allocations() -> usize {
    unsafe { KMEM_ALLOC }
}

/// Initialize kernel's memory
/// This is not to be used to allocate memory for user processes.
/// If that's the case, use alloc/free from the page module.
pub fn init() {
    unsafe {
        // Allocate 64 kernel pages (262KiB)
        let k_alloc = zalloc_page(64);
        assert!(!k_alloc.is_null());
        KMEM_ALLOC = 64;
        KMEM_HEAD = k_alloc as *mut AllocList;
        (*KMEM_HEAD).set_free();
        (*KMEM_HEAD).set_size(KMEM_ALLOC * PAGE_SIZE);
        KMEM_PAGE_TABLE = zalloc_page(1) as *mut Table;
    }
}
