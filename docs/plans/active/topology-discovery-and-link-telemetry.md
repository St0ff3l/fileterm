# Topology discovery and link telemetry

Status: Proposal; implementation has not started. This is follow-up work outside PR #411.

## Goal

Add data-backed network discovery and link status to the configurable topology board without turning FileTerm into a full network monitoring system. The board remains useful as a manually maintained diagram when no collector is configured.

## Current baseline

- The board supports manually placed nodes, typed links, per-node TCP endpoint checks, and supported-session hardware metrics.
- A hand-drawn link has no discovered route or live status. Per-interface RX/TX is a host-level metric, not traffic attributed to a link.
- Network-device SSH mode intentionally does not run generic resource-monitor commands. See the [completed custom topology plan](../completed/custom-network-topology.md) and [network-device SSH compatibility plan](network-device-compatibility.md).

## Stages

### 1. Define supported data sources and collection boundaries

- Select initial device families and their available LLDP/CDP neighbor, MAC-table, SNMP, and route data sources.
- Decide how users explicitly configure devices, credentials, polling targets, and collection intervals; keep credentials in the existing protected storage path.
- Keep collection opt-in and read-only. Do not perform implicit host scans or issue vendor commands as a side effect of opening the canvas.

### 2. Add opt-in topology discovery

- Discover neighbors from supported LLDP/CDP data or MAC tables, with the source and last refresh time visible.
- Let users review and reconcile discovered devices and links with the manually maintained board; do not silently overwrite positions, labels, or links.
- Represent unsupported, missing, and stale discovery data as unknown or stale rather than inferring connectivity.

### 3. Add explicit network telemetry and link attribution

- Evaluate SNMP interface polling and flow-export sources for network and firewall devices.
- Map interfaces or flow records to board links only when the source provides enough information to identify both endpoints.
- Show source, direction, unit, and sample time. Keep uncorrelated traffic as a device/interface metric; never present it as point-to-point flow.

### 4. Add vendor-specific device status where justified

- Add narrowly scoped, read-only adapters for selected firewall hardware and route state after supported vendors and reliable data sources are identified.
- Reuse the existing network-device compatibility boundaries; do not introduce arbitrary command execution or claim broad vendor support without device testing.

## Acceptance criteria

- [ ] Discovery and telemetry are disabled until the user configures a supported source.
- [ ] Manual links remain visually distinct from discovered routes and measured flows.
- [ ] Every discovered or measured state identifies its source and freshness; missing or stale values show unknown/stale.
- [ ] Traffic appears on a link only when the data source can attribute it to that link and direction.
- [ ] Unsupported devices do not trigger generic monitoring commands, implicit scans, or guessed vendor status.
- [ ] Discovery results can be reviewed before merging into the user's board.

## Deferred decisions

- Initial vendor/device support matrix and the first supported collection protocol.
- Whether discovery refresh is manual-only or scheduled per configured device.
- SNMP credential/privacy options, polling load limits, and flow-export retention.
- The interface identity and reconciliation model when MAC/neighbor data changes.
