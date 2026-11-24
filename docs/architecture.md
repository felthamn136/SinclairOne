# Project Sinclair architecture

## Status and scope

This is an initial architecture, not an emulator release. The Macroquad
application is a status and keyboard-matrix preview. The 48K memory map and
ROM size validation are implemented; a standalone Z80 core, 48K ULA snapshot
renderer and TAP parser are also available. Machine scheduling, other hardware
and shared-service backends are not implemented. Unsupported machine operations
return an explicit error. No ROM or copyrighted software is included. See
[the Z80 guide](z80.md) and [TAP guide](tap.md) for status and limitations.

The target is stable Rust and Cargo on Windows, Linux, and macOS. Platform
dependencies and window/audio drivers are supplied by Macroquad and the host
OS. No minimum Rust version or platform certification is claimed yet.

## Workspace and module hierarchy

```text
Cargo.toml                            workspace and shared package metadata
README.md
docs/architecture.md
crates/
  sinclair-core/
    Cargo.toml
    src/
      lib.rs                          public machine-neutral API
      error.rs                        shared Result and explicit errors
      machine.rs                      Machine, metadata, video and audio frames
      media.rs                        opaque media input and codec contract
      state.rs                        versioned in-memory state envelope
      services/
        mod.rs
        audio.rs                      host audio output
        config.rs                     shared configuration and persistence
        crt.rs                        video presentation and CRT boundary
        files.rs                      file selection and state storage
        input.rs                      host-to-machine input mapping
        ui.rs                         shared host UI commands
  sinclair-cpu-z80/
    Cargo.toml
    src/
      lib.rs
      bus.rs                          Z80 memory and port interface
      cpu.rs                          Z80 state, fetch and interrupt handling
      base.rs                         base and indexed instruction decoding
      cb.rs                           bit and indexed-bit operations
      ed.rs                           extended and block operations
  sinclair-spectrum48/
    Cargo.toml
    src/
      lib.rs
      bus.rs                          48K memory and I/O Z80 bus adapter
      keyboard.rs                     keyboard matrix and port adapter
      machine.rs                      48K Machine and hardware composition
      memory.rs                       16 KiB ROM / 48 KiB RAM map
      media.rs                        machine-specific codec modules
      media/tap.rs                    TAP parser and direct BASIC transfers
      video.rs                        screen and border snapshot renderer
  sinclair-app/
    Cargo.toml
    src/
      main.rs                         Macroquad preview
      keyboard.rs                     Macroquad host-key adapter
```

Dependencies point inward: the application selects machine implementations;
the 48K machine depends on core and Z80; the Z80 is independent; core
has no frontend, CPU, or machine dependencies. An executable host can store
different implementations as `Box<dyn Machine>`. Machines do not depend on
Macroquad. No universal CPU trait is imposed: a future 68000 implementation
for the QL need not fit the Z80's address, bus, timing, or interrupt model.

## Public contracts and ownership

- `Machine` exposes metadata, reset, input, one frame of execution, media
  loading, and save/load state. Machine identifiers are stable persistence
  identifiers, not display names. Operations currently return `Unsupported`
  until implemented; constructors reject invalid ROMs.
- `run_frame` returns borrowed RGBA8 row-major video and interleaved stereo
  `f32` audio with a sample rate. The borrow lasts until the next mutable
  machine operation; the host must consume or copy both buffers first. A
  producer must supply exactly `width * height * 4` video bytes, an even
  number of audio samples within -1..=1, and a nonzero sample rate.
- `InputManager` translates host events into machine-defined control names.
  The selected machine defines its supported names and must reject unknown
  ones; a host with focus notifications should send releases on focus loss or
  machine replacement.
  UI commands such as save and reset are distinct from emulated controls.
- `Media` carries a format identifier and bytes, not a filesystem dependency.
  The host obtains bytes; each machine validates its formats and determines
  whether loading is supported. Machine-specific `MediaCodec` implementations
  own format parsing. External snapshot formats are media, distinct from the
  internal save-state envelope.
- `State` identifies the machine, payload version, and machine-owned bytes.
  `StateStore` will implement the shared on-disk envelope and I/O; no on-disk
  encoding or backwards compatibility is promised yet. A machine must validate
  identity, version, lengths, and all payload data before mutating live state.
  Future versions must define migrations or reject older payloads explicitly.
  States must capture all timing, CPU, hardware and input state needed for
  deterministic continuation, including any ROM identity needed to restore.
