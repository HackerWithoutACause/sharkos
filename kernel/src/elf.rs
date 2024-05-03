use crate::{allocator::allocate_page, paging};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ELFError {
    Missing,
    WrongMagic,
    WrongMachine,
    WrongType,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: u32,
    pub bitsize: u8,
    pub endian: u8,
    pub ident_abi_version: u8,
    pub target_platform: u8,
    pub abi_version: u8,
    pub padding: [u8; 7],
    pub obj_type: u16,
    // 0xf3 for RISC-V 0x3e for x86_64
    pub machine: u16,
    pub version: u32,
    pub entry_addr: usize,
    pub program_header_offset: usize,
    pub shoff: usize,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

const MAGIC: u32 = 0x464c457f;
const X86_64: u16 = 0x3e;

const EXECUTABLE: u16 = 2;

const PH_SEG_TYPE_LOAD: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramHeader {
    pub seg_type: u32,
    pub flags: u32,
    pub off: usize,
    pub vaddr: usize,
    pub paddr: usize,
    pub filesz: usize,
    pub memsz: usize,
    pub align: usize,
}

pub fn load_program(buffer: &[u8], page_table: &mut paging::Table) -> Result<u64, ELFError> {
    if buffer.len() < core::mem::size_of::<Header>() {
        return Err(ELFError::Missing);
    }

    let header = unsafe { (buffer.as_ptr() as *const Header).as_ref().unwrap() };

    if header.magic != MAGIC {
        return Err(ELFError::WrongMagic);
    }

    if header.machine != X86_64 {
        return Err(ELFError::WrongMachine);
    }

    if header.obj_type != EXECUTABLE {
        return Err(ELFError::WrongType);
    }

    let ph_tab =
        unsafe { buffer.as_ptr().add(header.program_header_offset) as *const ProgramHeader };

    for i in 0..header.phnum as usize {
        let program_header = unsafe { ph_tab.add(i).as_ref().unwrap() };

        if program_header.seg_type != PH_SEG_TYPE_LOAD {
            continue;
        }

        if program_header.memsz == 0 {
            continue;
        }

        let page_count = (program_header.vaddr - (program_header.vaddr) / 4096 * 4096
            + program_header.memsz
            + 4095)
            / 4096;
        let page = allocate_page(page_count as u64);

        unsafe {
            let start = (program_header.vaddr / 4096) * 4096;
            for i in 0..page_count {
                // TODO: Map page with proper flags.
                page_table.create_mapping(start + i * 4096, page + i * 4096, paging::Flags::ALL);
            }

            core::ptr::copy_nonoverlapping(
                buffer.as_ptr().add(program_header.off),
                program_header.vaddr as *mut u8,
                program_header.memsz,
            );
        }
    }

    println!("Done mapping elf!");

    unsafe { paging::Table::activate_kernel_table() };

    Ok(header.entry_addr as u64)
}
