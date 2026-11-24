use crate::{
    cpu::{parity, InterruptMode, C, H, N, PV, S, X, Y, Z, Z80},
    Bus,
};

fn sz53(value: u8) -> u8 {
    (value & (S | Y | X)) | if value == 0 { Z } else { 0 }
}

fn sz53p(value: u8) -> u8 {
    sz53(value) | if parity(value) { PV } else { 0 }
}

fn pair(cpu: &Z80, code: u8) -> u16 {
    match code & 3 {
        0 => cpu.bc(),
        1 => cpu.de(),
        2 => cpu.hl(),
        _ => cpu.sp,
    }
}

fn set_pair(cpu: &mut Z80, code: u8, value: u16) {
    match code & 3 {
        0 => cpu.set_bc(value),
        1 => cpu.set_de(value),
        2 => cpu.set_hl(value),
        _ => cpu.sp = value,
    }
}

fn read_register(cpu: &Z80, code: u8) -> u8 {
    match code & 7 {
        0 => cpu.b,
        1 => cpu.c,
        2 => cpu.d,
        3 => cpu.e,
        4 => cpu.h,
        5 => cpu.l,
        7 => cpu.a,
        _ => unreachable!("(HL) is handled separately"),
    }
}

fn write_register(cpu: &mut Z80, code: u8, value: u8) {
    match code & 7 {
        0 => cpu.b = value,
        1 => cpu.c = value,
        2 => cpu.d = value,
        3 => cpu.e = value,
        4 => cpu.h = value,
        5 => cpu.l = value,
        7 => cpu.a = value,
        _ => unreachable!("(HL) is handled separately"),
    }
}

