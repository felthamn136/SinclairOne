# Sinclair QL architecture proposal

This is a design, not an implemented machine. The baseline is an original
PAL QL with a 7.5 MHz Motorola 68008, 128 KiB RAM, two Microdrives, two
serial ports and an externally supplied QDOS ROM set. No ROM or other
copyrighted software is bundled. Other ROM revisions and expansions are
separate, explicitly selected compatibility profiles.

## Subsystems and ownership

| Subsystem | Responsibility |
|---|---|
| 68008 | Registers, supervisor/user state, instruction decoding, exceptions, interrupts and ordered 8-bit bus cycles. |
| Machine bus and memory | Address decoding, ROM, RAM, expansion claims, bus errors and device wait states. |
| ZX8301 | Display modes and screen selection, video timing, RAM arbitration and relevant control registers. |
| ZX8302 | Interrupt sources, real-time clock, serial transmission, Microdrive control and the IPC link. |
| IPC | Model the 8049-facing protocol, keyboard, sound, joystick and serial reception. Start with a documented protocol-level model; use an 8049 core only if compatibility requires it and appropriate firmware is available. |
| Microdrives | Two independently mounted cartridges, drive selection, tape movement, sector/gap events and writable media. |
| Serial | Two emulated ports, transmit/receive queues, line timing, framing and handshake signals, with replaceable host endpoints. |
| Scheduler | Coordinate CPU, display, IPC, Microdrive, serial and interrupt events in emulated time. |
| QDOS | Execute the supplied ROM on the emulated hardware; QDOS owns traps, jobs, files and SuperBASIC. No host trap stubs are the compatibility baseline. |
| Host | Reuse the shared `Machine`, input, audio, video, media, UI, configuration and state boundaries; keep platform code in the application. |

Use the existing `Machine` trait, not a Z80-shaped CPU abstraction. The QL
owns its CPU and devices; the application owns ROM selection, media selection
and host endpoints. `run_frame` should return borrowed RGBA8 and interleaved
stereo audio under the existing contract. The IPC sound generator is a
machine device; the existing audio service is an output sink, not the device.
Expose physical QL keys through the shared input event interface, with a
QL-specific host mapping.

### Proposed modules

```text
crates/
  sinclair-cpu-m68008/       CPU core, instruction/exception tests, bus contract
    src/{lib,cpu,bus,decode,exception}.rs
  sinclair-ql/
    src/{lib,machine,bus,memory,scheduler}.rs
    src/{zx8301,zx8302,ipc,keyboard,audio}.rs
    src/{microdrive,serial,media,state}.rs
    tests/                   hardware and QDOS acceptance fixtures
  sinclair-app/              future QL selection and host adapters
```

These are proposed boundaries, not files to create now. Keep the CPU crate
independent of QL and host APIs. Device register access goes through the QL
bus; host I/O goes through bounded adapters rather than directly from CPU
instructions. Add shared interfaces only when an actual second consumer
requires them.

## Baseline address map

Addresses below are inclusive, hexadecimal, and describe the original
20-bit, 1 MiB 68008 address space. Installed expansion hardware can override
parts of this map; its profile must specify decoding and conflicts.

| Range | Baseline use |
|---|---|
| `00000-0BFFF` | 48 KiB system ROM; reset vectors reside at the start. |
| `0C000-0FFFF` | 16 KiB ROM cartridge/extension window, if populated. |
| `10000-17FFF` | Not onboard RAM in the baseline; expansion-owned where decoded. |
| `18000-1BFFF` | Onboard hardware register decode, including ZX8301/ZX8302. Individual addresses can have different read and write meanings. |
| `1C000-1FFFF` | Expansion-owned; no baseline onboard RAM is assumed. |
| `20000-27FFF` | First 32 KiB of onboard RAM and default display area. |
| `28000-3FFFF` | Remaining onboard RAM; QDOS normally places system structures starting near `28000`, but their locations are not a fixed application ABI. |
| `40000-BFFFF` | Conventional expansion RAM window, up to 512 KiB additional RAM when fitted. |
| `C0000-DFFFF` | Expansion peripheral space. |
| `E0000-FFFFF` | Expansion ROM space. |

