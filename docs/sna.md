# Spectrum 48K SNA snapshots

`Spectrum48::load_sna` and `Media { format: "sna", ... }` load a 48K SNA;
`Spectrum48::save_sna` returns its bytes. `SnaSnapshot::parse`, `capture`
and `to_bytes` provide the standalone codec, and `SnaCodec` implements
`MediaCodec`. Loading validates everything before changing the machine; it
preserves the supplied ROM, keyboard and mounted tape. The application does
not yet expose these operations, and the machine still cannot execute frames.

A 48K SNA consists of a 27-byte header followed by 49,152 RAM bytes. Register
pairs are little-endian. The header stores I, alternate HL/DE/BC/AF, main
HL/DE/BC, IY, IX, IFF2 in bit 2, R, AF, SP, interrupt mode and border colour.
The saved SP points to the little-endian PC in RAM. Loading simulates RETN:
PC is popped, SP advances by two with 16-bit wrapping, and IFF1 becomes IFF2.
Other bits in the IFF byte are ignored. Saving emits only bit 2.

Exactly 49,179 bytes are accepted; 128K snapshots and trailing data are not.
The stack's two saved PC bytes must both lie in RAM (`4000` through `ffff`);
interrupt mode must be 0, 1 or 2, and border colour 0 through 7. Saving
copies RAM and pushes PC into that copy, overwriting the two bytes immediately
below SP; the live machine is unchanged, but loading the file cannot recover
the original values of those two bytes. Saving fails if the stack cannot hold
PC or if IFF1 differs from IFF2, the CPU is halted, or an interrupt input,
NMI or post-EI delay is pending: SNA cannot faithfully encode those states.

SNA does not record ROM identity, keyboard, tape, timing or other device
state. It is an interchange snapshot, not the internal `State` envelope
described in [save states and rewind](state.md). The caller must supply a
compatible ROM.

Run `cargo test -p sinclair-spectrum48 --test sna` for format, register, RAM,
interrupt, stack-boundary, validation and machine-integration acceptance tests.
