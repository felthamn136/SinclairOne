/// Z80 memory, I/O and interrupt acknowledge operations.
///
/// Accesses occur in CPU order. The core returns instruction-level nominal
/// timing; this interface does not yet expose per-cycle timing or wait states.
pub trait Bus {
    fn read_memory(&mut self, address: u16) -> u8;
    fn write_memory(&mut self, address: u16, value: u8);
    fn read_port(&mut self, port: u16) -> u8;
    fn write_port(&mut self, port: u16, value: u8);

    /// Byte supplied by the interrupting device during interrupt acknowledge.
    fn interrupt_acknowledge(&mut self) -> u8 {
        0xff
    }

    /// Operand byte supplied by an IM0 device during an instruction fetch.
    /// The PC is held at `address` while the device supplies instruction bytes.
    fn interrupt_data(&mut self, address: u16) -> u8 {
        self.read_memory(address)
    }

    fn nmi_acknowledge(&mut self) {}

    /// Notify daisy-chain devices that a RETI has completed.
    fn reti(&mut self) {}
}
