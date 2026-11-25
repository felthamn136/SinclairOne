# Save states and rewind

The 48K and 128K `Machine::save_state` and `load_state` APIs capture the
hardware currently present in each machine. States include the complete Z80
registers and interrupt control, RAM, keyboard, and mounted TAP/TZX image and
playback position. The 48K state includes its border; the 128K state includes
all eight banks and the paging latch, including ROM and screen selection.
ROM bytes identify the required installed ROMs; loading with different ROMs
is rejected. A CRC-32 detects accidental payload corruption. Identity, version,
lengths and device values are validated before the machine is changed.

The ZX81 machine boundary also saves CPU, installed RAM, keyboard, line-timer
and mounted program or WAV playback position, with matching ROM and RAM-size
checks. Its execution is instruction-stepped and timed video is not implemented
yet; see [ZX81 components](zx81.md).

These are in-memory, versioned envelopes, not SNA files. They are distinct
from [48K SNA interchange snapshots](sna.md), and there is no implemented
on-disk `StateStore` yet. The machines have no emulated audio device or frame
scheduler, so no audio device state, audio buffers or frame timing can be
captured. Renderer frame numbers and border events are caller-supplied and
are not machine state; the 128K machine does not yet model a border register.
Future device or timing implementations must extend the state format and
version before claiming deterministic continuation of that hardware.

## Quick save and rewind

`StateManager::new(config.rewind_history_length)` creates a host-side quick
slot and a ring of at most that many checkpoints. The default configuration is
300 checkpoints; zero disables recording. The unit is checkpoints, not seconds:
the host chooses the capture cadence. Call `record(&machine)` **before**
advancing, or call `StateManager::run_frame(&mut machine)` to capture before
each successfully executed frame. A failed frame does not add a checkpoint or
evict history; the machine's own error semantics still apply. Then
`rewind(&mut machine)` restores and consumes the most recent checkpoint.
After rewinding, new recordings form a new branch. Resizing drops
the oldest entries first. A failed capture or restore leaves the slot and
history unchanged. Recording a different machine ID or state version into
nonempty history is rejected; clear the history before changing machines.

`quick_save` replaces the in-memory slot, and `quick_load` restores it and
clears rewind history after success. Both report errors, including an empty
slot or incompatible machine. Call `clear_history` after external state loads,
machine changes or other timeline changes. Each checkpoint stores a full
snapshot including ROM and tape data; memory consumption grows with history
length and mounted image size.

The machine manager exposes quick-save, rewind and frame recording. The
application runs the Spectrum 16K through `MachineManager::run_frame`, but has
no quick-save keys or rewind controls. Other machine frame loops remain
unimplemented.

Acceptance tests run with `cargo test -p sinclair-spectrum48 --test state`,
`cargo test -p sinclair-spectrum128 --test state`, and `cargo test
-p sinclair-cpu-z80 --test acceptance`.
