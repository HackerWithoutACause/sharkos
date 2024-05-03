use core::{
    alloc::Allocator,
    arch::asm,
    ops::{Index, IndexMut},
    ptr::null_mut,
};

use crate::{allocator::allocate_page, KERNEL_PAGE_TABLE, KERNEL_START, PHYSICAL_OFFSET};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Flags(u64);

impl Flags {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(0b110);
    pub const WRITE: Self = Self(1 << 1);
    pub const USER: Self = Self(1 << 2);
    pub const HUGE_PAGE: Self = Self(1 << 2);
    pub const NOT_EXECUTABLE: Self = Self(1 << 63);
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry(u64);

impl core::fmt::Debug for Entry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:#0x} u: {} w: {} x: {} p: {}",
            (self.0 & !((1 << 12) - 1) & ((1 << 51) - 1)),
            (self.0 >> 2) & 1,
            (self.0 >> 1) & 1,
            (!self.0 >> 63) & 1,
            self.0 & 1
        )
    }
}

impl Entry {
    pub const EMPTY: Self = Entry(0);

    // Function to create a page table entry given a address to a lower entry
    // table, setting the appropriate flags for the entry.
    pub unsafe fn new(address: u64, flags: Flags) -> Self {
        // Ensure the address is page-aligned, as the lower 12 bits should be zeros.
        assert!(
            address & 0xfff == 0,
            "Page entry addresses must be page-aligned"
        );

        Entry(address | flags.0 | 1)
    }

    fn address(&self) -> u64 {
        self.0 & !((1 << 12) - 1) & ((1 << 51) - 1)
    }

    pub fn is_present(&self) -> bool {
        (self.0 & 1) == 1
    }

    unsafe fn get_table(&self) -> &mut Table {
        unsafe {
            ((self.address() + PHYSICAL_OFFSET) as *mut Table)
                .as_mut()
                .unwrap()
        }
    }

    fn set_flags(&mut self, flags: Flags) {
        self.0 |= flags.0;
    }

    fn is_executable(&self) -> bool {
        (self.0 >> 63) & 1 == 0
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: VirtualAllocator = VirtualAllocator;

unsafe impl core::alloc::GlobalAlloc for VirtualAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.alloc_zeroed(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.allocate_zeroed(layout)
            .map(|ptr| ptr.as_ptr() as *mut u8)
            .unwrap_or(null_mut())
    }

    unsafe fn realloc(
        &self,
        _ptr: *mut u8,
        _layout: core::alloc::Layout,
        _new_size: usize,
    ) -> *mut u8 {
        todo!();
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        todo!()
    }
}

/// Allocates memory by updating page tables and avoiding memcpys.
#[derive(Debug)]
pub struct VirtualAllocator;

unsafe impl core::alloc::Allocator for VirtualAllocator {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.allocate_zeroed(layout)
    }

    fn allocate_zeroed(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        let page_table = unsafe { crate::get_active_page_table().as_mut().unwrap() };
        let minimum = table_indies(KERNEL_START as usize)[0];

        // Make sure alignment is less than the page size. This is unlikly,
        // but it's still good to check. This is OK because Layout::align is
        // guaranteed to be a power of 2.
        if layout.align() > 4096 {
            return Err(core::alloc::AllocError);
        }

        let address = page_table
            .0
            .iter()
            .enumerate()
            .skip(minimum)
            .find_map(|(index, entry)| match entry {
                _ if !entry.is_present() => Some(index << 39),
                _ if !entry.is_executable() => todo!(),
                _ => None,
            })
            .unwrap()
            | KERNEL_START as usize;

        unsafe {
            page_table.create_mapping(address, allocate_page(1), Flags::WRITE);
        }

        Ok(core::ptr::NonNull::slice_from_raw_parts(
            core::ptr::NonNull::new(address as *mut u8).unwrap(),
            4096,
        ))
    }

