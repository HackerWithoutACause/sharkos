#![no_std]
#![no_main]
#![feature(
    const_mut_refs,
    abi_x86_interrupt,
    str_from_raw_parts,
    naked_functions,
    asm_const,
    allocator_api
)]
#![allow(clippy::upper_case_acronyms)]

extern crate alloc;

use core::arch::asm;

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use bootloader_api::{entry_point, BootInfo};

const KERNEL_START: u64 = 0xFFFF_8000_0000_0000;

#[macro_export]
macro_rules! println {
    () => (print!("\n"));
    ($($arg:tt)*) => (print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::_print(format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        unsafe {
            framebuffer::WRITER.force_unlock();
        }
        framebuffer::WRITER
            .lock()
            .as_mut()
            .and_then(|writer| writer.write_fmt(args).ok());
    });
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{}", info);
    hlt_loop();
}

use bootloader_api::config::{BootloaderConfig, Mapping};
use conquer_once::spin::OnceCell;
use spinning_top::Spinlock;
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;

use crate::allocator::allocate_page;
use crate::gdt::GDT;
use crate::paging::VirtualAllocator;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.mappings.dynamic_range_start = Some(KERNEL_START);
    config.mappings.dynamic_range_end = Some(u64::MAX);
    config
};

entry_point!(main, config = &BOOTLOADER_CONFIG);

mod allocator;
mod apic;
mod elf;
mod framebuffer;
mod gdt;
mod interrupts;
mod paging;
mod scheduling;

const CPUID_FEAT_EDX_APIC: u32 = 1 << 9;
static mut PHYSICAL_OFFSET: u64 = 0;

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
static KERNEL_OFFSET: conquer_once::spin::OnceCell<u64> = conquer_once::spin::OnceCell::uninit();

use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask, Star};

unsafe extern "C" fn system_call_handler(
    r1: u64,
    r2: u64,
    _r3: u64,
    _r4: u64,
    _r5: u64,
    _r6: u64,
    code: u64,
) {
    match code {
        0 => {
            println!("Process exited with code: {}!", r1);
            hlt_loop();
        }
        1 => unsafe {
            print!(
                "{}",
                core::str::from_raw_parts(r1 as *const u8, r2 as usize)
            );
        },
        _ => panic!("Unknown system call with code: {}", code),
    }
}

#[naked]
unsafe extern "C" fn dispatch_system_call() -> ! {
    asm!(
        "push rcx",
        "push rax",
        "mov rcx, r10",
        "call {}",
        "add rsp, 8",
        "pop rcx",
        "sysretq",
        sym system_call_handler,
        options(noreturn)
    )
}

