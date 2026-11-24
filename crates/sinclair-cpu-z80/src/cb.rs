use crate::{
    cpu::{parity, C, H, N, PV, S, X, Y, Z, Z80},
    Bus,
};

impl Z80 {
    /// Execute a CB-prefixed instruction. The opcode and optional indexed
    /// effective address have already been fetched/calculated by the caller.
    ///
    /// Indexed BIT takes X/Y from the effective address. BIT (HL) approximates
    /// the undocumented internal WZ value with the address high byte.
    pub(crate) fn execute_cb(
        &mut self,
        opcode: u8,
        bus: &mut impl Bus,
        indexed_address: Option<u16>,
    ) -> u32 {
        let group = opcode >> 6;
        let bit = (opcode >> 3) & 7;
        let target = opcode & 7;

        if let Some(address) = indexed_address {
            let value = bus.read_memory(address);
            if group == 1 {
                self.bit_test(value, bit, (address >> 8) as u8);
            } else {
                let result = self.modify_cb_value(value, group, bit);
                bus.write_memory(address, result);
                if target != 6 {
                    self.write_register(target, result);
                }
            }
            return if group == 1 { 20 } else { 23 };
        }

        if target == 6 {
            let address = self.hl();
            let value = bus.read_memory(address);
            if group == 1 {
                self.bit_test(value, bit, (address >> 8) as u8);
                12
            } else {
                let result = self.modify_cb_value(value, group, bit);
                bus.write_memory(address, result);
                15
            }
        } else {
            let value = self.read_register(target);
            if group == 1 {
                self.bit_test(value, bit, value);
            } else {
                let result = self.modify_cb_value(value, group, bit);
                self.write_register(target, result);
            }
            8
        }
    }

    fn read_register(&self, register: u8) -> u8 {
        match register {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            7 => self.a,
            _ => unreachable!("(HL) is handled separately"),
        }
    }

    fn write_register(&mut self, register: u8, value: u8) {
        match register {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            7 => self.a = value,
            _ => unreachable!("(HL) is handled separately"),
        }
    }

    fn modify_cb_value(&mut self, value: u8, group: u8, bit: u8) -> u8 {
        match group {
            0 => {
                let old_carry = self.f & C != 0;
                let (result, carry) = match bit {
                    0 => (value.rotate_left(1), value & 0x80 != 0), // RLC
                    1 => (value.rotate_right(1), value & 0x01 != 0), // RRC
                    2 => ((value << 1) | u8::from(old_carry), value & 0x80 != 0), // RL
                    3 => ((value >> 1) | (u8::from(old_carry) << 7), value & 0x01 != 0), // RR
                    4 => (value << 1, value & 0x80 != 0),           // SLA
                    5 => (((value as i8) >> 1) as u8, value & 0x01 != 0), // SRA
                    6 => ((value << 1) | 1, value & 0x80 != 0),     // SLL (undocumented)
                    7 => (value >> 1, value & 0x01 != 0),           // SRL
                    _ => unreachable!(),
                };
                self.f = (result & (S | Y | X))
                    | if result == 0 { Z } else { 0 }
                    | if parity(result) { PV } else { 0 }
                    | if carry { C } else { 0 };
                result
            }
            2 => value & !(1 << bit),
            3 => value | (1 << bit),
            _ => unreachable!("BIT is handled separately"),
        }
    }

    fn bit_test(&mut self, value: u8, bit: u8, undocumented_xy: u8) {
        let set = value & (1 << bit) != 0;
        self.f = (self.f & C)
            | H
            | (undocumented_xy & (Y | X))
            | if !set { Z | PV } else { 0 }
            | if bit == 7 && set { S } else { 0 };
        // N is cleared by BIT; H is set above. Keeping the explicit mask here
        // also documents that no other previous flag survives this operation.
        self.f &= !N;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct TestBus {
        memory: HashMap<u16, u8>,
    }

    impl Bus for TestBus {
        fn read_memory(&mut self, address: u16) -> u8 {
            self.memory.get(&address).copied().unwrap_or(0)
        }

        fn write_memory(&mut self, address: u16, value: u8) {
            self.memory.insert(address, value);
        }

        fn read_port(&mut self, _port: u16) -> u8 {
            0
        }

        fn write_port(&mut self, _port: u16, _value: u8) {}
    }

    #[test]
    fn rotate_register_updates_result_flags_and_timing() {
        let mut cpu = Z80::default();
        let mut bus = TestBus::default();
        cpu.a = 0x81;

        assert_eq!(cpu.execute_cb(0x07, &mut bus, None), 8); // RLC A
        assert_eq!(cpu.a, 0x03);
        assert_eq!(cpu.f, C | PV);
    }

    #[test]
    fn indexed_rotate_copies_to_undocumented_register_destination() {
        let mut cpu = Z80::default();
        let mut bus = TestBus::default();
        let address = 0x1234;
        bus.memory.insert(address, 0x80);

        assert_eq!(cpu.execute_cb(0x00, &mut bus, Some(address)), 23); // RLC (IX+d),B
        assert_eq!(bus.read_memory(address), 0x01);
        assert_eq!(cpu.b, 0x01);
        assert_eq!(cpu.f, C);
    }

    #[test]
    fn indexed_bit_uses_address_xy_flags_and_does_not_write_memory() {
        let mut cpu = Z80::default();
        let mut bus = TestBus::default();
        let address = 0x2800;
        let value = 0x00;
        bus.memory.insert(address, value);
        cpu.f = C;

        assert_eq!(cpu.execute_cb(0x46, &mut bus, Some(address)), 20); // BIT 0,(IX+d)
        assert_eq!(bus.read_memory(address), value);
        assert_eq!(cpu.f, C | H | Z | PV | Y | X);
    }
}