impl Z80 {
    /// Execute an ED-prefixed instruction. `opcode` is the second byte, which
    /// has already been fetched (and counted as an R-register increment).
    /// Returned timings include both bytes of the ED instruction.
    pub(crate) fn execute_ed(&mut self, opcode: u8, bus: &mut impl Bus) -> u32 {
        match opcode {
            // IN r,(C), including the undocumented IN (C) form.
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                let value = bus.read_port(self.bc());
                if opcode != 0x70 {
                    write_register(self, opcode >> 3, value);
                }
                self.f = (self.f & C) | sz53(value) | if parity(value) { PV } else { 0 };
                12
            }

            // OUT (C),r and the undocumented OUT (C),0 encoding.
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => {
                let value = if opcode == 0x71 {
                    0
                } else {
                    read_register(self, opcode >> 3)
                };
                bus.write_port(self.bc(), value);
                12
            }

            // SBC HL,rr
            0x42 | 0x52 | 0x62 | 0x72 => {
                let left = self.hl();
                let right = pair(self, opcode >> 4);
                let carry = u32::from(self.f & C != 0);
                let result = (left as u32).wrapping_sub(right as u32).wrapping_sub(carry);
                let value = result as u16;
                let rhs = right as u32 + carry;
                let overflow = ((left ^ right) & (left ^ value) & 0x8000) != 0;
                let half_borrow = ((left ^ right ^ value) & 0x1000) != 0;
                self.set_hl(value);
                self.f = ((value >> 8) as u8 & (S | Y | X))
                    | if value == 0 { Z } else { 0 }
                    | if half_borrow { H } else { 0 }
                    | if overflow { PV } else { 0 }
                    | N
                    | if (left as u32) < rhs { C } else { 0 };
                15
            }

            // ADC HL,rr
            0x4a | 0x5a | 0x6a | 0x7a => {
                let left = self.hl();
                let right = pair(self, opcode >> 4);
                let carry = u32::from(self.f & C != 0);
                let result = left as u32 + right as u32 + carry;
                let value = result as u16;
                let overflow = (!(left ^ right) & (left ^ value) & 0x8000) != 0;
                let half_carry = ((left ^ right ^ value) & 0x1000) != 0;
                self.set_hl(value);
                self.f = ((value >> 8) as u8 & (S | Y | X))
                    | if value == 0 { Z } else { 0 }
                    | if half_carry { H } else { 0 }
                    | if overflow { PV } else { 0 }
                    | if result > 0xffff { C } else { 0 };
                15
            }

            // LD (nn),rr
            0x43 | 0x53 | 0x63 | 0x73 => {
                let address = self.fetch16(bus);
                let value = pair(self, opcode >> 4);
                bus.write_memory(address, value as u8);
                bus.write_memory(address.wrapping_add(1), (value >> 8) as u8);
                20
            }

            // LD rr,(nn)
            0x4b | 0x5b | 0x6b | 0x7b => {
                let address = self.fetch16(bus);
                let low = bus.read_memory(address) as u16;
                let high = bus.read_memory(address.wrapping_add(1)) as u16;
                set_pair(self, opcode >> 4, low | (high << 8));
                20
            }

            // NEG and its aliases.
            0x44 | 0x4c | 0x54 | 0x5c | 0x64 | 0x6c | 0x74 | 0x7c => {
                let old = self.a;
                let value = 0u8.wrapping_sub(old);
                self.a = value;
                self.f = sz53(value)
                    | if old & 0x0f != 0 { H } else { 0 }
                    | if old == 0x80 { PV } else { 0 }
                    | N
                    | if old != 0 { C } else { 0 };
                8
            }

            // RETN aliases and RETI.
            0x45 | 0x55 | 0x5d | 0x65 | 0x6d | 0x75 | 0x7d | 0x4d => {
                let low = bus.read_memory(self.sp) as u16;
                self.sp = self.sp.wrapping_add(1);
                let high = bus.read_memory(self.sp) as u16;
                self.sp = self.sp.wrapping_add(1);
                self.pc = low | (high << 8);
                self.iff1 = self.iff2;
                if opcode == 0x4d {
                    bus.reti();
                }
                14
            }

            // IM 0/1/2 documented encodings.
            0x46 | 0x4e | 0x66 | 0x6e => {
                self.im = InterruptMode::Mode0;
                8
            }
            0x56 | 0x76 => {
                self.im = InterruptMode::Mode1;
                8
            }
            0x5e | 0x7e => {
                self.im = InterruptMode::Mode2;
                8
            }

            0x47 => {
                self.i = self.a;
                9
            }
            0x4f => {
                self.r = self.a;
                9
            }
            0x57 => {
                self.a = self.i;
                self.f = (self.f & C) | sz53(self.a) | if self.iff2 { PV } else { 0 };
                9
            }
            0x5f => {
                self.a = self.r;
                self.f = (self.f & C) | sz53(self.a) | if self.iff2 { PV } else { 0 };
                9
            }

            // RRD
            0x67 => {
                let address = self.hl();
                let memory = bus.read_memory(address);
                let new_memory = (self.a << 4) | (memory >> 4);
                self.a = (self.a & 0xf0) | (memory & 0x0f);
                bus.write_memory(address, new_memory);
                self.f = (self.f & C) | sz53p(self.a);
                18
            }

            // RLD
            0x6f => {
                let address = self.hl();
                let memory = bus.read_memory(address);
                let new_memory = (memory << 4) | (self.a & 0x0f);
                self.a = (self.a & 0xf0) | (memory >> 4);
                bus.write_memory(address, new_memory);
                self.f = (self.f & C) | sz53p(self.a);
                18
            }

            // LDI, LDIR, LDD, LDDR
            0xa0 | 0xb0 | 0xa8 | 0xb8 => {
                let decrement = opcode & 0x08 != 0;
                let value = bus.read_memory(self.hl());
                bus.write_memory(self.de(), value);
                let delta = if decrement { u16::MAX } else { 1 };
                self.set_hl(self.hl().wrapping_add(delta));
                self.set_de(self.de().wrapping_add(delta));
                self.set_bc(self.bc().wrapping_sub(1));

                let sum = self.a.wrapping_add(value);
                let undocumented = (sum & X) | ((sum & 0x02) << 4);
                self.f =
                    (self.f & (S | Z | C)) | undocumented | if self.bc() != 0 { PV } else { 0 };

                let repeating = opcode & 0x10 != 0 && self.bc() != 0;
                if repeating {
                    self.pc = self.pc.wrapping_sub(2);
                    21
                } else {
                    16
                }
            }

            // CPI, CPIR, CPD, CPDR
            0xa1 | 0xb1 | 0xa9 | 0xb9 => {
                let decrement = opcode & 0x08 != 0;
                let value = bus.read_memory(self.hl());
                let result = self.a.wrapping_sub(value);
                let half_borrow = (self.a & 0x0f) < (value & 0x0f);
                self.set_hl(self.hl().wrapping_add(if decrement { u16::MAX } else { 1 }));
                self.set_bc(self.bc().wrapping_sub(1));

                let adjusted = result.wrapping_sub(u8::from(half_borrow));
                self.f = (self.f & C)
                    | (result & S)
                    | if result == 0 { Z } else { 0 }
                    | if half_borrow { H } else { 0 }
                    | if self.bc() != 0 { PV } else { 0 }
                    | N
                    | (adjusted & X)
                    | ((adjusted & 0x02) << 4);

                let repeating = opcode & 0x10 != 0 && self.bc() != 0 && result != 0;
                if repeating {
                    self.pc = self.pc.wrapping_sub(2);
                    21
                } else {
                    16
                }
            }

            // INI, INIR, IND, INDR
            0xa2 | 0xb2 | 0xaa | 0xba => {
                let decrement = opcode & 0x08 != 0;
                let direction = if decrement { -1i16 } else { 1i16 };
                let value = bus.read_port(self.bc());
                bus.write_memory(self.hl(), value);
                self.set_hl(self.hl().wrapping_add(if decrement { u16::MAX } else { 1 }));
                self.b = self.b.wrapping_sub(1);

                let port_adjustment = self.c.wrapping_add(direction as i8 as u8);
                let sum = value as u16 + port_adjustment as u16;
                self.f = block_io_flags(self.b, value, sum);

                let repeating = opcode & 0x10 != 0 && self.b != 0;
                if repeating {
                    self.pc = self.pc.wrapping_sub(2);
                    21
                } else {
                    16
                }
            }

            // OUTI, OTIR, OUTD, OTDR
            0xa3 | 0xb3 | 0xab | 0xbb => {
                let decrement = opcode & 0x08 != 0;
                let value = bus.read_memory(self.hl());
                self.set_hl(self.hl().wrapping_add(if decrement { u16::MAX } else { 1 }));
                self.b = self.b.wrapping_sub(1);
                bus.write_port(self.bc(), value);

                let sum = value as u16 + self.l as u16;
                self.f = block_io_flags(self.b, value, sum);

                let repeating = opcode & 0x10 != 0 && self.b != 0;
                if repeating {
                    self.pc = self.pc.wrapping_sub(2);
                    21
                } else {
                    16
                }
            }

            // Every other ED encoding is an eight-T-state NOP.
            _ => 8,
        }
    }
}

fn block_io_flags(b: u8, value: u8, sum: u16) -> u8 {
    sz53(b)
        | if value & 0x80 != 0 { N } else { 0 }
        | if sum > 0xff { H | C } else { 0 }
        | if parity(((sum as u8) & 7) ^ b) { PV } else { 0 }
}
