import { invoke, transformCallback } from '@tauri-apps/api/core'

export function subscribe<T>(eventName: string, listener: (payload: T) => void) {
  // React strict mode can clean up the first mount before Tauri's asynchronous
  // listen registration resolves. Keep the callback inert immediately, ask
  // the backend to remove the event id, and only then release the JS callback.
  // Tauri's public unlisten helper currently performs those last two steps in
  // the opposite order, leaving a race where an in-flight event tries to call
  // an id that no longer exists during Windows hot reloads and child-window
  // teardown.
  const internals = window as unknown as {
    __TAURI_INTERNALS__?: { unregisterCallback?: (id: number) => void }
    __TAURI_EVENT_PLUGIN_INTERNALS__?: { unregisterListener?: (event: string, eventId: number) => void }
  }
  let active = true
  let eventId: number | null = null
  let unlistenStarted = false

  const callbackId = transformCallback((event: unknown) => {
    if (!active) return
    const payload = (event as { payload?: T })?.payload
    if (payload !== undefined) listener(payload)
  })

  const unregisterFrontend = (id: number) => {
    internals.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener?.(eventName, id)
    internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId)
  }

  const stopListening = () => {
    if (eventId === null || unlistenStarted) return
    unlistenStarted = true
    const registeredEventId = eventId
    void invoke<void>('plugin:event|unlisten', { event: eventName, eventId: registeredEventId })
      .then(() => unregisterFrontend(registeredEventId))
      .catch(() => {
        // Keep the inert callback registered if the backend did not confirm
        // removal. This is preferable to an event targeting a missing id.
      })
  }

  void invoke<number>('plugin:event|listen', {
    event: eventName,
    target: { kind: 'Any' },
    handler: callbackId
  })
    .then((id) => {
      eventId = id
      if (!active) stopListening()
    })
    .catch(() => internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId))

  return () => {
    active = false
    stopListening()
  }
}

/**
 * A subscription whose promise resolves only once Tauri has registered the
 * native event listener. Secure remote-exec prompts use this so the backend
 * never starts a task that can only wait for an unobservable renderer event.
 */
export function subscribeReady<T>(eventName: string, listener: (payload: T) => void): Promise<() => void> {
  const internals = window as unknown as {
    __TAURI_INTERNALS__?: { unregisterCallback?: (id: number) => void }
    __TAURI_EVENT_PLUGIN_INTERNALS__?: { unregisterListener?: (event: string, eventId: number) => void }
  }
  let active = true
  let eventId: number | null = null
  let unlistenStarted = false
  const callbackId = transformCallback((event: unknown) => {
    if (!active) return
    const payload = (event as { payload?: T })?.payload
    if (payload !== undefined) listener(payload)
  })
  const stopListening = () => {
    active = false
    if (eventId === null || unlistenStarted) return
    unlistenStarted = true
    const registeredEventId = eventId
    void invoke<void>('plugin:event|unlisten', { event: eventName, eventId: registeredEventId })
      .then(() => {
        internals.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener?.(eventName, registeredEventId)
        internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId)
      })
      .catch(() => undefined)
  }

  return invoke<number>('plugin:event|listen', {
    event: eventName,
    target: { kind: 'Any' },
    handler: callbackId
  })
    .then((id) => {
      eventId = id
      return stopListening
    })
    .catch((error) => {
      active = false
      internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId)
      throw error
    })
}
