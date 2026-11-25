use crate::Bus;

pub(crate) const S: u8 = 0x80;
pub(crate) const Z: u8 = 0x40;
pub(crate) const Y: u8 = 0x20;
pub(crate) const H: u8 = 0x10;
pub(crate) const X: u8 = 0x08;
pub(crate) const PV: u8 = 0x04;
pub(crate) const N: u8 = 0x02;
pub(crate) const C: u8 = 0x01;

pub(crate) fn parity(value: u8) -> bool {
    value.count_ones() & 1 == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InterruptMode {
    #[default]
    Mode0,
    Mode1,
    Mode2,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Index {
    IX,
    IY,
}

/// Z80 programmer-visible registers and interrupt state.
///
/// `new` initializes unspecified power-on registers to zero for determinism;
/// real hardware does not guarantee those register values.
#[derive(Debug, Clone, Default)]
pub struct Z80 {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub a_alt: u8,
    pub f_alt: u8,
    pub b_alt: u8,
    pub c_alt: u8,
    pub d_alt: u8,
    pub e_alt: u8,
    pub h_alt: u8,
    pub l_alt: u8,
    pub ix: u16,
    pub iy: u16,
    pub pc: u16,
    pub sp: u16,
    pub i: u8,
    pub r: u8,
    pub iff1: bool,
    pub iff2: bool,
    pub im: InterruptMode,
    pub halted: bool,
    pub(crate) ei_delay: u8,
    irq: bool,
    nmi_pending: bool,
    im0_active: bool,
    im0_extra_cycles: u32,
}

pub const STATE_SIZE: usize = 38;

impl Z80 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset control state. General-purpose registers and SP are unspecified
    /// by hardware reset and retain their previous values here.
    pub fn reset(&mut self) {
        self.pc = 0;
        self.i = 0;
        self.r = 0;
        self.iff1 = false;
        self.iff2 = false;
        self.im = InterruptMode::Mode0;
        self.halted = false;
        self.ei_delay = 0;
        self.nmi_pending = false;
        self.im0_active = false;
        self.im0_extra_cycles = 0;
    }

    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }
    pub fn set_af(&mut self, value: u16) {
        [self.a, self.f] = value.to_be_bytes();
    }
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn set_bc(&mut self, value: u16) {
        [self.b, self.c] = value.to_be_bytes();
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn set_de(&mut self, value: u16) {
        [self.d, self.e] = value.to_be_bytes();
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn set_hl(&mut self, value: u16) {
        [self.h, self.l] = value.to_be_bytes();
    }
    pub fn af_alt(&self) -> u16 {
        u16::from_be_bytes([self.a_alt, self.f_alt])
    }
    pub fn set_af_alt(&mut self, value: u16) {
        [self.a_alt, self.f_alt] = value.to_be_bytes();
    }
    pub fn bc_alt(&self) -> u16 {
        u16::from_be_bytes([self.b_alt, self.c_alt])
    }
    pub fn set_bc_alt(&mut self, value: u16) {
        [self.b_alt, self.c_alt] = value.to_be_bytes();
    }
    pub fn de_alt(&self) -> u16 {
        u16::from_be_bytes([self.d_alt, self.e_alt])
    }
    pub fn set_de_alt(&mut self, value: u16) {
        [self.d_alt, self.e_alt] = value.to_be_bytes();
    }
    pub fn hl_alt(&self) -> u16 {
        u16::from_be_bytes([self.h_alt, self.l_alt])
    }
    pub fn set_hl_alt(&mut self, value: u16) {
        [self.h_alt, self.l_alt] = value.to_be_bytes();
    }

    /// Set the level-sensitive maskable interrupt input.
    pub fn set_irq(&mut self, asserted: bool) {
        self.irq = asserted;
    }

    /// Latch an NMI edge for the next instruction boundary.
    pub fn request_nmi(&mut self) {
        self.nmi_pending = true;
    }

    /// Whether interrupt inputs or the post-EI delay are awaiting execution.
    pub fn has_pending_interrupt_state(&self) -> bool {
        self.irq || self.nmi_pending || self.ei_delay != 0
    }

    /// Encode complete CPU state at an instruction boundary.
    pub fn state_bytes(&self) -> [u8; STATE_SIZE] {
        let mut bytes = [0; STATE_SIZE];
        bytes[..16].copy_from_slice(&[
            self.a, self.f, self.b, self.c, self.d, self.e, self.h, self.l, self.a_alt, self.f_alt,
            self.b_alt, self.c_alt, self.d_alt, self.e_alt, self.h_alt, self.l_alt,
        ]);
        for (offset, value) in [(16, self.ix), (18, self.iy), (20, self.pc), (22, self.sp)] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[24..26].copy_from_slice(&[self.i, self.r]);
        bytes[26..32].copy_from_slice(&[
            self.iff1 as u8,
            self.iff2 as u8,
            self.halted as u8,
            self.irq as u8,
            self.nmi_pending as u8,
            self.im0_active as u8,
        ]);
        bytes[32] = match self.im {
            InterruptMode::Mode0 => 0,
            InterruptMode::Mode1 => 1,
            InterruptMode::Mode2 => 2,
        };
        bytes[33] = self.ei_delay;
        bytes[34..38].copy_from_slice(&self.im0_extra_cycles.to_le_bytes());
        bytes
    }

    /// Decode CPU state; an active IM0 instruction is not an instruction boundary.
    pub fn from_state_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != STATE_SIZE
            || bytes[26..32].iter().any(|&value| value > 1)
            || bytes[31] != 0
            || bytes[33] > 2
        {
            return Err("invalid Z80 state");
        }
        let im = match bytes[32] {
            0 => InterruptMode::Mode0,
            1 => InterruptMode::Mode1,
            2 => InterruptMode::Mode2,
            _ => return Err("invalid Z80 interrupt mode"),
        };
        let word = |offset| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        Ok(Self {
            a: bytes[0],
            f: bytes[1],
            b: bytes[2],
            c: bytes[3],
            d: bytes[4],
            e: bytes[5],
            h: bytes[6],
            l: bytes[7],
            a_alt: bytes[8],
            f_alt: bytes[9],
            b_alt: bytes[10],
            c_alt: bytes[11],
            d_alt: bytes[12],
            e_alt: bytes[13],
            h_alt: bytes[14],
            l_alt: bytes[15],
            ix: word(16),
            iy: word(18),
            pc: word(20),
            sp: word(22),
            i: bytes[24],
            r: bytes[25],
            iff1: bytes[26] != 0,
            iff2: bytes[27] != 0,
            halted: bytes[28] != 0,
            irq: bytes[29] != 0,
            nmi_pending: bytes[30] != 0,
            im0_active: false,
            im,
            ei_delay: bytes[33],
            im0_extra_cycles: u32::from_le_bytes(bytes[34..38].try_into().unwrap()),
        })
    }

    pub(crate) fn fetch8(&mut self, bus: &mut impl Bus) -> u8 {
        if self.im0_active {
            return bus.interrupt_data(self.pc);
        }
        let value = bus.read_memory(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    pub(crate) fn fetch16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.fetch8(bus);
        let hi = self.fetch8(bus);
        u16::from_le_bytes([lo, hi])
    }

    fn increment_r(&mut self) {
        self.r = (self.r & 0x80) | (self.r.wrapping_add(1) & 0x7f);
    }

    fn fetch_opcode(&mut self, bus: &mut impl Bus) -> u8 {
        self.increment_r();
        if self.im0_active {
            self.im0_extra_cycles += 2;
            bus.interrupt_acknowledge()
        } else {
            self.fetch8(bus)
        }
    }

    fn push(&mut self, bus: &mut impl Bus, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.write_memory(self.sp, hi);
        self.sp = self.sp.wrapping_sub(1);
        bus.write_memory(self.sp, lo);
    }

    fn execute_opcode(&mut self, bus: &mut impl Bus, mut opcode: u8) -> u32 {
        let mut index = None;
        let mut index_prefixes = 0u32;
        loop {
            match opcode {
                0xdd | 0xfd => {
                    index = Some(if opcode == 0xdd { Index::IX } else { Index::IY });
                    index_prefixes += 1;
                    opcode = self.fetch_opcode(bus);
                }
                0xcb => {
                    if let Some(index) = index {
                        let displacement = self.fetch8(bus) as i8;
                        let operation = self.fetch8(bus);
                        let base = match index {
                            Index::IX => self.ix,
                            Index::IY => self.iy,
                        };
                        let address = base.wrapping_add_signed(i16::from(displacement));
                        return self.execute_cb(operation, bus, Some(address))
                            + 4 * (index_prefixes - 1);
                    }
                    let operation = self.fetch_opcode(bus);
                    return self.execute_cb(operation, bus, None);
                }
                0xed => {
                    let operation = self.fetch_opcode(bus);
                    return self.execute_ed(operation, bus) + 4 * index_prefixes;
                }
                _ => return self.execute_base(opcode, bus, index) + 4 * index_prefixes,
            }
        }
    }

    /// Execute one complete instruction or one interrupt/HALT cycle.
    ///
    /// Returns nominal T-states, excluding external wait states. Bus accesses
    /// are ordered but not exposed at individual T-state boundaries.
    pub fn step(&mut self, bus: &mut impl Bus) -> u32 {
        if self.nmi_pending {
            self.nmi_pending = false;
            self.halted = false;
            self.iff1 = false;
            self.increment_r();
            bus.nmi_acknowledge();
            self.push(bus, self.pc);
            self.pc = 0x0066;
            return 11;
        }

        if self.irq && self.iff1 && self.ei_delay == 0 {
            self.halted = false;
            self.iff1 = false;
            self.iff2 = false;
            self.increment_r();
            let vector = bus.interrupt_acknowledge();
            return match self.im {
                InterruptMode::Mode0 => {
                    self.im0_active = true;
                    self.im0_extra_cycles = 2;
                    let cycles = self.execute_opcode(bus, vector) + self.im0_extra_cycles;
                    self.im0_active = false;
                    self.ei_delay = self.ei_delay.saturating_sub(1);
                    cycles
                }
                InterruptMode::Mode1 => {
                    self.push(bus, self.pc);
                    self.pc = 0x0038;
                    13
                }
                InterruptMode::Mode2 => {
                    let address = u16::from_be_bytes([self.i, vector]);
                    self.push(bus, self.pc);
                    let lo = bus.read_memory(address);
                    let hi = bus.read_memory(address.wrapping_add(1));
                    self.pc = u16::from_le_bytes([lo, hi]);
                    19
                }
            };
        }

        if self.halted {
            self.increment_r();
            // HALT continues issuing M1 cycles at the instruction after HALT.
            bus.read_memory(self.pc);
            return 4;
        }
        let opcode = self.fetch_opcode(bus);
        let cycles = self.execute_opcode(bus, opcode);
        self.ei_delay = self.ei_delay.saturating_sub(1);
        cycles
    }
}
