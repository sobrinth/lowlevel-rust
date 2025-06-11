use crate::page::PageBits::{Last, Taken};
use core::ptr::null_mut;

unsafe extern "C" {
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
    static MEMORY_END: usize;
}

static mut ALLOC_START: usize = 0;
pub const PAGE_SIZE: usize = 1 << 12;
pub const PAGE_ORDER: usize = 12;

/// Align (set to a multiple of some power of two)
/// This takes an order, which is the exponent to 2^order
/// Therefore, all alignments must be made as a power of two.
/// This function always rounds up.
pub const fn align_val(val: usize, order: usize) -> usize {
    let o = (1usize << order) - 1;
    (val + o) & !o
}

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

/// Initialize the page allocation system. This allocator implements a Page structure per page
pub fn init() {
    unsafe {
        let num_pages = HEAP_SIZE / PAGE_SIZE;
        let ptr = HEAP_START as *mut Page;

        // Clear all the pages to make sure they aren't accidentially taken
        for i in 0..num_pages {
            (*ptr.add(i)).clear();
        }

        // Determine where the actual useful memory starts. This will be after all the page structures.
        // We also must align the ALLOC_START to a page-boundary.
        // ALLOC_START = (HEAP_START + num_pages * sizeof(Page) + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
        ALLOC_START = align_val(HEAP_START + num_pages * size_of::<Page>(), PAGE_ORDER);
    }
}

/// Allocate a page or multiple pages
pub fn alloc_page(pages: usize) -> *mut u8 {
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
                    // a page structure per PAGE_SIZE bytes starting at ALLOC_START
                    return (ALLOC_START + PAGE_SIZE * i) as *mut u8;
                }
            }
        }
    }

    // If we get here, we didn't find any memory to allocate
    null_mut()
}

/// Allocate and zero a page or multiple pages
/// Each page is PAGE_SIZE that is calculated as 1 << PAGE_ORDER
/// On RISC-V, this typically will be 4096 bytes.
pub fn zalloc_page(pages: usize) -> *mut u8 {
    // Allocate and zero a page
    let ret = alloc_page(pages);
    if !ret.is_null() {
        let size = (PAGE_SIZE * pages) / 8;
        let big_ptr = ret as *mut u64;
        for i in 0..size {
            // We use big_ptr so that we can force an `sd` instruction rather than the `sb`.
            // This means 8x fewer stores than before. Typically, we have to be concerned about remaining
            // bytes, but fortunately, 4096 % 8 = 0, so we won't have any.
            unsafe { *big_ptr.add(i) = 0 }
        }
    }
    ret
}

/// Free a page by its pointer
/// The way this is structured, it will automatically coalesce contiguous pages.
pub fn free_page(ptr: *mut u8) {
    assert!(!ptr.is_null());
    unsafe {
        let page_addr = HEAP_START + (ptr as usize - ALLOC_START) / PAGE_SIZE;
        // Make sure the address makes sense. The calculated address is the page structure, not the HEAP address!
        assert!(page_addr >= HEAP_START && page_addr < MEMORY_END);
        let mut p = page_addr as *mut Page;

        // Keep clearing until the last page is reached
        while (*p).is_taken() && !(*p).is_last() {
            (*p).clear();
            p = p.add(1);
        }

        // If the following assertion fails, it is most likely a double-free
        assert!(
            (*p).is_last(),
            "Possible double-free detected! (Not taken found before last)"
        );

        // If we get here, we've taken care of all previous pages, and we are on the last page.
        (*p).clear();
    }
}

/// Print all page allocations
/// This is mainly used for debugging
pub fn print_page_allocations() {
    unsafe {
        let num_pages = HEAP_SIZE / PAGE_SIZE;
        let mut begin = HEAP_START as *const Page;
        let end = begin.add(num_pages);
        let alloc_begin = ALLOC_START;
        let alloc_end = ALLOC_START + num_pages * PAGE_SIZE;

        println!();
        println!(
            "PAGE ALLOCATION TABLE\nMETA: {:p} -> {:p}\nPHYS: 0x{:x} -> 0x{:x}",
            begin, end, alloc_begin, alloc_end
        );
        println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
        let mut num = 0;
        while begin < end {
            if (*begin).is_taken() {
                let start = begin as usize;
                let memaddr = ALLOC_START + (start - HEAP_START) * PAGE_SIZE;
                print!("0x{:x} => ", memaddr);
                loop {
                    num += 1;
                    if (*begin).is_last() {
                        let end = begin as usize;
                        let memaddr = ALLOC_START + (end - HEAP_START) * PAGE_SIZE + PAGE_SIZE - 1;
                        print!("0x{:x}: {:>3} page(s)", memaddr, end - start + 1);
                        println!(".");
                        break;
                    }
                    begin = begin.add(1);
                }
            }
            begin = begin.add(1);
        }
        println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
        println!(
            "Allocated: {:>6} pages ({:>10} bytes).",
            num,
            num * PAGE_SIZE
        );
        println!(
            "Free     : {:>6} pages ({:>10} bytes).",
            num_pages - num,
            (num_pages - num) * PAGE_SIZE
        );
        println!();
    }
}
