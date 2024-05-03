use core::arch::asm;

use crate::{Context, Core, PROCESSES};

pub unsafe extern "C" fn current_context_address() -> *mut Context {
    crate::paging::Table::activate_kernel_table();

    &mut PROCESSES.lock()[Core::local().current_thread].state as *mut Context
}

#[naked]
pub unsafe extern "C" fn switch_to_userspace_slow(context: *mut Context, cr3: u64) -> ! {
    asm!(
        "mov rax, rdi",
        "push (3 * 8) | 3",
        "push [rax + 0x48]",
        "mov rbx, 0x202",
        "push rbx",
        "push (4 * 8) | 3",
        "push [rax + 0x50]",
        "mov rdi, [rax + 0x08]",
        "push [rax + 0x10]",
        "mov rdx, [rax + 0x18]",
        "mov rcx, [rax + 0x20]",
        "mov r8, [rax + 0x28]",
        "mov r9, [rax + 0x30]",
        "mov r10, [rax + 0x38]",
        "mov r11, [rax + 0x40]",
        "mov rax, [rax]",
        "mov cr3, rsi",
        "pop rsi",
        "iretq",
        options(noreturn)
    )
}

pub unsafe extern "C" fn switch_process() -> ! {
    let core = Core::local();
    let (context, cr3) = {
        let mut processes = PROCESSES.lock();
        let next_process = pick_next(core);

        core.current_thread = next_process;

        crate::apic::end_of_interrupt();
        (
            &mut processes[next_process].state as *mut Context,
            processes[next_process].cr3,
        )
    };

    switch_to_userspace_slow(context, cr3)
}

pub unsafe extern "C" fn requeue_active_process() {
    let core = Core::local();
    let mut processes = PROCESSES.lock();

    processes[core.current_thread].fast_entry = false;
    processes[core.current_thread].elapsed += core::arch::x86_64::_rdtsc() - core.thread_started;

    let task = crate::Task::from(&processes[core.current_thread]);

    core.queue.push_back(task);
}

fn pick_next(core: &mut Core) -> usize {
    core.queue.pop_front().unwrap().1
}

#[naked]
pub unsafe extern "C" fn timer_interrupt_handler() -> ! {
    asm!(
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push rax",

        "call {current_context_address}",

        "pop rdi",
        "mov [rax], rdi",

        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",

        "mov [rax + 0x08], rdi",
        "mov [rax + 0x10], rsi",
        "mov [rax + 0x18], rdx",
        "mov [rax + 0x20], rcx",
        "mov [rax + 0x28], r8",
        "mov [rax + 0x30], r9",
        "mov [rax + 0x38], r10",
        "mov [rax + 0x40], r11",

        "pop rdi",
        "mov [rax + 0x50], rdi",

        // "add rsp, 0x18",
        "pop rbx",
        "mov [rax + 0x58], rbx",
        "pop rbx",
        "pop rdi",
        "mov [rax + 0x48], rdi",

        "call {requeue_active_process}",
        "call {switch_process}",
        requeue_active_process = sym requeue_active_process,
        current_context_address = sym crate::scheduling::current_context_address,
        switch_process = sym crate::scheduling::switch_process,
        options(noreturn)
    )
}
