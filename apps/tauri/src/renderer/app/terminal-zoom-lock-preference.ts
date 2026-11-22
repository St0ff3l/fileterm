let terminalZoomLocked = false
let isTerminalZoomLockPreferenceLoaded = false

export function getTerminalZoomLockedPreference() {
  return terminalZoomLocked
}

export function hasTerminalZoomLockPreferenceLoaded() {
  return isTerminalZoomLockPreferenceLoaded
}

export function setTerminalZoomLockedPreference(value: boolean) {
  terminalZoomLocked = value
  isTerminalZoomLockPreferenceLoaded = true
}
