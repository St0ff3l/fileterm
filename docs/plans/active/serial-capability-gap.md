# Serial capability gap

## Scope

This plan tracks the serial-session parity work requested from the common
terminal clients. The implementation keeps the worker orchestration small and
places protocol-specific behavior under `apps/tauri/src-tauri/src/sessions/serial/`.

## Module boundaries

- `mod.rs`: port lifecycle, command routing, receive/display loop.
- `codec.rs`: text/Hex input, line mode, streaming decoding, display formatting.
- `config.rs`: data bits, stop bits, parity, flow control, platform error mapping.
- `control.rs`: DTR/RTS, Break, buffer clear, reset, modem-line status.
- `pacing.rs`: cancellation-safe per-byte and per-line write delays.
- `reconnect.rs`: exponential backoff and maximum-attempt policy.
- `transfer.rs`: Raw, XMODEM, and YMODEM framing/checksums.

Renderer controls live in `features/serial/`: line controls, file transfer,
quick-send history, saved macros, and loop sending are separate components.

## Current verification boundary

The Rust protocol state machines and XMODEM round trip use in-memory streams,
so they can run without a USB/serial device. Physical modem-line behavior,
driver-specific Break timing, and real-device X/YMODEM interoperability still
need a hardware pass before a release is declared validated.
