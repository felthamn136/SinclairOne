# Unified machine manager

The shared `sinclair-core::manager` registry stores metadata and optional
factories, without depending on machine crates. The application assembles the
catalogue and injects ROM configuration into each factory. Register a new
machine in the catalogue with a unique, stable ID and factory; the selection
menu enumerates registrations rather than naming machines. A registration
without a factory is shown as not implemented.

The manager owns at most one active machine. Loading validates the replacement
before shutting down the old machine; during that transaction both objects
can temporarily exist. Failed creation or shutdown leaves the current machine
selected. Unload calls `shutdown` before dropping the machine. Restart creates
a fresh machine; reset delegates to the machine. A machine owning external
resources must override `shutdown` and report failures.

The manager forwards input, frames, byte media, path media, state and reset
operations to the active machine. Media paths use a lowercase extension as
the format; support is determined by the machine. Eject and media information
refer to mounted removable media, not snapshots or directly copied programs.
Quick save/load and frame-based rewind use the shared state manager. Successful
switch, restart or unload clears the quick slot and rewind history; reset,
media changes and state load clear rewind history. Unsupported operations
and missing machines are errors.

The application reads `machine_config.json` from its working directory. It
captures ROM paths and ZX81 RAM size when registering factories, and applies
the configured rewind length; restart the app after editing those settings.
The menu persists the most recently selected machine and reloads it at startup.
An invalid configuration is reported rather than overwritten. See the README
for an example. The local-ROM factory tests check that each switch completes
within one second; external file-system latency cannot be bounded.

The Spectrum 16K has a runnable, instruction-timed frame loop and uses the
configured Spectrum 48K ROM; the frontend displays its generated video.
ZX81, Spectrum 48K and Spectrum 128K remain component previews. QL is a
placeholder. Audio output, media controls, state controls, debugger and other
host service backends remain future work. The core manager API does not claim
unsupported media formats or silently emulate missing operations.
