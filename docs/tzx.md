# Spectrum TZX support

`TzxImage::parse` validates a TZX 1.x image; `TzxCodec` implements `MediaCodec`
and `TzxImage::from_file` reads a file. The 48K and 128K machines accept
`Media { format: "tzx", ... }` and expose the mounted `TzxDeck` via
`tzx_tape` and `tzx_tape_mut`. Failed mounts leave the old tape untouched.
On the 48K, mounting a TAP or TZX replaces the other tape.

Supported blocks are standard data (`10`), turbo data (`11`), pure tone
(`12`), pulse sequence (`13`), pure data (`14`), pause/stop (`20`), and
signal-level setting (`2b`). Text, group, message, archive and hardware
metadata (`21`, `22`, `30`–`33`) are retained but have no playback effect;
the host is responsible for presenting messages. Other blocks, including
loops, jumps and recordings, are rejected explicitly rather than skipped.
Data is played as stored without TAP checksum validation. Malformed lengths,
invalid bit counts and zero-length active pulses are rejected.

## Playback timing

TZX pulse lengths use a fixed **3.5 MHz** reference clock. `TzxDeck` streams
the current EAR level and the T-states in `remaining()` until the next
boundary. `advance(n)` advances that clock and returns the number of T-states
actually consumed, stopping early at a stop block or the end of tape. To
observe every edge, advance by at most `remaining()` each time. The initial
level is low. Pulses alternate levels; bits are MSB-first with two pulses per
bit. Turbo and pure-data blocks honor the used-bit count in the last byte.

Standard blocks use 2,168-T pilot pulses (8,063 when the first byte is below
128, otherwise 3,223), 667- and 735-T sync pulses, and 855/1,710-T pulses
for zero/one bits. Turbo blocks supply their own timings and pilot count.
Nonzero pauses last exactly 3,500 T-states per millisecond: the first
millisecond uses the current next-pulse level, then the level is low. A zero
pause on a data block adds no time and does not change polarity; a zero
standalone pause stops the deck. `resume` continues after a stop, and
`rewind` returns to the start.

The deck is a standalone timing source: no scheduler, EAR port, audio output,
ROM tape loader or application tape controls are connected yet. In particular,
a 128K scheduler must convert between its CPU clock and the TZX reference
clock without losing fractional time. `cargo test -p sinclair-spectrum48
--test tzx` runs the format, timing, playback and mounting acceptance tests.