fn initialize_usermode() {
    // Set up STAR, LSTAR, and SFMASK MSRs for sysret.
    // STAR: Segment selectors for sysret (user code and data segments).
    // LSTAR: System call target address (not used here but typically required for syscall setup).
    // SFMASK: RFLAGS mask.
    Star::write(
        GDT.1.user_code_selector,
        GDT.1.user_data_selector,
        GDT.1.code_selector,
        GDT.1.data_selector,
    )
    .unwrap();
    LStar::write(VirtAddr::new(dispatch_system_call as u64)); // Syscall target address, not relevant for sysret
    SFMask::write(RFlags::INTERRUPT_FLAG);

    // Enable system call extensions.
    unsafe {
        Efer::update(|flags| {
            *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
    }
}

unsafe fn _jump_usermode(user_rip: u64, stack_pointer: u64) {
    // Prepare registers for sysret.
    // rcx will be loaded into RIP (instruction pointer),
    // r11 will be loaded into RFLAGS.
    // Swap the stack pointer to the user stack.
    // rsp will be loaded with the value of user_rsp on sysret.
    asm!(
        "mov r11, 0x202",
        "mov rsp, r9",
        "swapgs",
        "sysretq",
        in("rcx") user_rip,
        in("r9") stack_pointer,
        options(noreturn)
    );
}

pub fn initialize(boot_info: &'static mut BootInfo) {
    unsafe {
        PHYSICAL_OFFSET = boot_info.physical_memory_offset.into_option().unwrap();
    }

    KERNEL_OFFSET.init_once(|| boot_info.kernel_image_offset);

    framebuffer::initialize(boot_info.framebuffer.as_mut().unwrap());
    gdt::init();
    interrupts::init_idt();

    let supports_apic = (unsafe { core::arch::x86_64::__cpuid(1) }.edx & CPUID_FEAT_EDX_APIC) != 0;
    assert!(supports_apic);

    allocator::initialize(&boot_info.memory_regions);

    let mut cr3: u64;
    unsafe {
        asm!("mov {x}, cr3", x = out(reg) cr3);
    }
    KERNEL_PAGE_TABLE.init_once(|| cr3 + boot_info.physical_memory_offset.into_option().unwrap());
    let page_table = unsafe {
        ((cr3 + boot_info.physical_memory_offset.into_option().unwrap()) as *mut paging::Table)
            .as_mut()
            .unwrap()
    };

    unsafe {
        let apic_address = 0xfee0_0000usize;
        page_table.create_mapping(apic_address, apic_address, paging::Flags::WRITE);
        apic::initialize(apic_address);
    }

    initialize_usermode();
}

static mut CORE_LOCAL: [Core; 1] = [Core::new()];

#[derive(Clone, Copy, Debug)]
struct Task(u64, usize);

impl From<&Process> for Task {
    fn from(process: &Process) -> Self {
        Self(process.elapsed, process.pid)
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Task {}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        other.0.cmp(&self.0)
    }
}

/// Storage for variables for each core
pub struct Core {
    thread_started: u64,
    current_thread: usize,
    queue: VecDeque<Task, VirtualAllocator>,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    pub fn local() -> &'static mut Self {
        unsafe { &mut CORE_LOCAL[coreid()] }
    }

    pub const fn new() -> Self {
        Core {
            thread_started: 0,
            current_thread: usize::MAX,
            queue: VecDeque::new_in(VirtualAllocator),
        }
    }
}

fn coreid() -> usize {
    unsafe { core::arch::x86_64::__cpuid(1).ebx as usize >> 24 }
}

#[repr(C)]
#[derive(Debug, Default)]
struct Context {
    rax: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    rsp: u64,
    rip: u64,
    eflags: u64,
}

#[derive(Debug)]
struct Process {
    pid: usize,
    /// If true then the process is returning from a syscall and can use sysretq rather than iretq
    fast_entry: bool,
    /// The number of clock cycles the process has used.
    elapsed: u64,
    /// The pointer to the level-4 page table entry for this process.
    cr3: u64,
    /// The saved registers for this processes.
    state: Context,
}

impl Process {
    fn load(elf: &[u8], stack_start: usize) {
        let table_ptr = paging::Table::new_copy();
        let page_table = unsafe { table_ptr.as_mut().unwrap() };

        unsafe {
            paging::Table::activate(table_ptr);
        }

        let address = elf::load_program(elf, page_table).unwrap();

        unsafe {
            page_table.create_mapping(stack_start, allocate_page(1), paging::Flags::ALL);
            page_table.create_mapping(stack_start + 4096, allocate_page(1), paging::Flags::ALL);
        }

        Process::launch(
            address,
            stack_start as u64 + 4096 + 4096,
            table_ptr as u64 - unsafe { PHYSICAL_OFFSET },
        );
    }

    fn launch(entry: u64, sp: u64, cr3: u64) {
        let mut processes = PROCESSES.lock();
        let pid = processes.len();

        processes.push(Process {
            pid,
            fast_entry: true,
            // TODO: When starting new task they should not have zero eleapsed
            // time to pervent from monopolizing the core.
            elapsed: 0,
            cr3,
            state: Context {
                rsp: sp,
                rip: entry,
                ..Context::default()
            },
        });

        Core::local().queue.push_back(Task::from(&processes[pid]));
    }
}

static PROCESSES: Spinlock<Vec<Process, VirtualAllocator>> =
    Spinlock::new(Vec::new_in(VirtualAllocator));

static KERNEL_PAGE_TABLE: OnceCell<u64> = OnceCell::uninit();

// TODO: Security:
// * Supervisor mode access / execution prevention.
// * User-mode instruction prevention.

#[no_mangle]
fn main(boot_info: &'static mut BootInfo) -> ! {
    initialize(boot_info);
    println!("Welcome to codename annarbor!");

    Process::load(include_bytes!("../../program.elf"), 0x1000_0000);
    Process::load(include_bytes!("../../program2.elf"), 0x1000_0000);

    unsafe {
        scheduling::switch_process();
    }
}

pub unsafe fn get_active_page_table() -> *mut paging::Table {
    let mut cr3: u64;
    unsafe {
        asm!("mov {x}, cr3", x = out(reg) cr3);
    }
    let page_table = unsafe {
        ((cr3 + PHYSICAL_OFFSET) as *mut paging::Table)
            .as_mut()
            .unwrap()
    };
    page_table
}
