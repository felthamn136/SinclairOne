//! Adapt the 48K memory map and an explicitly supplied I/O device to the Z80.

use sinclair_cpu_z80::Bus;

use crate::memory::Memory;

/// Machine-owned port and interrupt behavior. No I/O hardware is assumed here.
pub trait SpectrumIo {
    fn read_port(&mut self, port: u16) -> u8;
    fn write_port(&mut self, port: u16, value: u8);
    fn interrupt_acknowledge(&mut self) -> u8;
    fn interrupt_data(&mut self, address: u16, memory: &mut Memory) -> u8;
    fn nmi_acknowledge(&mut self);
    fn reti(&mut self);
}

/// Borrows memory and I/O for CPU execution without taking ownership of either.
pub struct SpectrumBus<'a, I: SpectrumIo + ?Sized> {
    memory: &'a mut Memory,
    io: &'a mut I,
}

impl<'a, I: SpectrumIo + ?Sized> SpectrumBus<'a, I> {
    pub fn new(memory: &'a mut Memory, io: &'a mut I) -> Self {
        Self { memory, io }
    }
}

impl<I: SpectrumIo + ?Sized> Bus for SpectrumBus<'_, I> {
    #[inline]
    fn read_memory(&mut self, address: u16) -> u8 {
        self.memory.read(address)
    }

    #[inline]
    fn write_memory(&mut self, address: u16, value: u8) {
        self.memory.write(address, value);
    }

    fn read_port(&mut self, port: u16) -> u8 {
        self.io.read_port(port)
    }

    fn write_port(&mut self, port: u16, value: u8) {
        self.io.write_port(port, value);
    }

    fn interrupt_acknowledge(&mut self) -> u8 {
        self.io.interrupt_acknowledge()
    }

    fn interrupt_data(&mut self, address: u16) -> u8 {
        self.io.interrupt_data(address, self.memory)
    }

    fn nmi_acknowledge(&mut self) {
        self.io.nmi_acknowledge();
    }

    fn reti(&mut self) {
        self.io.reti();
    }
}
