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

/// Allocate subpage level based on bytes and zero the memory
pub fn kzmalloc(size: usize) -> *mut u8 {
    let size = align_val(size, 3);
    let ret = kmalloc(size);

    if !ret.is_null() {
        for i in 0..size {
            unsafe {
                *ret.add(i) = 0;
            }
        }
    }
    ret
}

/// Allocate subpage level allocation based on bytes
pub fn kmalloc(size: usize) -> *mut u8 {
    let size = align_val(size, 3) + size_of::<AllocList>();
    unsafe {
        let mut head = KMEM_HEAD;
        // Calculate the tail after this allocation
        let tail = KMEM_HEAD.byte_add(KMEM_ALLOC * PAGE_SIZE);

        while head < tail {
            if (*head).is_free() && size <= (*head).get_size() {
                let chunk_size = (*head).get_size();
                let rem = chunk_size - size;
                (*head).set_taken();
                // Is the chunk bigger than what we need? -> split it
                if rem > size_of::<AllocList>() {
                    let next = head.byte_add(size);
                    // Create a new AllocList after this allocation with remaining space and shrink
                    // head to the allocation size
                    (*next).set_free();
                    (*next).set_size(rem);
                    (*head).set_size(size);
                } else {
                    // If we get here, take the entire memory chunk
                    (*head).set_size(size);
                }
                // Return pointer to allocated memory (after the AllocList struct)
                return head.add(1) as *mut u8;
            } else {
                // The investigated memory chunk wasn't free or big enough, move to the next
                head = head.byte_add((*head).get_size());
            }
        }
        // If we get here, then there was no free chunk (with enough memory). Realistically, this should
        // then on-demand allocate a new page, but that is more difficult to implement.
        null_mut()
    }
}

/// Free a subpage level allocation
pub fn kfree(ptr: *mut u8) {
    unsafe {
        if !ptr.is_null() {
            // Get the corresponding AllocList struct which is located at immediately before the 'real' memory
            let allocation = (ptr as *mut AllocList).offset(-1);
            if (*allocation).is_taken() {
                (*allocation).set_free();
            }

            // Coalesce adjacent free memory chunks to reduce fragmentation
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
        // Align by minimum size needed for layout
        kzmalloc(layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        // Ignore the layout because the allocator uses the pointer to determine what to deallocate
        kfree(ptr);
    }
}

#[global_allocator]
static GA: OsGlobalAllocator = OsGlobalAllocator;