The ZX8301 can select a second 32 KiB screen at `28000-2FFFF`; it is the
same underlying RAM QDOS normally uses, not extra memory. Selecting it must
not silently reserve or copy RAM. Unpopulated and partially decoded regions
need explicit, hardware-verified bus behavior; do not treat every hole as
RAM or assume a bus error without testing the selected hardware profile.
The map above is not a map for replacement-CPU cards with wider address buses.

Register decoding must preserve direction and side effects: for example,
`18000-18003` include RTC reads and control writes; `18020-18023` cover
Microdrive/status, interrupts, data and display-control functions. Verify
aliases, bit definitions and timing against the hardware documentation before
implementation rather than extrapolating one register into an entire range.

## CPU and timing

The 68008 has a 32-bit programmer model, an 8-bit external data bus and a
20-bit physical address bus in the original QL. Keep addresses and arithmetic
in appropriate CPU widths, and apply physical address decoding at the bus
boundary. Word and longword accesses are big-endian ordered byte transfers;
odd word/long accesses must generate the appropriate address exception.
Implement reset vectors, supervisor/user stacks, privilege checks, condition
codes, instruction families, exception frames, interrupt priority and
acknowledgement before claiming QDOS compatibility.

Schedule devices using an integer emulated clock and preserve fractional
conversion where clocks differ. Bus callbacks must distinguish instruction,
data and interrupt accesses, expose access ordering and account for waits and
ZX8301 RAM contention. A simple instruction-total-only interface is not
sufficient for timing-sensitive device accesses. Define event ordering for
simultaneous events and test it; presentation and host clock must not drive
emulated time.

## Video, audio and input

The ZX8301 reads a 32 KiB screen from RAM, 128 bytes per scanline. Mode 4 is
512x256 with black, red, green and white; mode 8 is 256x256 with eight RGB
colours and per-pixel flash. Decode the documented packed bit layout, mode
register, selected screen, flash phase and video timing, including writes
that occur during a frame. For the host, output 512x256 RGBA8 in both modes,
horizontally doubling mode-8 pixels; a frame is exactly 524,288 bytes. PAL
50 Hz is the baseline; other timing standards require separate profiles.

The IPC receives keyboard/joystick input and generates the speaker signal.
Model the CPU-to-IPC commands and timing, then turn the device signal into
host audio samples. Keep display and audio time, pending events and partial
sample state in snapshots. Do not infer QL video from the Spectrum renderer.

## Microdrive and serial

Treat cartridges as media with explicit format identification, write
protection and errors. The hardware backend models a continuous tape loop,
drive-selection chain, motion, gaps, headers, data sectors, checksums and
interrupts, rather than assuming an ordinary random-access disk. Support two
drives independently and preserve cartridge identity, tape position and
pending writes. Define and document the first supported image format before
coding; reject unsupported or malformed images rather than guessing. A
sector-level acceleration layer may be added later only as an explicitly
enabled compatibility mode.

The ZX8302 transmits serial data and the IPC handles receive. Model SER1 and
SER2 independently, including framing, timing, buffering, error and handshake
state. Host adapters can provide disconnected, loopback and file/stream
endpoints; translating physical wiring belongs at the adapter boundary.
External bytes and outputs are nondeterministic side effects, not part of
QDOS itself.

## QDOS compatibility and expansion

Boot a user-supplied, identified QDOS ROM through its reset vectors and let
the ROM implement its own traps, scheduling, drivers and SuperBASIC. Define a
versioned compatibility matrix with ROM revision, machine profile, known
limitations and test results. A boot screen alone does not establish QDOS
compatibility. Do not distribute ROMs or copyrighted commercial programs;
tests should use user-supplied ROMs and redistributable or generated fixtures.

An expansion registry should claim address ranges, interrupts and bus
arbitration explicitly; reject conflicting claims. First support optional
contiguous RAM in the conventional window and the ROM cartridge. Later cards
may add storage, networking or alternate CPUs as distinct hardware profiles
with their own address maps, firmware, timing and compatibility tests. Do not
silently widen the original 68008 address bus.

## Save states and external effects

