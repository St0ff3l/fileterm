// Remembers the most recent terminal tab per server so returning from the
// overview reopens the tab the user left instead of the first one. Kept in a
// module store so it survives home-tab remounts and snapshot timing.
const lastSessionTabIdByProfile = new Map<string, string>()

export function rememberSessionTab(tabId: string, profileId: string) {
  lastSessionTabIdByProfile.set(profileId, tabId)
}

export function lastSessionTabIdForProfile(profileId: string): string | null {
  return lastSessionTabIdByProfile.get(profileId) ?? null
}
