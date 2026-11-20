# Server topology board

Use [HeartLink's asset canvas](https://heartlink.hearthrob.cn/) as a product reference for visualizing saved connections. Add a topology page to FileTerm's multi-server mode with movable nodes, manually drawn relationships, and an action to open each saved connection.

- Store one board in renderer-local storage using profile IDs, canvas coordinates, and undirected links. Resolve names and addresses from existing profiles.
- Add status filters and compact cards to the multi-server overview. Keep the existing inspector, session actions, and profile ordering.
- Keep the canvas origin stable when the selection changes. Preview node and link movement together and save coordinates on release. Read previously saved grid positions at their original on-screen locations.
- Show protocol and session status on nodes. Keep connection details, relationship navigation, and node actions in an inspector beside the canvas, with a stacked layout for narrow workspaces. Clear selection on blank canvas clicks outside placement mode.
- Align the topology and server overview headers. Use the compact UI font in the native macOS topology dropdowns. Windows and Linux use the shared custom dropdown.
- Use the existing direction-aware page entrance for the topology header and canvas. Respect reduced-motion settings.
- Allow selecting and naming links in the inspector. Keep existing unnamed links readable and move each name with its line. Restore empty resource rings in the overview and use the shared search input.
- Add a multiline memo to each node in the inspector and persist it with the board. Keep connection profile notes independent.
