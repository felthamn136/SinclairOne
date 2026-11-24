use crate::{
    cpu::{parity, Index, C, H, N, PV, S, X, Y, Z, Z80},
    Bus,
};

impl Z80 {
    fn base_hl(&self, index: Option<Index>) -> u16 {
        match index {
            Some(Index::IX) => self.ix,
            Some(Index::IY) => self.iy,
            None => self.hl(),
        }
    }

    fn base_set_hl(&mut self, index: Option<Index>, value: u16) {
        match index {
            Some(Index::IX) => self.ix = value,
            Some(Index::IY) => self.iy = value,
            None => self.set_hl(value),
        }
    }

    fn base_rp(&self, p: u8, index: Option<Index>) -> u16 {
        match p {
            0 => self.bc(),
            1 => self.de(),
            2 => self.base_hl(index),
            3 => self.sp,
            _ => unreachable!(),
        }
    }

    fn base_set_rp(&mut self, p: u8, index: Option<Index>, value: u16) {
        match p {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.base_set_hl(index, value),
            3 => self.sp = value,
            _ => unreachable!(),
        }
    }

    fn base_rp2(&self, p: u8, index: Option<Index>) -> u16 {
        if p == 3 {
            self.af()
        } else {
            self.base_rp(p, index)
        }
    }

    fn base_set_rp2(&mut self, p: u8, index: Option<Index>, value: u16) {
        if p == 3 {
            self.set_af(value);
        } else {
            self.base_set_rp(p, index, value);
        }
    }

    // Code 6 is handled separately, because an indexed operand consumes a displacement.
    fn base_reg(&self, r: u8, index: Option<Index>) -> u8 {
        match r {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => (self.base_hl(index) >> 8) as u8,
            5 => self.base_hl(index) as u8,
            7 => self.a,
            _ => unreachable!(),
        }
    }

    fn base_set_reg(&mut self, r: u8, index: Option<Index>, value: u8) {
        match r {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => {
                let pair = self.base_hl(index);
                self.base_set_hl(index, (u16::from(value) << 8) | (pair & 0xff));
            }
            5 => {
                let pair = self.base_hl(index);
                self.base_set_hl(index, (pair & 0xff00) | u16::from(value));
            }
            7 => self.a = value,
            _ => unreachable!(),
        }
    }

    fn base_address(&mut self, bus: &mut impl Bus, index: Option<Index>) -> u16 {
        let address = self.base_hl(index);
        if index.is_some() {
            let displacement = self.fetch8(bus) as i8;
            address.wrapping_add(displacement as i16 as u16)
        } else {
            address
        }
    }

    fn base_condition(&self, y: u8) -> bool {
        match y {
            0 => self.f & Z == 0,
            1 => self.f & Z != 0,
            2 => self.f & C == 0,
            3 => self.f & C != 0,
            4 => self.f & PV == 0,
            5 => self.f & PV != 0,
            6 => self.f & S == 0,
            7 => self.f & S != 0,
            _ => unreachable!(),
        }
    }

    fn base_szxy(value: u8) -> u8 {
        (value & (S | Y | X)) | if value == 0 { Z } else { 0 }
    }

    fn base_szpxy(value: u8) -> u8 {
        Self::base_szxy(value) | if parity(value) { PV } else { 0 }
    }

