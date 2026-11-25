<div align="center">

<img src="assets/sinclair-one-banner.png" alt="Sinclair One" width="100%">

</div>

# Project Sinclair

Project Sinclair is a Rust workspace for a multi-machine Sinclair emulator.
It contains Spectrum 48K and 128K hardware components and architecture
previews, **not a working emulator**.

## Workspace

```text
.
├── Cargo.toml
├── docs/
│   └── architecture.md
└── crates/
    ├── sinclair-core/          # machine and shared-service contracts
    ├── sinclair-cpu-z80/       # Z80 and its bus contract
    ├── sinclair-spectrum48/   # 48K machine, memory map and media boundary
    ├── sinclair-spectrum128/  # 128K memory paging and machine boundary
    ├── sinclair-zx81/         # ZX81 memory, program and video components
    └── sinclair-app/          # Macroquad application
```

See [the architecture document](docs/architecture.md) for the full module
hierarchy, API contracts, expansion strategy, and acceptance criteria. The
[Z80 core guide](docs/z80.md) describes its API, opcode groups and limitations.
[TAP support](docs/tap.md) documents the format and current loading limits.
[TZX support](docs/tzx.md) documents pulse playback and supported blocks.
[SNA support](docs/sna.md) describes 48K snapshot loading and saving.
[Save states and rewind](docs/state.md) describes the in-memory state APIs.
[ZX81 components](docs/zx81.md) describes their current capabilities and limits.
[QL architecture proposal](docs/ql.md) outlines future QL support.
The [128K paging guide](docs/architecture.md#spectrum-128k-memory-and-bus)
describes the 128K memory map and display banks.

## Build and run

Install the stable Rust toolchain and the platform dependencies required by
[Macroquad](https://github.com/not-fl3/macroquad). Then run:

```sh
cargo test --workspace
cargo run -p sinclair-app
```

The application is an architecture preview, not a running machine. The Z80,
Spectrum memory/bus and ULA snapshot renderer are not yet connected. The
preview includes a keyboard matrix display: Shift is CAPS SHIFT, Ctrl is
SYMBOL SHIFT, and Esc releases held keys. TAP images can be mounted and
validated, and TZX pulse playback is available, but neither is connected to
ROM-driven loading. In-memory save states and rewind are available through
the machine APIs, but the application has no snapshot controls. Audio hardware
and output are not implemented. No ROM is distributed; future emulation must
use a legally obtained compatible ROM.

The ZX81 crate provides a machine boundary with input and in-memory state,
1K/16K memory mapping, direct `.p` image copying, PCM WAV tape input and a
non-cycle-timed display-file renderer. It requires an externally supplied 8K
ZX81 ROM. It provides instruction-level stepping but not a frame loop,
cycle-timed video or complete FAST/SLOW display timing.
