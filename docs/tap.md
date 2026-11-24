# Spectrum TAP support

## Status

The 48K machine accepts `Media { format: "tap", ... }` and mounts a validated
image. The standalone `TapDeck` can find an unnamed standard BASIC program,
copy its data to a caller-supplied RAM address, and verify those bytes against
RAM. This is the data-transfer portion of `LOAD ""` / `VERIFY ""`, **not**
an implementation of those BASIC commands: it does not operate the ROM,
allocate or update the BASIC workspace, interpret variables, run a program,
or simulate tape signals.

The machine cannot execute BASIC commands yet: CPU scheduling, cassette
playback and EAR input are not wired up. Mounting a TAP is not a successful
ROM load. The direct transfer API is useful for testing and later integration;
callers must not treat it as a complete substitute for ROM loading.

## File format

A TAP file is a sequence of blocks with no global header:

```text
2-byte little-endian block length
1-byte flag | payload | 1-byte XOR checksum
... repeated ...
```

The length counts the flag, payload and checksum, but not the length field.
This implementation requires at least two bytes per block and verifies that
the XOR of the flag, all payload bytes and checksum is zero. It rejects a
partial length, truncated or too-short block, and checksum mismatch, reporting
the block and/or offset. An empty file is a valid image with no blocks.
These checks use the standard Spectrum ROM XOR convention for *every* block.
Custom-loader TAPs using a different checksum convention are rejected.

Standard headers have flag `00`, a 17-byte payload, and a checksum (a
19-byte block). The payload is:

| Offset | Size | Meaning |
|---|---:|---|
| 0 | 1 | Type: 0 BASIC, 1 numeric array, 2 character array, 3 CODE |
| 1 | 10 | Spectrum filename bytes, conventionally space-padded |
| 11 | 2 | Data length, little-endian |
| 13 | 2 | Parameter 1: BASIC autostart line; values >= 32768 mean no autostart |
| 15 | 2 | Parameter 2: BASIC program length excluding variables |

For BASIC the header is followed by a standard data block with flag `ff`;
its payload length must match the header. Filename bytes are preserved
verbatim rather than interpreted as UTF-8. TAP may contain nonstandard
blocks; the parser retains their flags and data, while the BASIC transfer
operation only interprets standard BASIC header/data pairs.

## Architecture and API

- `TapImage::parse` validates the complete byte slice before constructing an
  immutable image; `TapImage::from_file` reads a file and propagates I/O
  errors. `TapCodec` implements the shared `MediaCodec` interface.
- `TapDeck` owns an image and a block cursor. `load_next_basic` searches from
  the cursor for the next standard BASIC header, ignoring filename and
  non-BASIC blocks; it validates the following data block and copies its
  contents to the caller's RAM address. It returns the raw header metadata.
- `verify_next_basic` performs the same search and compares the data with
  RAM without writing. Both methods reject ranges outside RAM, including
  overflow past `ffff`. On any error, RAM and cursor remain unchanged. On
  success, the cursor moves past the data block; `rewind` returns it to zero.
  This transactional cursor is not a simulation of physical tape movement.
- Machine media mounting parses before replacing a previous image; invalid
  input leaves the previous mount intact. Other format IDs are unsupported.

## Acceptance tests

`cargo test -p sinclair-spectrum48 --test tap` exercises two self-authored TAP
fixtures. `basic.tap` contains a small tokenized `10 PRINT "HI"` program;
`mixed.tap` contains a CODE block followed by that program. Their hexadecimal
sources are included alongside the generated binary files. The tests cover
file reads, empty-name program selection, direct load and verify, cursor and
memory behavior on failure, mount replacement, block lengths, checksum errors,
and RAM boundaries.