- `FileBrowser`, `ConfigStore`, `AudioOutput`, `VideoPresenter`, and
  `UserInterface` are shared host boundaries, not implementations. File
  selection distinguishes cancellation from failure. The presenter may apply
  CRT effects without modifying machine state. Platform adapters may need
  asynchronous dialogs; revise the synchronous skeleton before integrating
  any backend that cannot safely block the Macroquad event loop.
- `Memory` models the 48K's `0000–3fff` ROM and `4000–ffff` RAM. The 48K
  `Ula` reads screen memory independently of the CPU. `SpectrumBus` adapts
  the same memory and a supplied `SpectrumIo` to the Z80 `Bus` contract.
  Machine scheduling, actual port devices and contention are not implemented.

### Spectrum 48K memory and bus

`Memory::new` requires exactly 16 KiB of ROM and initializes the 48 KiB RAM
to zero. Reads use a flat 64 KiB backing store with constant-time indexing;
writes use the same mapping and ignore ROM addresses. The store is private so
callers cannot bypass ROM protection. This design does not claim a particular
measured throughput or model memory contention.

Construct `SpectrumBus::new(&mut memory, &mut io)` and pass it to `Z80::step`.
Memory accesses go directly to `Memory`; ports and interrupt hooks are
delegated to the supplied `SpectrumIo`. The caller must implement that
interface explicitly: no keyboard, ULA I/O or interrupt device is fabricated.
The adapter's borrows end after execution, allowing the ULA to read the same
memory. The `Spectrum48` machine still does not schedule or run the CPU.

Memory acceptance criteria:

- Exactly 16 KiB of ROM is accepted; all RAM starts at zero.
- All 65,536 addresses read from the specified map; writes to ROM leave it
  unchanged and RAM writes are independently visible, including both boundaries.
- CPU instruction fetches and memory accesses use the same map through the
  Z80 bus; port accesses and interrupt callbacks reach the supplied device.
- `cargo test -p sinclair-spectrum48` passes, including a CPU program that
  exercises RAM, ROM protection and I/O through the adapter.

### Spectrum 48K ULA renderer

`video::screen_address` and `video::attribute_address` take pixel coordinates
within the 256x192 display and return CPU addresses, rejecting out-of-range
coordinates. The renderer reads bitmap bytes at `4000–57ff` and attributes at
`5800–5aff`. It implements INK, PAPER, BRIGHT and FLASH; FLASH swaps ink and
paper every 16 frame numbers. RGB intensity is a conventional approximation,
not an analogue output model.

`Ula::render_frame` returns 320x240 row-major RGBA8 pixels: the display plus
a 32-pixel left/right and 24-pixel top/bottom border. This is a chosen crop,
not the complete PAL raster. `render_scanline` returns one row of the same
frame and produces identical pixels. `FrameConfig` supplies the frame number,
starting border colour and raster-ordered `BorderChange` events. Each event
takes effect before its specified cropped-frame pixel, even within the active
display. `BorderChange::from_port` extracts bits 0–2 of a ULA port write.
The caller supplies the last colour as the next frame's starting colour and
maps writes in blanking or outside the crop to the appropriate visible
position. Invalid colours, positions and event ordering return errors.

This renderer samples a supplied memory snapshot. It does not model
cycle-timed screen-memory writes, contention, interrupts, floating bus or ULA
port I/O; no machine scheduler currently generates frame numbers or border
events. A `.scr` image can be rendered by loading its 6,912 bytes into memory
at `4000`; file selection and format loading remain host/machine work.

### Spectrum keyboard

`KeyboardMatrix` stores eight active-low rows of five keys. A zero on an
address line A8–A15 selects its row; selecting multiple rows ANDs their column
bits. `KeyboardIo` composes with the machine's supplied `SpectrumIo`: every
even port (A0 low, including `FE`) scans the matrix, preserving the underlying
device's upper bits and any other low bits it pulls down. Odd ports and all
output and interrupt callbacks are delegated unchanged. The matrix does not
model electrical ghosting. The underlying device is responsible for EAR and
other port behavior; no complete ULA I/O device or machine scheduler exists.

`Spectrum48::input` accepts lowercase letters, digits, `enter`, `space`,
`caps_shift` and `symbol_shift` as control IDs and reports unknown IDs as
errors. The Macroquad preview translates host key state into those controls:

