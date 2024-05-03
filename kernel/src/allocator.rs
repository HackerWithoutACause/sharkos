use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use spinning_top::Spinlock;

use crate::PHYSICAL_OFFSET;

// TODO: Get a better allocator
static mut BLOCK_HEAD: Spinlock<*mut UnallocatedPage> =
    Spinlock::new(10000 as *mut UnallocatedPage);

struct UnallocatedPage {
    /// The pointer to the start of the next free block.
    next: *mut UnallocatedPage,
    /// The number of free pages after and including this block.
    size: u64,
}

pub fn initialize(memory_regions: &MemoryRegions) {
    let mut regions = memory_regions
        .iter()
        .filter(|region| region.kind == MemoryRegionKind::Usable)
        .peekable();

    let mut lock = unsafe { BLOCK_HEAD.lock() };

    unsafe {
        *lock = (regions.peek().unwrap().start + PHYSICAL_OFFSET) as *mut UnallocatedPage;
    }

    while let Some(region) = regions.next() {
        unsafe {
            *((region.start + PHYSICAL_OFFSET) as *mut UnallocatedPage) = UnallocatedPage {
                next: (regions
                    .peek()
                    .map(|region| region.start + PHYSICAL_OFFSET)
                    .unwrap_or(u64::MAX)) as *mut UnallocatedPage,
                size: (region.end - region.start) / 4096,
            }
        }
    }
}

/// Allocates a number of free pages given by amount and zeros them out.
/// Returns the start of the physical address.
pub fn allocate_page(amount: u64) -> usize {
    let mut lock = unsafe { BLOCK_HEAD.lock() };
    let mut cursor = *lock;
    let mut trailing = None;

    unsafe {
        while (*cursor).size < amount {
            if (*cursor).next as u64 == u64::MAX {
                return 0; // No sufficient space was found
            }

            trailing = Some(cursor);
            cursor = (*cursor).next;
        }

        let alloc_start_addr = cursor as u64 + (*cursor).size * 4096 - amount * 4096;
        (*cursor).size -= amount;
        if (*cursor).size == 0 {
            // Remove the empty block from the list
            match trailing {
                None => *lock = (*cursor).next,
                Some(previous) => (*previous).next = (*cursor).next,
            }
        }

        // Zero out the allocated memory
        core::ptr::write_bytes(alloc_start_addr as *mut u8, 0, (amount * 4096) as usize);

        // Return the starting physical address of the allocated memory
        (alloc_start_addr - PHYSICAL_OFFSET) as usize
    }
}

pub unsafe fn _free_page(physical_address: u64, amount: u64) {
    let mut lock = unsafe { BLOCK_HEAD.lock() };
    let mut cursor = *lock;
    let mut trailing = None;

    unsafe {
        while ((*cursor).next as u64) < physical_address + PHYSICAL_OFFSET {
            cursor = (*cursor).next;
            trailing = Some(cursor);
        }

        let page = (physical_address + PHYSICAL_OFFSET) as *mut UnallocatedPage;
        (*page).next = cursor;
        (*page).size = amount;

        match trailing {
            None => *lock = page,
            Some(previous) => (*previous).next = page,
        }
    }
}
