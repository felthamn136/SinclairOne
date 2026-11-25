//! Adapt 128K paging and supplied I/O to the Z80 bus.

use sinclair_cpu_z80::Bus;
use sinclair_spectrum48::bus::SpectrumIo;

use crate::memory::Memory;

pub struct SpectrumBus<'a, I: SpectrumIo<Memory> + ?Sized> {
    memory: &'a mut Memory,
    io: &'a mut I,
}

impl<'a, I: SpectrumIo<Memory> + ?Sized> SpectrumBus<'a, I> {
    pub fn new(memory: &'a mut Memory, io: &'a mut I) -> Self {
        Self { memory, io }
    }
}

impl<I: SpectrumIo<Memory> + ?Sized> Bus for SpectrumBus<'_, I> {
    fn read_memory(&mut self, address: u16) -> u8 {
        self.memory.read(address)
    }

    fn write_memory(&mut self, address: u16, value: u8) {
        self.memory.write(address, value);
    }

    fn read_port(&mut self, port: u16) -> u8 {
        self.io.read_port(port)
    }

    fn write_port(&mut self, port: u16, value: u8) {
        self.memory.write_port(port, value);
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
