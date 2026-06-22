use crate::page::{PAGE_SIZE, Table, align_val, zalloc_page};
use core::alloc::{GlobalAlloc, Layout};
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
    // Combination of size and flag for an allocation
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

    pub fn get_size(&self) -> usize {
        self.flags_size & !AllocListFlags::Taken.val()
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
        KMEM_ALLOC = 512;
        let k_alloc = zalloc_page(KMEM_ALLOC);
        assert!(!k_alloc.is_null());
        KMEM_HEAD = k_alloc as *mut AllocList;
        (*KMEM_HEAD).set_free();
        (*KMEM_HEAD).set_size(KMEM_ALLOC * PAGE_SIZE);
        KMEM_PAGE_TABLE = zalloc_page(1) as *mut Table;
    }
}

/// Allocate subpage level based on bytes and zero the memory
#[allow(dead_code)]
pub fn kzmalloc(size: usize) -> *mut u8 {
    let ret = kmalloc(size, 8);
    if !ret.is_null() {
        let aligned_size = align_val(size, 3);
        for i in 0..aligned_size {
            unsafe { *ret.add(i) = 0; }
        }
    }
    ret
}

/// Allocate subpage level allocation based on bytes and alignment.
///
/// Layout inside the heap chunk:
///   [AllocList header (8 B)] [padding (0..align-8 B)] [back-ptr (8 B)] [user data (aligned)]
///
/// The back-pointer (stored immediately before the returned pointer) lets `kfree` recover the
/// AllocList header regardless of how much padding was inserted for alignment.
pub fn kmalloc(size: usize, align: usize) -> *mut u8 {
    let align = align.max(8);
    let user_size = align_val(size, 3);
    // Worst-case overhead: header(8) + back-ptr(8) + padding(align-8) = align+8
    let total = size_of::<AllocList>() + align + user_size;
    unsafe {
        let mut head = KMEM_HEAD;
        let tail = KMEM_HEAD.byte_add(KMEM_ALLOC * PAGE_SIZE);

        while head < tail {
            if (*head).is_free() && total <= (*head).get_size() {
                let chunk_size = (*head).get_size();
                let rem = chunk_size - total;
                (*head).set_taken();
                if rem > size_of::<AllocList>() {
                    let next = head.byte_add(total);
                    (*next).set_free();
                    (*next).set_size(rem);
                    (*head).set_size(total);
                } else {
                    // Take the entire chunk; record its true size so walkers stay aligned
                    (*head).set_size(chunk_size);
                }

                // Compute the aligned return address.
                // back-ptr occupies the 8 bytes immediately before user_start.
                let after_header = head as usize + size_of::<AllocList>();
                let back_ptr_size = size_of::<*mut AllocList>();
                let user_start =
                    align_val(after_header + back_ptr_size, align.trailing_zeros() as usize);

                // Store back-pointer so kfree can recover the AllocList header
                let back_ptr = (user_start - back_ptr_size) as *mut *mut AllocList;
                *back_ptr = head;

                return user_start as *mut u8;
            } else {
                head = head.byte_add((*head).get_size());
            }
        }
        null_mut()
    }
}

/// Free a subpage level allocation
pub fn kfree(ptr: *mut u8) {
    unsafe {
        if !ptr.is_null() {
            // Recover the AllocList header via the back-pointer stored immediately before ptr
            let back_ptr = ptr.byte_sub(size_of::<*mut AllocList>()) as *const *mut AllocList;
            let allocation = *back_ptr;
            if (*allocation).is_taken() {
                (*allocation).set_free();
            }
            coalesce();
        }
    }
}

/// Merge smaller chunks into a bigger chunk
pub fn coalesce() {
    unsafe {
        let mut head = KMEM_HEAD;
        let tail = KMEM_HEAD.byte_add(KMEM_ALLOC * PAGE_SIZE);
        while head < tail {
            let next = head.byte_add((*head).get_size());
            if (*head).get_size() == 0 {
                // If this ever happens, we have a bad heap (double free or worse). This would cause
                // an infinite loop since the `next` pointer would never move
                break;
            } else if next >= tail {
                // `next` was calculated using the size given by `get_size()`, however this could push
                // the pointer past tail. In that case, the size is wrong, hence we break and stop
                break;
            } else if (*head).is_free() && (*next).is_free() {
                // This means there are adjacent free blocks, so we combine them into one allocation
                (*head).set_size((*head).get_size() + (*next).get_size());
            }
            // Recalculate the new head
            head = head.byte_add((*head).get_size());
        }
    }
}

/// For debugging purposes, print the kmem table
pub fn print_table() {
    println!("\nKMEM HEAP ALLOCATIONS:");
    unsafe {
        let mut head = KMEM_HEAD;
        let tail = KMEM_HEAD.byte_add(KMEM_ALLOC * PAGE_SIZE);
        while head < tail {
            println!(
                "{:p}: Length = {:<10} Taken = {}",
                head,
                (*head).get_size(),
                (*head).is_taken()
            );
            head = head.byte_add((*head).get_size());
        }
    }
}

/// The global allocator is a static constant to a global allocator structure. We don't need any
/// members because we're using this structure just to implement `alloc` and `dealloc`.
struct OsGlobalAllocator;

unsafe impl GlobalAlloc for OsGlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        kmalloc(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        // Ignore the layout because the allocator uses the pointer to determine what to deallocate
        kfree(ptr);
    }
}

#[global_allocator]
static GA: OsGlobalAllocator = OsGlobalAllocator;
