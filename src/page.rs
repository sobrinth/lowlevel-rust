use crate::page::PageBits::{Last, Taken};
use core::ptr::null_mut;

unsafe extern "C" {
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
    static MEMORY_END: usize;
}

static mut ALLOC_START: usize = 0;
pub const PAGE_SIZE: usize = 1 << 12;

#[repr(u8)]
pub enum PageBits {
    Empty = 0,
    Taken = 1 << 0,
    Last = 1 << 1,
}

/// Each page is described by the Page structure. Linux does this as well, where each 4096-byte chunk
/// of memory has a structure associated with it. However, their structure is much larger.
pub struct Page {
    flags: u8,
}

impl Page {
    pub fn is_last(&self) -> bool {
        self.flags & Last as u8 != 0
    }

    pub fn is_taken(&self) -> bool {
        self.flags & Taken as u8 != 0
    }

    pub fn is_free(&self) -> bool {
        !self.is_taken()
    }

    pub fn clear(&mut self) {
        self.flags = PageBits::Empty as u8;
    }

    pub fn set_flag(&mut self, flag: PageBits) {
        self.flags |= flag as u8;
    }

    pub fn clear_flag(&mut self, flag: PageBits) {
        self.flags &= !(flag as u8);
    }
}

pub fn alloc(pages: usize) -> *mut u8 {
    assert!(pages > 0);
    unsafe {
        /*
        Create a Page structure for each page on the heap. We might actually have more since HEAP_SIZE
        moves and so does the size of our structure, but we'll only waste a few bytes.
        */
        let num_usable_pages = (MEMORY_END - ALLOC_START) / PAGE_SIZE;
        let bp = HEAP_START as *mut Page;
        for i in 0..num_usable_pages {
            let mut found = false;
            // Check to see if the page is free. If so, we have our first candidate memory
            if (*bp.add(i)).is_free() {
                found = true;
                for j in i..i + pages {
                    // Check if there is a contiguous allocation for all pages
                    if (*bp.add(j)).is_taken() {
                        found = false;
                        break;
                    }
                }
                /*
                We've checked to see if there are enough contiguous pages to allocate what is needed. If true,
                valid memory was found that can be allocated.
                 */
                if found {
                    for k in i..i + pages - 1 {
                        (*bp.add(k)).set_flag(Taken);
                    }
                    // The marker for the last page is PageBits::Last. This lets us know we hit the
                    // end of this particular allocation.
                    (*bp.add(i + pages - 1)).set_flag(Taken);
                    (*bp.add(i + pages - 1)).set_flag(Last);

                    // The page structures themselves aren't the useful memory. Instead, there is
                    // 1 page structure per 4096 bytes starting at ALLOC_START
                    return (ALLOC_START + PAGE_SIZE * i) as *mut u8;
                }
            }
        }
    }

    // If we get here, we didn't find any memory to allocate
    null_mut()
}
