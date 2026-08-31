// SSH authentication facade. Password/key/agent auth and keyboard-interactive
// MFA remain in one private session namespace so the same russh Handle is kept
// across partial-success continuation.

include!("authentication/common.rs");
include!("authentication/primary.rs");
include!("authentication/keyboard_interactive.rs");
