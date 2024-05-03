struct APIC(usize);

impl APIC {
    const LVT_TIMER: usize = 0x320;
    const TICR: usize = 0x380;
    const SIVR: usize = 0x0F0;
    const EOI: usize = 0x0B0;

    unsafe fn write_register(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.0 + offset) as *mut u32, value)
    }

    unsafe fn read_register(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.0 + offset) as *mut u32)
    }
}

pub fn enable_timer_interrupts() {
    unsafe {
        let apic = APIC(0xfee0_0000usize);
        apic.write_register(
            APIC::LVT_TIMER,
            apic.read_register(APIC::LVT_TIMER) & !(1 << 16),
        );
    }
}

pub fn disable_timer_interrupts() {
    unsafe {
        let apic = APIC(0xfee0_0000usize);
        apic.write_register(
            APIC::LVT_TIMER,
            apic.read_register(APIC::LVT_TIMER) | 1 << 16,
        );
    }
}

/// Local APIC timer modes.
#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum TimerMode {
    /// Timer only fires once.
    OneShot = 0b00,
    /// Timer fires periodically.
    Periodic = 0b01,
    /// Timer fires at an absolute time.
    TscDeadline = 0b10,
}

pub fn end_of_interrupt() {
    unsafe {
        APIC(0xfee0_0000usize).write_register(APIC::EOI, 0);
    }
}

pub unsafe fn initialize(apic_address: usize) {
    let apic = APIC(apic_address);
    apic.write_register(APIC::LVT_TIMER, 32 | ((TimerMode::Periodic as u32) << 17));
    apic.write_register(APIC::TICR, 10_000_000);
    // apic.write_register(APIC::SIVR, 1 << 16);
}
