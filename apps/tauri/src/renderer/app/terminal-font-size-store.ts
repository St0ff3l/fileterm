// Profile IDs remain stable across renderer sessions, unlike runtime tab IDs.
// Keep the scope aligned with the persisted file-panel ratio preferences.
const TERMINAL_FONT_SIZE_UI_STATE_KEY = 'ui.terminal-font-sizes.v1'

export const TERMINAL_DEFAULT_FONT_SIZE = 12
export const TERMINAL_MIN_FONT_SIZE = 8
export const TERMINAL_MAX_FONT_SIZE = 28

type TerminalFontSizeListener = () => void

const terminalFontSizes = new Map<string, number>()
const localOverrides = new Set<string>()
const listeners = new Set<TerminalFontSizeListener>()

let hasHydrated = false
let hydrationPromise: Promise<void> | null = null
let persistInFlight = false
let persistRequested = false

function getDesktopApi() {
  return typeof window === 'undefined' ? undefined : window.fileterm
}

export function clampTerminalFontSize(value: number) {
  if (!Number.isFinite(value)) {
    return TERMINAL_DEFAULT_FONT_SIZE
  }

  return Math.max(TERMINAL_MIN_FONT_SIZE, Math.min(TERMINAL_MAX_FONT_SIZE, Math.round(value)))
}

function notifyListeners() {
  for (const listener of listeners) {
    listener()
  }
}

function parseStoredTerminalFontSizes(raw: string | null) {
  if (!raw) {
    return new Map<string, number>()
  }

  try {
    const parsed: unknown = JSON.parse(raw)
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return new Map<string, number>()
    }

    return new Map(
      Object.entries(parsed).flatMap(([profileId, value]) => {
        if (!profileId || typeof value !== 'number' || !Number.isFinite(value)) {
          return []
        }

        return [[profileId, clampTerminalFontSize(value)] as const]
      })
    )
  } catch {
    return new Map<string, number>()
  }
}

function serializeTerminalFontSizes() {
  return JSON.stringify(Object.fromEntries(terminalFontSizes))
}

async function persistTerminalFontSizes() {
  const desktopApi = getDesktopApi()
  if (!desktopApi) {
    return
  }

  if (!hasHydrated) {
    await hydrateTerminalFontSizes()
  }

  while (persistRequested) {
    persistRequested = false
    try {
      await desktopApi.setUiStateItem(TERMINAL_FONT_SIZE_UI_STATE_KEY, serializeTerminalFontSizes())
    } catch {
      // A failed preference write must not interrupt terminal zoom. The in-memory
      // value remains authoritative for the current renderer session.
    }
  }
}

function requestPersistence() {
  if (!getDesktopApi()) {
    return
  }

  persistRequested = true
  if (persistInFlight) {
    return
  }

  persistInFlight = true
  void persistTerminalFontSizes().finally(() => {
    persistInFlight = false
    if (persistRequested) {
      requestPersistence()
    }
  })
}

export function getTerminalFontSize(profileId: string) {
  return terminalFontSizes.get(profileId) ?? TERMINAL_DEFAULT_FONT_SIZE
}

export function setTerminalFontSize(profileId: string, value: number) {
  if (!profileId) {
    return
  }

  const nextSize = clampTerminalFontSize(value)
  localOverrides.add(profileId)
  if (terminalFontSizes.get(profileId) === nextSize) {
    return
  }

  terminalFontSizes.set(profileId, nextSize)
  notifyListeners()
  requestPersistence()
}

export function subscribeTerminalFontSizes(listener: TerminalFontSizeListener) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function hydrateTerminalFontSizes() {
  if (hasHydrated) {
    return Promise.resolve()
  }
  if (hydrationPromise) {
    return hydrationPromise
  }

  const desktopApi = getDesktopApi()
  if (!desktopApi) {
    hasHydrated = true
    return Promise.resolve()
  }

  hydrationPromise = desktopApi
    .getUiStateItem(TERMINAL_FONT_SIZE_UI_STATE_KEY)
    .then((raw) => {
      const storedSizes = parseStoredTerminalFontSizes(raw)
      for (const [profileId, size] of storedSizes) {
        if (!localOverrides.has(profileId)) {
          terminalFontSizes.set(profileId, size)
        }
      }
    })
    .catch(() => {
      // Keep the default/in-memory values when the persisted state is unavailable.
    })
    .finally(() => {
      hasHydrated = true
      notifyListeners()
    })

  return hydrationPromise
}