    fn base_inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.f = (self.f & C)
            | Self::base_szxy(result)
            | if value & 0x0f == 0x0f { H } else { 0 }
            | if value == 0x7f { PV } else { 0 };
        result
    }

    fn base_dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.f = (self.f & C)
            | Self::base_szxy(result)
            | N
            | if value & 0x0f == 0 { H } else { 0 }
            | if value == 0x80 { PV } else { 0 };
        result
    }

    fn base_alu(&mut self, operation: u8, value: u8) {
        let a = self.a;
        match operation {
            0 | 1 => {
                let carry = u8::from(operation == 1 && self.f & C != 0);
                let wide = u16::from(a) + u16::from(value) + u16::from(carry);
                let result = wide as u8;
                self.f = Self::base_szxy(result)
                    | if (a ^ value ^ result) & 0x10 != 0 {
                        H
                    } else {
                        0
                    }
                    | if (!(a ^ value) & (a ^ result) & 0x80) != 0 {
                        PV
                    } else {
                        0
                    }
                    | if wide > 0xff { C } else { 0 };
                self.a = result;
            }
            2 | 3 | 7 => {
                let carry = u8::from(operation == 3 && self.f & C != 0);
                let result = a.wrapping_sub(value).wrapping_sub(carry);
                self.f = Self::base_szxy(result)
                    | N
                    | if (a ^ value ^ result) & 0x10 != 0 {
                        H
                    } else {
                        0
                    }
                    | if ((a ^ value) & (a ^ result) & 0x80) != 0 {
                        PV
                    } else {
                        0
                    }
                    | if u16::from(a) < u16::from(value) + u16::from(carry) {
                        C
                    } else {
                        0
                    };
                if operation == 7 {
                    self.f = (self.f & !(X | Y)) | (value & (X | Y));
                } else {
                    self.a = result;
                }
            }
            4 => {
                self.a &= value;
                self.f = Self::base_szpxy(self.a) | H;
            }
            5 => {
                self.a ^= value;
                self.f = Self::base_szpxy(self.a);
            }
            6 => {
                self.a |= value;
                self.f = Self::base_szpxy(self.a);
            }
            _ => unreachable!(),
        }
    }

    fn base_add_hl(&mut self, index: Option<Index>, value: u16) {
        let hl = self.base_hl(index);
        let result = hl.wrapping_add(value);
        self.f = (self.f & (S | Z | PV))
            | ((result >> 8) as u8 & (Y | X))
            | if (hl ^ value ^ result) & 0x1000 != 0 {
                H
            } else {
                0
            }
            | if u32::from(hl) + u32::from(value) > 0xffff {
                C
            } else {
                0
            };
        self.base_set_hl(index, result);
    }

    fn base_daa(&mut self) {
        let old = self.a;
        let subtract = self.f & N != 0;
        let mut correction = 0;
        let mut carry = self.f & C != 0;
        if self.f & H != 0 || old & 0x0f > 9 {
            correction |= 0x06;
        }
        if carry || old > 0x99 {
            correction |= 0x60;
            carry = true;
        }
        self.a = if subtract {
            old.wrapping_sub(correction)
        } else {
            old.wrapping_add(correction)
        };
        self.f = (self.f & N)
            | Self::base_szpxy(self.a)
            | if (old ^ self.a) & 0x10 != 0 { H } else { 0 }
            | if carry { C } else { 0 };
    }

    fn base_push(&mut self, bus: &mut impl Bus, value: u16) {
        self.sp = self.sp.wrapping_sub(1);
        bus.write_memory(self.sp, (value >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        bus.write_memory(self.sp, value as u8);
    }

    fn base_pop(&mut self, bus: &mut impl Bus) -> u16 {
        let low = bus.read_memory(self.sp);
        self.sp = self.sp.wrapping_add(1);
        let high = bus.read_memory(self.sp);
        self.sp = self.sp.wrapping_add(1);
        u16::from_le_bytes([low, high])
    }

    fn base_read16(bus: &mut impl Bus, address: u16) -> u16 {
        let low = bus.read_memory(address);
        let high = bus.read_memory(address.wrapping_add(1));
        u16::from_le_bytes([low, high])
    }

    fn base_write16(bus: &mut impl Bus, address: u16, value: u16) {
        let [low, high] = value.to_le_bytes();
        bus.write_memory(address, low);
        bus.write_memory(address.wrapping_add(1), high);
    }

    /// Execute a non-prefix opcode. DD/FD prefix time is accounted for by the caller.
    ///
    /// SCF/CCF use A for undocumented X/Y flags; the interface has no Q latch
    /// for the prior-instruction-dependent behavior of some Z80 variants.
    pub(crate) fn execute_base(
        &mut self,
        opcode: u8,
        bus: &mut impl Bus,
        index: Option<Index>,
    ) -> u32 {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let p = y >> 1;
        let q = y & 1;
        match x {
            0 => match z {
                0 => match y {
                    0 => 4,
                    1 => {
                        std::mem::swap(&mut self.a, &mut self.a_alt);
                        std::mem::swap(&mut self.f, &mut self.f_alt);
                        4
                    }
                    2 => {
                        let offset = self.fetch8(bus) as i8;
                        self.b = self.b.wrapping_sub(1);
                        if self.b != 0 {
                            self.pc = self.pc.wrapping_add(offset as i16 as u16);
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        let offset = self.fetch8(bus) as i8;
                        self.pc = self.pc.wrapping_add(offset as i16 as u16);
                        12
                    }
                    4..=7 => {
                        let offset = self.fetch8(bus) as i8;
                        if self.base_condition(y - 4) {
                            self.pc = self.pc.wrapping_add(offset as i16 as u16);
                            12
                        } else {
                            7
                        }
                    }
                    _ => unreachable!(),
                },
                1 => {
                    if q == 0 {
                        let value = self.fetch16(bus);
                        self.base_set_rp(p, index, value);
                        10
                    } else {
                        let value = self.base_rp(p, index);
                        self.base_add_hl(index, value);
                        11
                    }
                }
                2 => {
                    if q == 0 {
                        match p {
                            0 => bus.write_memory(self.bc(), self.a),
                            1 => bus.write_memory(self.de(), self.a),
                            2 => {
                                let address = self.fetch16(bus);
                                Self::base_write16(bus, address, self.base_hl(index));
                            }
                            3 => {
                                let address = self.fetch16(bus);
                                bus.write_memory(address, self.a);
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        match p {
                            0 => self.a = bus.read_memory(self.bc()),
                            1 => self.a = bus.read_memory(self.de()),
                            2 => {
                                let address = self.fetch16(bus);
                                let value = Self::base_read16(bus, address);
                                self.base_set_hl(index, value);
                            }
                            3 => {
                                let address = self.fetch16(bus);
                                self.a = bus.read_memory(address);
                            }
                            _ => unreachable!(),
                        }
                    }
                    match p {
                        0 | 1 => 7,
                        2 => 16,
                        3 => 13,
                        _ => unreachable!(),
                    }
                }
                3 => {
                    let value = self.base_rp(p, index);
                    let result = if q == 0 {
                        value.wrapping_add(1)
                    } else {
                        value.wrapping_sub(1)
                    };
                    self.base_set_rp(p, index, result);
                    6
                }
                4 | 5 => {
                    if y == 6 {
                        let address = self.base_address(bus, index);
                        let value = bus.read_memory(address);
                        let result = if z == 4 {
                            self.base_inc(value)
                        } else {
                            self.base_dec(value)
                        };
                        bus.write_memory(address, result);
                        if index.is_some() {
                            19
                        } else {
                            11
                        }
                    } else {
                        let value = self.base_reg(y, index);
                        let result = if z == 4 {
                            self.base_inc(value)
                        } else {
                            self.base_dec(value)
                        };
                        self.base_set_reg(y, index, result);
                        4
                    }
                }
                6 => {
                    if y == 6 {
                        let address = self.base_address(bus, index);
                        let value = self.fetch8(bus);
                        bus.write_memory(address, value);
                        if index.is_some() {
                            15
                        } else {
                            10
                        }
                    } else {
                        let value = self.fetch8(bus);
                        self.base_set_reg(y, index, value);
                        7
                    }
                }
                7 => {
                    match y {
                        0..=3 => {
                            let old = self.a;
                            let carry_in = u8::from(self.f & C != 0);
                            let (result, carry) = match y {
                                0 => (old.rotate_left(1), old >> 7),
                                1 => (old.rotate_right(1), old & 1),
                                2 => ((old << 1) | carry_in, old >> 7),
                                3 => ((old >> 1) | (carry_in << 7), old & 1),
                                _ => unreachable!(),
                            };
                            self.a = result;
                            self.f = (self.f & (S | Z | PV)) | (result & (Y | X)) | carry;
                        }
                        4 => self.base_daa(),
                        5 => {
                            self.a = !self.a;
                            self.f = (self.f & (S | Z | PV | C)) | (self.a & (Y | X)) | H | N;
                        }
                        6 => {
                            self.f = (self.f & (S | Z | PV)) | (self.a & (Y | X)) | C;
                        }
                        7 => {
                            let old_carry = self.f & C;
                            self.f = (self.f & (S | Z | PV))
                                | (self.a & (Y | X))
                                | if old_carry != 0 { H } else { C };
                        }
                        _ => unreachable!(),
                    }
                    4
                }
                _ => unreachable!(),
            },
            1 => {
                if y == 6 && z == 6 {
                    self.halted = true;
                    4
                } else if y == 6 || z == 6 {
                    let address = self.base_address(bus, index);
                    // H and L stay real H/L when the other operand is (IX/IY+d).
                    if z == 6 {
                        let value = bus.read_memory(address);
                        self.base_set_reg(y, None, value);
                    } else {
                        let value = self.base_reg(z, None);
                        bus.write_memory(address, value);
                    }
                    if index.is_some() {
                        15
                    } else {
                        7
                    }
                } else {
                    let value = self.base_reg(z, index);
                    self.base_set_reg(y, index, value);
                    4
                }
            }
            2 => {
                if z == 6 {
                    let address = self.base_address(bus, index);
                    let value = bus.read_memory(address);
                    self.base_alu(y, value);
                    if index.is_some() {
                        15
                    } else {
                        7
                    }
                } else {
                    let value = self.base_reg(z, index);
                    self.base_alu(y, value);
                    4
                }
            }
            3 => match z {
                0 => {
                    if self.base_condition(y) {
                        self.pc = self.base_pop(bus);
                        11
                    } else {
                        5
                    }
                }
                1 => {
                    if q == 0 {
                        let value = self.base_pop(bus);
                        self.base_set_rp2(p, index, value);
                        10
                    } else {
                        match p {
                            0 => {
                                self.pc = self.base_pop(bus);
                                10
                            }
                            1 => {
                                std::mem::swap(&mut self.b, &mut self.b_alt);
                                std::mem::swap(&mut self.c, &mut self.c_alt);
                                std::mem::swap(&mut self.d, &mut self.d_alt);
                                std::mem::swap(&mut self.e, &mut self.e_alt);
                                std::mem::swap(&mut self.h, &mut self.h_alt);
                                std::mem::swap(&mut self.l, &mut self.l_alt);
                                4
                            }
                            2 => {
                                self.pc = self.base_hl(index);
                                4
                            }
                            3 => {
                                self.sp = self.base_hl(index);
                                6
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                2 => {
                    let address = self.fetch16(bus);
                    if self.base_condition(y) {
                        self.pc = address;
                    }
                    10
                }
                3 => match y {
                    0 => {
                        self.pc = self.fetch16(bus);
                        10
                    }
                    1 => unreachable!("CB prefix must be decoded by the caller"),
                    2 => {
                        let low = self.fetch8(bus);
                        bus.write_port((u16::from(self.a) << 8) | u16::from(low), self.a);
                        11
                    }
                    3 => {
                        let low = self.fetch8(bus);
                        self.a = bus.read_port((u16::from(self.a) << 8) | u16::from(low));
                        11
                    }
                    4 => {
                        let value = Self::base_read16(bus, self.sp);
                        let old = self.base_hl(index);
                        bus.write_memory(self.sp.wrapping_add(1), (old >> 8) as u8);
                        bus.write_memory(self.sp, old as u8);
                        self.base_set_hl(index, value);
                        19
                    }
                    5 => {
                        let value = self.de();
                        self.set_de(self.hl());
                        self.set_hl(value);
                        4
                    }
                    6 => {
                        self.iff1 = false;
                        self.iff2 = false;
                        self.ei_delay = 0;
                        4
                    }
                    7 => {
                        self.iff1 = true;
                        self.iff2 = true;
                        self.ei_delay = 2;
                        4
                    }
                    _ => unreachable!(),
                },
                4 => {
                    let address = self.fetch16(bus);
                    if self.base_condition(y) {
                        self.base_push(bus, self.pc);
                        self.pc = address;
                        17
                    } else {
                        10
                    }
                }
                5 => {
                    if q == 0 {
                        let value = self.base_rp2(p, index);
                        self.base_push(bus, value);
                        11
                    } else if p == 0 {
                        let address = self.fetch16(bus);
                        self.base_push(bus, self.pc);
                        self.pc = address;
                        17
                    } else {
                        unreachable!("DD/ED/FD prefix must be decoded by the caller")
                    }
                }
                6 => {
                    let value = self.fetch8(bus);
                    self.base_alu(y, value);
                    7
                }
                7 => {
                    self.base_push(bus, self.pc);
                    self.pc = u16::from(y) * 8;
                    11
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBus {
        memory: [u8; 65536],
        port: u16,
        port_value: u8,
    }

    impl Default for TestBus {
        fn default() -> Self {
            Self {
                memory: [0; 65536],
                port: 0,
                port_value: 0,
            }
        }
    }

    impl Bus for TestBus {
        fn read_memory(&mut self, address: u16) -> u8 {
            self.memory[usize::from(address)]
        }
        fn write_memory(&mut self, address: u16, value: u8) {
            self.memory[usize::from(address)] = value;
        }
        fn read_port(&mut self, port: u16) -> u8 {
            self.port = port;
            self.port_value
        }
        fn write_port(&mut self, port: u16, value: u8) {
            self.port = port;
            self.port_value = value;
        }
    }

    #[test]
    fn indexed_memory_uses_real_hl_and_signed_displacement() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::default();
        cpu.ix = 0x1000;
        cpu.h = 0x5a;
        bus.memory[0] = 0xff;
        assert_eq!(cpu.execute_base(0x74, &mut bus, Some(Index::IX)), 15);
        assert_eq!(bus.memory[0x0fff], 0x5a);
        bus.memory[1] = 0xfe;
        bus.memory[0x0ffe] = 0xa5;
        assert_eq!(cpu.execute_base(0x6e, &mut bus, Some(Index::IX)), 15);
        assert_eq!(cpu.l, 0xa5);
        assert_eq!(cpu.ix, 0x1000);
        assert_eq!(cpu.execute_base(0x44, &mut bus, Some(Index::IX)), 4);
        assert_eq!(cpu.b, 0x10);
    }

    #[test]
    fn indexed_immediate_and_memory_increment_timing() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::default();
        cpu.iy = 0;
        bus.memory[0] = 0xff;
        bus.memory[1] = 0x7f;
        assert_eq!(cpu.execute_base(0x36, &mut bus, Some(Index::IY)), 15);
        assert_eq!(bus.memory[0xffff], 0x7f);
        bus.memory[2] = 0xff;
        cpu.f = C;
        assert_eq!(cpu.execute_base(0x34, &mut bus, Some(Index::IY)), 19);
        assert_eq!(bus.memory[0xffff], 0x80);
        assert_eq!(cpu.f, S | H | PV | C);
        assert_eq!(cpu.pc, 3);
    }

    #[test]
    fn alu_and_daa_flags() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::default();
        cpu.a = 0x7f;
        cpu.f = C;
        bus.memory[0] = 0;
        assert_eq!(cpu.execute_base(0xce, &mut bus, None), 7);
        assert_eq!((cpu.a, cpu.f), (0x80, S | H | PV));
        cpu.a = 0x28;
        bus.memory[1] = 0x28;
        cpu.execute_base(0xfe, &mut bus, None);
        assert_eq!(cpu.a, 0x28);
        assert_eq!(cpu.f, Z | N | Y | X);
        cpu.a = 0x9a;
        cpu.f = 0;
        cpu.execute_base(0x27, &mut bus, None);
        assert_eq!((cpu.a, cpu.f), (0, Z | H | PV | C));
        cpu.a = 0x0f;
        cpu.f = N | H;
        cpu.execute_base(0x27, &mut bus, None);
        assert_eq!((cpu.a, cpu.f), (9, N | X | PV));
        cpu.a = 0x9c;
        cpu.f = N;
        cpu.execute_base(0x27, &mut bus, None);
        assert_eq!((cpu.a, cpu.f), (0x36, Y | PV | N | C));
    }

    #[test]
    fn conditional_stack_exchange_and_interrupt_control() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::default();
        cpu.sp = 0;
        cpu.f = Z;
        bus.memory[0] = 0x34;
        bus.memory[1] = 0x12;
        assert_eq!(cpu.execute_base(0xc4, &mut bus, None), 10);
        assert_eq!((cpu.pc, cpu.sp), (2, 0));
        bus.memory[2] = 0x78;
        bus.memory[3] = 0x56;
        assert_eq!(cpu.execute_base(0xcc, &mut bus, None), 17);
        assert_eq!((cpu.pc, cpu.sp), (0x5678, 0xfffe));
        assert_eq!((bus.memory[0xfffe], bus.memory[0xffff]), (4, 0));
        cpu.ix = 0xabcd;
        assert_eq!(cpu.execute_base(0xe3, &mut bus, Some(Index::IX)), 19);
        assert_eq!(cpu.ix, 4);
        assert_eq!((bus.memory[0xfffe], bus.memory[0xffff]), (0xcd, 0xab));
        cpu.execute_base(0xfb, &mut bus, None);
        assert!(cpu.iff1 && cpu.iff2);
        assert_eq!(cpu.ei_delay, 2);
        cpu.execute_base(0xf3, &mut bus, None);
        assert!(!cpu.iff1 && !cpu.iff2);
        assert_eq!(cpu.ei_delay, 0);
    }

    #[test]
    fn immediate_ports_use_old_accumulator_as_high_byte() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::default();
        cpu.a = 0x12;
        cpu.f = 0xa5;
        bus.memory[0] = 0x34;
        bus.port_value = 0x56;
        assert_eq!(cpu.execute_base(0xdb, &mut bus, None), 11);
        assert_eq!((cpu.a, cpu.f, bus.port), (0x56, 0xa5, 0x1234));
        bus.memory[1] = 0x78;
        assert_eq!(cpu.execute_base(0xd3, &mut bus, None), 11);
        assert_eq!((bus.port, bus.port_value), (0x5678, 0x56));
    }
}
