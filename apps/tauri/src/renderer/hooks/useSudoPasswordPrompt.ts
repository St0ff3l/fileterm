import { useCallback, useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, SudoPasswordRequest } from '@fileterm/core'

export type UseSudoPasswordPromptOptions = {
  desktopApi?: FileTermDesktopApi
  onError(scope: string, error: unknown): void
}

export type UseSudoPasswordPromptResult = {
  request: SudoPasswordRequest | null
  errorMessage: string | null
  isResolving: boolean
  cancel(): Promise<void>
  submit(value: string, save: boolean): Promise<void>
}

/**
 * Owns the main-window-only sudo/su password queue. It deliberately does not
 * share the visible terminal or generic background-exec input routes.
 */
export function useSudoPasswordPrompt({
  desktopApi,
  onError
}: UseSudoPasswordPromptOptions): UseSudoPasswordPromptResult {
  const [requests, setRequests] = useState<SudoPasswordRequest[]>([])
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [resolvingRequestId, setResolvingRequestId] = useState<string | null>(null)
  const resolvingRequestIdsRef = useRef(new Set<string>())
  const pendingRequestsRef = useRef<SudoPasswordRequest[]>([])
  const onErrorRef = useRef(onError)

  useEffect(() => {
    pendingRequestsRef.current = requests
  }, [requests])

  useEffect(() => {
    onErrorRef.current = onError
  }, [onError])

  useEffect(() => {
    if (!desktopApi) return
    const registrationId = `sudo-password-renderer-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`}`
    let disposed = false
    let unsubscribe: (() => void) | undefined
    const unsubscribeCancelled = desktopApi.onSudoPasswordPromptCancelled(({ requestId }) => {
      setRequests((current) => current.filter((item) => item.requestId !== requestId))
      resolvingRequestIdsRef.current.delete(requestId)
      setResolvingRequestId((current) => (current === requestId ? null : current))
    })
    void desktopApi
      .onSudoPasswordPrompt((nextRequest) => {
        setRequests((current) => {
          const index = current.findIndex((item) => item.requestId === nextRequest.requestId)
          if (index === -1) return [...current, nextRequest]
          const next = [...current]
          next[index] = nextRequest
          return next
        })
        setErrorMessage(null)
      })
      .then(async (nextUnsubscribe) => {
        if (disposed) {
          nextUnsubscribe()
          return
        }
        unsubscribe = nextUnsubscribe
        await desktopApi.setSudoPasswordPromptRendererReady(registrationId, true)
      })
      .catch((error) => {
        if (!disposed) {
          onErrorRef.current('启用 sudo/su 密码输入', error)
          setErrorMessage(error instanceof Error ? error.message : String(error))
        }
      })
    return () => {
      disposed = true
      unsubscribe?.()
      unsubscribeCancelled()
      void desktopApi.setSudoPasswordPromptRendererReady(registrationId, false).catch(() => undefined)
    }
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi) return
    return () => {
      for (const request of pendingRequestsRef.current) {
        void desktopApi.resolveSudoPasswordPrompt(request.requestId, true).catch(() => undefined)
      }
    }
  }, [desktopApi])

  const resolve = useCallback(
    async (requestId: string, cancelled: boolean, value?: string, save = false) => {
      if (!desktopApi || resolvingRequestIdsRef.current.has(requestId)) return
      resolvingRequestIdsRef.current.add(requestId)
      setResolvingRequestId(requestId)
      try {
        await desktopApi.resolveSudoPasswordPrompt(requestId, cancelled, value, save)
        setRequests((current) => current.filter((item) => item.requestId !== requestId))
        setErrorMessage(null)
      } catch (error) {
        onError('响应 sudo/su 密码输入', error)
        setErrorMessage(error instanceof Error ? error.message : String(error))
      } finally {
        resolvingRequestIdsRef.current.delete(requestId)
        setResolvingRequestId((current) => (current === requestId ? null : current))
      }
    },
    [desktopApi, onError]
  )

  const request = requests[0] ?? null
  const cancel = useCallback(async () => {
    if (request) await resolve(request.requestId, true)
  }, [request, resolve])
  const submit = useCallback(
    async (value: string, save: boolean) => {
      if (request) await resolve(request.requestId, false, value, save)
    },
    [request, resolve]
  )

  return {
    request,
    errorMessage,
    isResolving: request?.requestId === resolvingRequestId,
    cancel,
    submit
  }
}