Use the shared versioned, checksummed machine envelope and quick-save/rewind
manager. Capture CPU exception/interrupt state, all RAM, ROM identities,
ZX8301/ZX8302 and IPC state, scheduler deadlines, audio/video phases,
Microdrive mechanics and media changes, serial queues and configuration.
Validate the entire state and required media/ROM identities before mutation.

Use a checkpointable per-cartridge write overlay for rewind; commit changes
to the underlying image only through an explicit host action. Snapshotting
must preserve overlay contents and pending writes. A physical serial endpoint
cannot undo bytes already transmitted or recreate future input: record a
deterministic input trace for replay, or explicitly disconnect/pause the
endpoint on restore and report that limitation. Never silently claim
deterministic rewind across uncontrolled external effects.

## Acceptance criteria

1. **CPU/bus:** independently verified 68008 tests cover all instruction
   families, flags, address errors, exceptions, interrupt priority, reset,
   big-endian word/long byte ordering, bus errors and wait-state propagation.
   The Z80 and existing machine tests remain green.
2. **Memory:** boundary tests cover every baseline range, ROM write
   protection, cartridge mapping, both screen selections, absence of
   uninstalled RAM and a 512 KiB expansion. QDOS detects 128 KiB and 640 KiB
   total RAM in the respective profiles.
3. **Boot/QDOS:** with a legally supplied, identified reference ROM, cold boot
   reaches the startup selection and interactive SuperBASIC prompt within
   30 seconds of PAL emulated time. An automated input fixture executes a
   small BASIC program and checks its displayed output; separate fixtures
   exercise a documented manager trap, I/O trap and multitasking.
4. **Video:** exact pixel tests cover all four mode-4 colours, all eight
   mode-8 colours, flash phases, first/last pixel and scanline, both screen
   banks, and writes during display. Every output buffer has 512x256 RGBA8
   pixels; 50 frames advance one second of PAL emulated time.
5. **Microdrive:** two independent test cartridges can be mounted, formatted,
   written, read, ejected and remounted with byte-identical file contents,
   including a file larger than one 512-byte sector. Test write protection,
   malformed images, gap/sector interrupts and full-media errors.
6. **Serial:** deterministic loopback tests at 1,200 baud, 8N1 transfer
   `00 55 FF` in order on each port; exercise receive, transmit, handshake,
   interrupt and framing/error handling without requiring a physical port.
7. **State and UI:** quick save/load and rolling rewind restore video, audio,
   memory, device timing and writable cartridge overlays. Incompatible
   ROM/media and corrupt states fail without mutation. The host exposes
   machine, ROM, media and serial selection and reports errors explicitly.
8. **Integration:** run the existing workspace test suite and the above
   profile-specific acceptance suite with a recorded ROM fingerprint and
   fixtures. Repeat deterministic tests from a clean state and document any
   unsupported profile or known deviation.

## Implementation roadmap

1. Establish reference ROM/profile ownership, hardware traces and public test
   fixtures; specify image formats and device/event interfaces.
2. Build and independently validate the 68008 core and byte-wide bus; add the
   baseline ROM/RAM map and expansion conflict checks.
3. Add the scheduler, ZX8301, ZX8302 and IPC protocol, input and sound. Reach
   a reproducible QDOS boot and prompt before expanding the compatibility
   claim.
4. Implement Microdrive and serial hardware plus host adapters; validate real
   QDOS file and serial operations, including failure paths.
5. Integrate transactional state, rewind and media overlays, then frontend
   selection and error presentation.
6. Add additional ROM revisions, RAM/ROM expansions and peripherals one
   profile at a time; publish the compatibility matrix and regression results.

## References

- [QL Technical Guide](https://ia801404.us.archive.org/0/items/SinclairQLHomepage/docs/manuals/qltm.pdf)
  (QDOS, memory, hardware and expansion interfaces).
- [QL service manual](https://www.manualslib.com/manual/4272943/Sinclair-Ql.html)
  (hardware architecture and IPC).
- [QL screen layout](https://www.timexsinclair.com/article/the-sinclair-ql-screen/index.html)
  (screen organisation and pixel modes).
- [Microdrive hardware notes](https://www.sinclairql.net/djw/docs/hardware/Microdrives.pdf)
  (cartridge format, drive control and gaps).
