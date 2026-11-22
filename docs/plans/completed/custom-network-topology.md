# Custom network topology and health

## Goal

Extend the server topology board so users can identify node roles, describe the meaning and direction of links, and see only operational state supported by a real data source.

## Existing baseline

- The renderer stores a user-arranged board in local storage. Nodes refer to FileTerm connection profiles; configured SSH jump/proxy routes are rendered separately.
- Supported SSH resource snapshots already include CPU, memory, disks, GPU, per-interface RX/TX rates, and top processes.
- Manual links currently store two node IDs and an optional label. They do not encode edge meaning, live health, or flow attribution.
- Network-device SSH profiles intentionally disable generic resource monitoring. Do not send server or vendor commands to them as a topology side effect.

## Delivered in PR #411

### Editable topology semantics

- Added backward-compatible node roles and display labels.
- Replaced legacy link tuples internally with typed links carrying kind, direction, and label; existing tuples still decode.
- Made configured jump/proxy routes visibly distinct and colored them by current session state.

### Supported operational status

- Added user-configured TCP endpoint checks per node. Checks run from the local FileTerm machine and are labeled with that vantage point.
- Reused active session state and resource-monitoring snapshots for connection and hardware status.
- Show unknown when there is no fresh compatible source. Service checks are bounded to 64 endpoints, 8 concurrent connections, and a 2-second timeout.
- Kept per-interface RX/TX separate from edge flow, and explain that limitation when a link is marked as a traffic path.

### Deferred source integrations

- Per-link traffic requires a source that attributes bytes to a link (for example, an explicitly configured SNMP interface pair or flow exporter). Per-interface RX/TX remains an aggregate device metric.
- Automatic physical discovery requires supported neighbor data such as LLDP/CDP or MAC tables and a separate collector lifecycle. It must not run an implicit host scan.
- Vendor-specific firewall hardware and route state require explicit, read-only adapters; generic network-device mode remains raw terminal only.

These items are tracked separately in the [topology discovery and link telemetry plan](../active/topology-discovery-and-link-telemetry.md); they are outside PR #411.

## Acceptance

- [x] Existing v1 board data loads without losing node positions, metrics, or links.
- [x] Users can assign roles and edit edge kind/direction/label without breaking configured SSH route rendering.
- [x] Configured TCP services display reachable/unreachable/unknown with the local probe vantage point and a recent-check time.
- [x] Existing CPU/memory/disk/GPU/network/process metrics appear only when their source session has a fresh healthy snapshot and supports monitoring.
- [x] No UI presents interface aggregate rates as point-to-point link traffic.
