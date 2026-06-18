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

    #[allow(dead_code)]
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
        for i in 0..=num_usable_pages.saturating_sub(pages) {
            // Check to see if the page is free. If so, we have our first candidate memory
            if (*bp.add(i)).is_free() {
                let mut found = true;
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
        let alloc_end = MEMORY_END;

        println!();
        println!(
            "PAGE ALLOCATION TABLE\nMETA: {:p} -> {:p}\nPHYS: 0x{:x} -> 0x{:x}",
            begin, end, alloc_begin, alloc_end
        );
        println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
        println!("ALLOCATED PAGE RANGES");
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
                        print!("0x{:x}: {:>8} page(s)", memaddr, end - start + 1);
                        println!(".");
                        break;
                    }
                    begin = begin.add(1);
                }
            }
            begin = begin.add(1);
        }
        println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
        println!("ALLOCATION SUMMARY");
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

// MMU

#[repr(i64)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub enum EntryBits {
    None = 0,
    Valid = 1 << 0,
    Read = 1 << 1,
    Write = 1 << 2,
    Execute = 1 << 3,
    User = 1 << 4,
    Global = 1 << 5,
    Access = 1 << 6,
    Dirty = 1 << 7,

    // Convenience combinations
    ReadWrite = 1 << 1 | 1 << 2,
    ReadExecute = 1 << 1 | 1 << 3,
    ReadWriteExecute = 1 << 1 | 1 << 2 | 1 << 3,

    // User convenience combinations
    UserReadWrite = 1 << 4 | 1 << 1 | 1 << 2,
    UserReadExecute = 1 << 4 | 1 << 1 | 1 << 3,
    UserReadWriteExecute = 1 << 4 | 1 << 1 | 1 << 2 | 1 << 3,
}

impl EntryBits {
    pub fn val(self) -> usize {
        self as usize
    }
}

/// Table represents a single table, which contains 512 (2^9) entries.
pub struct Table {
    pub entries: [Entry; 512],
}

impl Table {
    pub fn len() -> usize {
        512
    }
}

pub struct Entry {
    pub entry: usize,
}

impl Entry {
    pub fn is_valid(&self) -> bool {
        self.entry & EntryBits::Valid.val() != 0
    }

    // The first bit (bit index #0 is the V-bit for valid
    pub fn is_invalid(&self) -> bool {
        !self.is_valid()
    }

    // A leaf has one or more RWX bits set
    pub fn is_leaf(&self) -> bool {
        self.entry & 0xe != 0
    }

    pub fn is_branch(&self) -> bool {
        !self.is_leaf()
    }

    pub fn set_entry(&mut self, entry: usize) {
        self.entry = entry;
    }
}

/// Map a virtual address to a physical address using 4096-byte page size.
/// root: a mutable reference to the root Table
/// vaddr: The virtual address to map
/// paddr: The physical address to map
/// bits: An OR'd bitset containing the bits the leaf should have.
///       The bits should contain only the following:
///         Read, Write, Execute, User, and/or Global
///       The bits MUST include one or more of the following:
///         Read, Write, Execute
///       The valid bit automatically gets added.
pub fn map(root: &mut Table, vaddr: usize, paddr: usize, bits: usize, level: usize) {
    // Make sure that R, W, or X have been provided. Otherwise, we'll leak memory and always page fault.
    assert_ne!(bits & 0xe, 0);

    /*
    Extract out each Virtual Page Number (VPN) from the virtual address.
    On the virtual address, each VPN is exactly 9 bits,
    which is why we use the mask 0x1ff = 0b1_1111_1111 (9 bits)
     */
    let vpn = [
        // VPN[0] = vaddr[20:12]
        (vaddr >> 12) & 0x1ff,
        // VPN[1] = vaddr[29:21]
        (vaddr >> 21) & 0x1ff,
        // VPN[2] = vaddr[38:30]
        (vaddr >> 30) & 0x1ff,
    ];

    /*
    Extract out each Physical Page Number (PPN) from the physical address.
    However, PPN[2] is different in that it stores 26 bits instead of 9.
    Therefore, we use, 0x3ff_ffff = 0b11_1111_1111_1111_1111_1111_1111 (26 bits)
     */
    let ppn = [
        // PPN[0] = paddr[20:12]
        (paddr >> 12) & 0x1ff,
        // PPN[1] = paddr[29:21]
        (paddr >> 21) & 0x1ff,
        // PPN[2] = paddr[55:30]
        (paddr >> 30) & 0x3ff_ffff,
    ];

    // Floating reference to set while walking the table
    let mut v = &mut root.entries[vpn[2]];

    /*
    Now, traverse the page table and set the bits properly. We expect the root to be
    valid, however, we're required to create anything beyond the root.
    We reverse the iteration since we need to start with VPN[2]. The range operator is inclusive
    at the start but exclusive at the end, so (0..2) will iterate 0 and 1.
    */
    for i in (level..2).rev() {
        if v.is_invalid() {
            // allocate a page
            let page = zalloc_page(1);
            // The page is already aligned, so store it directly. The page is stored in the entry >> 2
            v.set_entry((page as usize >> 2) | EntryBits::Valid.val());
        }
        let entry = ((v.entry & !0x3ff) << 2) as *mut Entry; // Page address of entry (without the bits)
        v = unsafe { entry.add(vpn[i]).as_mut().unwrap() };
    }

    /*
    When we get here, we should be at VPN[0] and v should be pointing to our entry.
     */
    let entry = (ppn[2] << 28) | (ppn[1] << 19) | (ppn[0] << 10) | bits | EntryBits::Valid.val();
    v.set_entry(entry);
}