    unsafe fn grow(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.grow_zeroed(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        println!(
            "growing from {} to {}",
            old_layout.size(),
            new_layout.size()
        );
        // Make sure alignment is less than the page size. This is unlikly,
        // but it's still good to check. This is OK because Layout::align is
        // guaranteed to be a power of 2.
        if new_layout.align() > 4096 {
            return Err(core::alloc::AllocError);
        }

        let page_table = unsafe { crate::get_active_page_table().as_mut().unwrap() };

        let indies = table_indies(ptr.as_ptr() as usize);
        let pages_needed = (new_layout.size() - old_layout.size()).div_ceil(4096);

        for page in 0..pages_needed {
            let table =
                page_table[indies[0]].get_table()[indies[1]].get_table()[indies[2]].get_table();
            let new_page = allocate_page(1);
            table[indies[3] + page] = Entry::new(new_page as u64, Flags::WRITE);
        }

        Ok(core::ptr::NonNull::slice_from_raw_parts(
            ptr,
            (indies[3] + pages_needed) * 4096,
        ))
    }

    unsafe fn deallocate(&self, _ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        todo!(
            "Implement deallocation for VirtualAllocator with layout: {:#?}",
            layout
        );
    }
}

#[repr(C, align(4096))]
#[derive(Debug, Clone, Copy)]
pub struct Table(pub [Entry; 512]);

fn table_indies(address: usize) -> [usize; 4] {
    [
        (address >> 39) & 0x1ff,
        (address >> 30) & 0x1ff,
        (address >> 21) & 0x1ff,
        (address >> 12) & 0x1ff,
    ]
}

impl Table {
    pub unsafe fn activate(pointer: *const Table) {
        asm!("mov cr3, {}", in(reg) pointer as u64 - unsafe { PHYSICAL_OFFSET });
    }

    pub unsafe fn activate_kernel_table() {
        Self::activate(*KERNEL_PAGE_TABLE.get().unwrap() as *const Table)
    }

    /// Creates a copy of the current table and returns a pointer it.
    pub fn new_copy() -> *mut Table {
        let mut cr3: u64;
        unsafe {
            asm!("mov {x}, cr3", x = out(reg) cr3);
        }
        let page_table = unsafe { ((cr3 + PHYSICAL_OFFSET) as *mut Table).as_mut().unwrap() };

        let page = allocate_page(1);
        let table = (page as u64 + unsafe { PHYSICAL_OFFSET }) as *mut Table;

        unsafe {
            *table = *page_table;
        }

        unsafe {
            (*table).0[0] = Entry::EMPTY;
        }

        table
    }

    pub unsafe fn create_mapping(
        &mut self,
        virtual_address: usize,
        physical_address: usize,
        flags: Flags,
    ) {
        assert!(
            physical_address & 0xfff == 0,
            "Physical address must be page-aligned"
        );

        assert!(
            virtual_address & 0xfff == 0,
            "Virtual address must be page-aligned"
        );

        let indies = table_indies(virtual_address);
        let final_table = unsafe {
            self.get_or_create(indies[0], flags)
                .get_table()
                .get_or_create(indies[1], flags)
                .get_table()
                .get_or_create(indies[2], flags)
                .get_table()
        };

        assert!(!final_table[indies[3]].is_present());

        final_table[indies[3]] = unsafe { Entry::new(physical_address as u64, flags) };
    }

    // TODO: This code crash on real hardware?
    pub fn set_recursively(&mut self, flags: Flags) {
        unsafe {
            for entry in &mut self.0 {
                if !entry.is_present() {
                    continue;
                }

                entry.set_flags(flags);

                for entry in &mut entry.get_table().0 {
                    if !entry.is_present() {
                        continue;
                    }

                    entry.set_flags(flags);

                    for entry in &mut entry.get_table().0 {
                        if !entry.is_present() {
                            continue;
                        }

                        entry.set_flags(flags);

                        for entry in &mut entry.get_table().0 {
                            if !entry.is_present() {
                                continue;
                            }

                            entry.set_flags(flags);
                        }
                    }
                }
            }
        }
    }

    /// Get the table entry at the corresponding index or creates a one the
    /// given flags if it is not present.
    pub fn get_or_create(&mut self, index: usize, flags: Flags) -> &Entry {
        if !self.0[index].is_present() {
            let page = allocate_page(1);
            self.0[index] = unsafe { Entry::new(page as u64, flags) };
        }

        self.0[index].set_flags(flags);

        assert!(self.0[index].is_present());

        return &self.0[index];
    }
}

impl Index<usize> for Table {
    type Output = Entry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Table {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}
