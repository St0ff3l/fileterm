> 归档状态（2026-09-01）：串口能力缺口的核心实现已完成，已接入 Rust session、Renderer 控件、串口控制、Raw/XMODEM/YMODEM/ZMODEM/Kermit 传输、重连与进度反馈，并通过自动化协议/状态机测试和虚拟串口回环测试。实体串口的 modem-line 行为、驱动相关 Break 时序以及真实设备 X/YMODEM 互操作仍需在发布前手工验收；本文移至 `docs/plans/completed/`，该事项作为外部验收记录保留。

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
