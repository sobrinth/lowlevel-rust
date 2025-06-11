use core::fmt::Write;

pub struct Uart {
    base_address: usize,
}

impl Uart {
    pub fn new(base_address: usize) -> Self {
        Self { base_address }
    }

    pub fn init(&self) {
        let bp = self.base_address as *mut u8;
        unsafe {
            /*
                First, set the word length, which are bits 0 and 1 of the line control register (LCR).
                The LCR is located at offset 0x3 of the UART base address.
            // */
            let lcr: u8 = (1 << 0) | (1 << 1);
            bp.add(0x3).write_volatile(lcr);

            // Enable FIFO in the FiFO control register (FCR at offset 0x2)
            bp.add(0x2).write_volatile(1 << 0);

            // Enable receiver buffer interrupts (IER at offset 0x1)
            bp.add(0x1).write_volatile(1 << 0);

            /*
               If we cared about the divisor, the code below would set the divisor from a global clock
               rate of 22.729 MHz to a signaling rate of 2400 baud. We usually have much faster signaling
               rates nowadays, but this demonstrates what the divisor actually does.
               The formula given in the NS16500A Specification for calculating the divisor is:
               divisor = ceil((clock_hz) / (baud_sps x 16))
               So with our values we get 592

               The divisor register is two bytes, so we split the value. Typically, the divisor would
               be calculated based on measuring the clock rate, but for qemu this doesn't really do anything.
            */
            let divisor: u16 = 592;
            let divisor_lsb: u8 = (divisor & 0xFF) as u8;
            let divisor_msb: u8 = ((divisor >> 8) & 0xFF) as u8;

            /*
               The divisor registers (DLL and DLM) have the same base address as the RX/TX and IE registers.
               To change what the base address points to, we open the "divisor latch" by writing 1 into the DLAB,
               which is bit 7 of the LCR.
            */
            let lcr = bp.add(0x3).read_volatile();
            bp.add(0x3).write_volatile(lcr | (1 << 7));

            // Now the base address 0 and 1 point to DLL and DLM
            bp.add(0x0).write_volatile(divisor_lsb);
            bp.add(0x1).write_volatile(divisor_msb);

            // Close the "divisor latch" by writing 0 into the DLAB
            bp.add(0x3).write_volatile(lcr)
        }
    }
    
    pub fn get(&self) -> Option<u8> {
        let bp = self.base_address as *const u8;
        unsafe {
            if bp.add(0x5).read_volatile() & 1 == 0 {
                // The DR bit is 0, meaning no data is available
                None
            } else { 
               Some(bp.add(0x0).read_volatile()) 
            }
        }
    }
    
    pub fn put(&self, byte: u8) {
        let bp = self.base_address as *mut u8;
        unsafe {
            bp.add(0x0).write_volatile(byte);
        }
    }
}

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            self.put(byte);
        }
        Ok(())
    }
}
