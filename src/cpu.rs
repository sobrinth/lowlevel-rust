#[repr(C)]
#[derive(Copy, Clone)]
pub struct TrapFrame {
    pub regs: [usize; 32],
    pub fregs: [usize; 32],
    pub satp: usize,
    pub trap_stack: *mut u8,
    pub hartid: usize,
}

impl TrapFrame {
    pub const fn zero() -> Self {
        Self {
            regs: [0; 32],
            fregs: [0; 32],
            satp: 0,
            trap_stack: 0 as *mut u8,
            hartid: 0,
        }
    }
}