- Letters, digits, Space and Enter map directly.
- Either Shift is CAPS SHIFT; either Ctrl is SYMBOL SHIFT. These are physical
  Spectrum modifiers, not host text/character translation.
- Backspace maps to CAPS SHIFT+0; arrows map to CAPS SHIFT+5/6/7/8.
- Comma and period map to SYMBOL SHIFT+N/M. Key codes are layout-dependent;
  other host punctuation is not yet mapped.

Multiple host keys may hold the same Spectrum key without releasing it
prematurely. Esc in the preview releases all emulated keys and suppresses
stale host-down states until a fresh press. Macroquad's input API does not
reliably report focus loss on every platform, so automatic focus-loss release
is not guaranteed; use Esc if a key becomes stuck.

Keyboard acceptance criteria: every key scans on its documented row and
column; simultaneous row selections combine correctly; both shift keys and
shifted combinations work; port reads are tested through the Z80 bus; and the
Macroquad adapter handles overlapping mappings and release.

All interfaces are an initial, pre-stability API. Errors must be surfaced to
the user; do not treat unsupported operations, malformed data, or I/O errors
as success. Machine and host code should validate data at their boundaries.

## Intended runtime flow

1. The host chooses a machine and loads a user-supplied, legally obtained ROM.
   The machine constructor validates it before use.
2. The host polls UI and platform input, translates machine controls, and
   handles UI commands separately.
3. The machine advances by its own frame timing; the host consumes the
   resulting video and audio, applies optional presentation-only CRT effects,
   and paces execution without embedding host time in machine logic.
4. The host selects and reads files, passes bytes and format identifiers to
   the machine, and persists state envelopes through a shared state store.
5. On failures, the host displays actionable errors. On focus loss or machine
   switch it releases held controls and handles audio buffers appropriately.

This is the target flow, not current app behavior. Frame pacing, buffering,
resampling, UI, browser, file I/O, and backend error reporting need actual
implementations and cross-platform validation.

## Expansion strategy

1. Validate and refine Z80 instructions, flags, interrupts, bus cycles and
   timing against established test vectors. Keep machine I/O in machine buses.
2. Connect and extend the 48K ULA and keyboard, and add the frame scheduler,
   beeper, ROM selection, media codecs, and deterministic state
   payload. Verify behavior against hardware references and legally usable
   test ROMs.
3. Implement shared Macroquad presentation, audio, input and UI adapters,
   plus file selection, configuration and versioned state persistence.
   Connect them to a machine registry in the app rather than hard-coding
   machine behavior into the host.
4. Add ZX81, Spectrum 16K and 128K as separate machine crates. Reuse Z80
   where appropriate; extract shared Spectrum hardware only after behavior
   and variation boundaries are established. Each machine owns its map,
   timing, supported controls, media and state version.
5. Add a separate 68000 CPU and QL machine with its own buses, video and
   formats. Reuse only machine-neutral host contracts; evolve those contracts
   when real hardware requirements expose gaps, rather than forcing a Z80
   model onto it.

## Acceptance criteria

### Initial skeleton

- `cargo test --workspace` and `cargo build --workspace` succeed on a stable
  Rust toolchain with Macroquad's platform prerequisites installed.
- The app opens a window identifying the 48K target and clearly states that
  emulation is not implemented. It must not claim to boot a ROM.
- Core builds without Macroquad or machine/CPU dependencies; a machine is
  usable through `dyn Machine`. Z80-specific bus types remain outside core.
- A 48K constructor accepts exactly 16 KiB of ROM. Reads cover the whole
  address space; writes leave ROM unchanged and update RAM at both boundaries.
- Machine operations that are not implemented report `Unsupported`.
  State identity/version mismatches are rejected.
- Shared host boundaries exist for UI, file selection, audio, state storage,
  configuration, CRT presentation and input; their implementations are
  explicitly identified as future work.

### Before calling the 48K emulator usable

- A legal compatible ROM boots; CPU instruction, interrupt and timing tests
  pass against documented reference vectors, including contention and ULA
  behavior where relevant.
- Keyboard, screen, beeper, supported media, reset, configuration, file
  selection, state save/restore, and error reporting work end to end. State
  round trips reproduce subsequent machine output; incompatible/corrupt states
  do not alter the running machine.
- Frame pacing and audio operate without unbounded drift or buffer growth;
  presentation effects are optional and do not affect emulated state.
- Build, launch and representative interaction tests pass on Windows, Linux,
  and macOS; platform-specific limitations are documented.