/// Unmaps and frees all memory associated with a table.
/// root: The root table to start freeing.
/// NOTE: This does not free root directly. This must be freed manually
///       The reason we don't free root is that it is usually embedded into the Process structure
#[allow(dead_code)]
pub fn unmap(root: &mut Table) {
    // Start with level 2
    for lv2 in 0..Table::len() {
        let entry_lv2 = &root.entries[lv2];
        if entry_lv2.is_valid() && entry_lv2.is_branch() {
            // This is a valid entry, so drill down and free
            let memaddr_lv1 = (entry_lv2.entry & !0x3ff) << 2;
            let table_lv1 = unsafe {
                // Make table_lv1 a &mut instead of pointer
                (memaddr_lv1 as *mut Table).as_mut().unwrap()
            };
            for lv1 in 0..Table::len() {
                let entry_lv1 = &table_lv1.entries[lv1];
                if entry_lv1.is_valid() && entry_lv1.is_branch() {
                    let memaddr_lv0 = (entry_lv1.entry & !0x3ff) << 2;
                    // The next level is level 0, which cannot have branches, so free here
                    free_page(memaddr_lv0 as *mut u8);
                }
            }
            free_page(memaddr_lv1 as *mut u8);
        }
    }
}

/// Walk the page table to convert a virtual address to a physical address
/// If a page fault occurs, this returns `None`
/// Otherwise, it returns `Some` with the physical address
pub fn virt_to_phys(root: &Table, vaddr: usize) -> Option<usize> {
    // Walk the page table pointed to by the root
    let vpn = [
        (vaddr >> 12) & 0x1ff,
        (vaddr >> 21) & 0x1ff,
        (vaddr >> 30) & 0x1ff,
    ];

    let mut v = &root.entries[vpn[2]];
    for i in (0..=2).rev() {
        if v.is_invalid() {
            // This is an invalid entry -> page fault
            break;
        } else if v.is_leaf() {
            /*
            According to RISC-V, a leaf can be at any level.

            The offset mask masks off the PPN. Each PPN is 9 bits, and they start at bit #12.
            So the formula is: 12 + i * 9
             */
            let off_mask = (1 << (12 + i * 9)) - 1;
            let vaddr_pgoff = vaddr & off_mask;
            let addr = (v.entry << 2) & !off_mask;
            return Some(addr | vaddr_pgoff);
        }

        /*
        Set `v` to the next entry, which is pointed to by this entry. However, the address was shifted
        right by 2 places when stored in the PTE, so we shift it left.else
         */
        let entry = ((v.entry & !0x3ff) << 2) as *const Entry;
        // We do i-1 here, which can be dangerous. We should get None or Some() above before we do 0 - 1 = -1
        v = unsafe { entry.add(vpn[i - 1]).as_ref().unwrap() };
    }
    // Exhausted all valid tables and haven't found a leaf
    None
}
