// PLIC adresses for the QEMU virt machine

/*
   Each register is 4 bytes (u32).
   The PLIC is an external interrupt controller. The one used by QEMU virt is the same as the
   SiFive PLIC.
   https://sifive.cdn.prismic.io/sifive%2F834354f0-08e6-423c-bf1f-0cb58ef14061_fu540-c000-v1.0.pdf

   Ch. 10 explains the priority, pending, interrupt enable, threshold, and claims.

   The virt machine has the following external interrupts:
   0 -> 0 is the `null` interrupt
   VIRTIO = [1..8]
   UART0 = 10
   PCIE = [32..35]
*/
const PLIC_PRIO: usize = 0x0c00_0000;
const _PLIC_PENDING: usize = 0x0c00_1000; // could be used to see if interrupt is pending
const PLIC_ENABLE: usize = 0x0c00_2000;
const PLIC_THRESHOLD: usize = 0x0c20_0000;
const PLIC_CLAIM: usize = 0x0c20_0004;

/// Get the next available interrupt. This is the 'claim' process.
/// The PLIC will automatically sort by priority and hand the ID of the interrupt.
pub fn next() -> Option<u32> {
    let claim_reg = PLIC_CLAIM as *const u32;
    let claim_nbr;

    unsafe {
        claim_nbr = claim_reg.read_volatile();
    }
    if claim_nbr == 0 {
        None
    } else {
        Some(claim_nbr)
    }
}

/// Complete a pending interrupt by id
pub fn complete(id: u32) {
    let reg = PLIC_CLAIM as *mut u32;
    unsafe {
        // Write an u32 into the same register as the one we read from.
        // The PLIC can differentiate between a claim and a complete.
        reg.write_volatile(id);
    }
}

/// Set the global threshold. The threshold can be value [0..7].
/// The PLIC will mask any interrupts at or below the global threshold.
/// That means that a threshold of 7 will mask ALL interrupts and threshold 0 will allow ALL.
pub fn set_threshold(threshold: u8) {
    let actual_threshold = threshold & 7;
    let threshold_reg = PLIC_THRESHOLD as *mut u32;
    unsafe {
        threshold_reg.write_volatile(actual_threshold as u32);
    }
}

/// Enable a given interrupt id
pub fn enable(id: u32) {
    let enables = PLIC_ENABLE as *mut u32;
    let actual_id = 1 << id;
    unsafe {
        // PLIC_ENABLE is a bitfield register
        enables.write_volatile(enables.read_volatile() | actual_id);
    }
}

/// Set a given interrupt priority to the given priority.
/// The priority is a number between 0 and 7.
pub fn set_priority(id: u32, priority: u8) {
    let actual_prio = priority as u32 & 7;
    let prio_reg = PLIC_PRIO as *mut u32;
    unsafe {
        // Offset for a interrupt id is:
        // PLIC_PRIO + (id * 4)
        prio_reg.add(id as usize).write_volatile(actual_prio);
    }
}
