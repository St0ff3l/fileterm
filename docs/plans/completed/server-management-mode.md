# Multi-server management mode

## Goal

Replace the optional second-level tab layout with a persisted workspace mode. The default mode keeps the existing single-row workspace. Multi-server management mode provides a persistent page sidebar, foldered saved connections, a fleet overview, and per-server session tab groups.

## Scope

- Persist `workspaceMode` in Rust UI preferences and migrate legacy `tabBarLayout` values.
- Replace the tab layout selector with default and multi-server management mode cards.
- Keep the management sidebar available on the overview and connected session views.
- Show only active connected sessions under their saved folders; selecting one activates its existing workspace.
- Add a server overview with search, resource summaries from current sessions, and a selected-server inspector. Selecting a card only shows details; connecting keeps the overview open, with a separate action to enter the terminal and file workspace.
- Let overview cards drag-reorder profiles using the same saved folder and order fields as Connection Manager; keep the inspector and card list independently scrollable, with a collapsible inspector.
- Keep the existing terminal, file, and monitoring workspace when a server session is active.
- Keep file transfer out of the management sidebar; container management and server logs are not supported pages.
- Update architecture and design documentation.

## Boundaries

- Do not open background SSH connections for dashboard metrics.
- Do not add asset links, remote desktop, or other modules that FileTerm does not currently provide.
- Do not change the standard workspace's navigation or single-row tab behavior.

## Completion checks

- Settings expose workspace mode instead of a second-level tab option.
- Legacy `host-groups` preferences migrate to multi-server mode; `single-row` migrates to default mode.
- In management mode, the sidebar navigates between existing pages and lists only active connections by folder.
- The top bar shows only the active server profile's session tabs; its plus opens another session for that server, and the management home has no plus button.
- The overview navigation returns to the management home, and file transfer is not a sidebar destination.
- Dashboard metrics are shown only from connected sessions with fresh snapshots; saved unconnected profiles remain “not connected”.
