# Manager list scroll height

The inline connection list inherited the modal body's 480px maximum height and left unused space below the scroll area. Connection, command, proxy/tunnel, SSH key and background-session lists should fill their panels, with four extra rows of space beneath the last entry and floating actions above the list.

The inline table now fills the available flex height. Its body has no maximum height, permits grid shrinking, and includes 144px of bottom padding for four 36px rows. MCP call logs and standalone dialogs are outside this adjustment. The proxy manager now uses the shared main-container class instead of an undefined content-container class.

Browser layout fixtures using the application's actual CSS reproduced a 480px body and 456px unused bottom gap before the change. After the change, 45 combinations of the five pages, three panel heights and empty/short/long lists fill the panel, preserve the floating action position and expose 144px beneath the last row when scrolled to the bottom.

Typecheck, lint, Prettier, renderer tests, Rust/contract tests and Clippy passed. CSS contract checks fail both before and after on existing legacy danger color aliases in `feedback-bubble.css`, with no new color declarations in this change. Native desktop and Windows rendering were not exercised.
