use sinclair_cpu_z80::Bus;
use sinclair_spectrum48::keyboard::KeyboardMatrix;

use crate::{memory::Memory, tape::Tape};

pub(crate) struct Zx81Bus<'a> {
    pub memory: &'a mut Memory,
    pub keyboard: &'a KeyboardMatrix,
    pub tape: &'a Option<Tape>,
    pub nmi_enabled: &'a mut bool,
    pub interrupt_acknowledged: bool,
}

impl Bus for Zx81Bus<'_> {
    fn read_memory(&mut self, address: u16) -> u8 {
        self.memory.read(address)
    }

    fn read_opcode(&mut self, address: u16) -> u8 {
        let value = self.memory.read(address);
        if address & 0x8000 != 0 && value & 0x40 == 0 {
            0
        } else {
            value
        }
    }

    fn write_memory(&mut self, address: u16, value: u8) {
        self.memory.write(address, value);
    }

    fn read_port(&mut self, port: u16) -> u8 {
        if port & 1 != 0 {
            return 0xff;
        }
        let ear = self.tape.as_ref().is_some_and(Tape::level);
        0x60 | self.keyboard.column_mask(port) | if ear { 0x80 } else { 0 }
    }

    fn write_port(&mut self, port: u16, _value: u8) {
        if port & 2 == 0 {
            *self.nmi_enabled = false;
        } else if port & 1 == 0 {
            *self.nmi_enabled = true;
        }
    }

    fn interrupt_acknowledge(&mut self) -> u8 {
        self.interrupt_acknowledged = true;
        0xff
    }
}
