use crate::{gdt::GDT, hlt_loop, println};
use conquer_once::spin::Lazy;
use core::arch::asm;
use pic8259::ChainedPics;
use spinning_top::Spinlock;
use x86_64::{
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
    VirtAddr,
};

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_code_selector(GDT.1.code_selector)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring0)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
    }

    unsafe {
        idt.general_protection_fault
            .set_handler_fn(general_protection)
            .set_code_selector(GDT.1.code_selector)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);

        idt.invalid_tss
            .set_handler_fn(invalid_tss_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.divide_error
            .set_handler_fn(divide_error)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.debug
            .set_handler_fn(breakpoint_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.non_maskable_interrupt
            .set_handler_fn(breakpoint_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.breakpoint
            .set_handler_fn(breakpoint_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.overflow
            .set_handler_fn(overflow_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.bound_range_exceeded
            .set_handler_fn(bound_range_exceeded)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.invalid_opcode
            .set_handler_fn(invalid_opcode_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.device_not_available
            .set_handler_fn(device_not_avaiable)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.segment_not_present
            .set_handler_fn(segment_no_present_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        idt.stack_segment_fault
            .set_handler_fn(stack_segment_fault_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);

        idt.page_fault
            .set_handler_fn(page_fault_handler)
            .set_code_selector(GDT.1.code_selector)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
    }

    unsafe {
        idt.slice_mut(InterruptIndex::Timer.as_u8()..=InterruptIndex::Timer.as_u8())[0]
            .set_handler_addr(VirtAddr::new(
                crate::scheduling::timer_interrupt_handler as u64,
            ))
            .set_code_selector(GDT.1.code_selector)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring0)
            .set_stack_index(crate::gdt::INTERRUPT_STACK_INDEX);
    }

    idt.slice_mut(40..=40)[0].set_handler_fn(error_vector);
    idt.slice_mut(48..=48)[0].set_handler_fn(spurious_vector);
    idt
});

extern "x86-interrupt" fn divide_error(stack_frame: InterruptStackFrame) {
    panic!("divide_error\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn overflow_handler(stack_frame: InterruptStackFrame) {
    panic!("overflow_handler\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn bound_range_exceeded(stack_frame: InterruptStackFrame) {
    panic!("bound_range_exceeded\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn device_not_avaiable(stack_frame: InterruptStackFrame) {
    panic!("device_not_avaiable\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "stack_segment_fault_handler: {}\n{:#?}",
        error_code, stack_frame
    );
}

extern "x86-interrupt" fn invalid_tss_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!("invalid_tss_handler: {}\n{:#?}", error_code, stack_frame);
}

extern "x86-interrupt" fn segment_no_present_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "segment_no_present_handler: {}\n{:#?}",
        error_code, stack_frame
    );
}

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    println!(
        "Invalid opcode at {:#0x}",
        stack_frame.instruction_pointer.as_u64()
    );

    unsafe {
        for (index, opcode) in (*(stack_frame.instruction_pointer.as_u64() as *const [u8; 16]))
            .iter()
            .enumerate()
        {
            if index > 0 {
                print!(" ")
            }
            print!("{:0x}", opcode);
        }

        println!();
    }

    hlt_loop()
}

extern "x86-interrupt" fn general_protection(stack_frame: InterruptStackFrame, error: u64) {
    panic!(
        "general prorection {} at {:#0x}\n{:#?}",
        error >> 3,
        stack_frame.instruction_pointer.as_u64(),
        stack_frame
    );
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    panic!(
        "EXCEPTION: DOUBLE FAULT {} at {:#0x}\n{:#?}",
        error_code,
        stack_frame.instruction_pointer.as_u64(),
        stack_frame
    );
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt_loop();
}

fn print_message() {
    println!("[!]");
}

extern "x86-interrupt" fn error_vector(_stack_frame: InterruptStackFrame) {
    unsafe { asm!("call {}", sym print_message) }
}

extern "x86-interrupt" fn spurious_vector(_stack_frame: InterruptStackFrame) {
    print!("1");

    unsafe {
        PICS.lock().notify_end_of_interrupt(48);
    }
}

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Spinlock<ChainedPics> =
    Spinlock::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
}
