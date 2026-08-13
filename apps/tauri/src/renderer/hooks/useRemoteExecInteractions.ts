import { useCallback, useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, RemoteExecInteractionRequest } from '@fileterm/core'

export type UseRemoteExecInteractionsOptions = {
  desktopApi?: FileTermDesktopApi
  onError(scope: string, error: unknown): void
}

export type UseRemoteExecInteractionsResult = {
  request: RemoteExecInteractionRequest | null
  errorMessage: string | null
  isResolving: boolean
  cancel(): Promise<void>
  submit(value: string): Promise<void>
}

/**
 * Renderer queue for task-local remote exec prompts. Unlike SSH login
 * interactions, these answers are never persisted and never touch the visible
 * terminal input route.
 */
export function useRemoteExecInteractions({
  desktopApi,
  onError
}: UseRemoteExecInteractionsOptions): UseRemoteExecInteractionsResult {
  const [requests, setRequests] = useState<RemoteExecInteractionRequest[]>([])
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [resolvingRequestId, setResolvingRequestId] = useState<string | null>(null)
  const resolvingRequestIdsRef = useRef(new Set<string>())
  const pendingRequestsRef = useRef<RemoteExecInteractionRequest[]>([])
  const onErrorRef = useRef(onError)

  useEffect(() => {
    pendingRequestsRef.current = requests
  }, [requests])

  useEffect(() => {
    onErrorRef.current = onError
  }, [onError])

  useEffect(() => {
    if (!desktopApi) return
    // A new id per listener lifetime means a stale Strict Mode / HMR cleanup
    // can never withdraw readiness established by the listener that replaced
    // it. This runs in an effect, not during render.
    const registrationId = `remote-exec-renderer-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`}`
    let disposed = false
    let unsubscribe: (() => void) | undefined
    void desktopApi
      .onRemoteExecInteraction((nextRequest) => {
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
        await desktopApi.setRemoteExecInteractionRendererReady(registrationId, true)
      })
      .catch((error) => {
        if (!disposed) {
          onErrorRef.current('启用远程命令安全输入', error)
          setErrorMessage(error instanceof Error ? error.message : String(error))
        }
      })
    return () => {
      disposed = true
      unsubscribe?.()
      // Best effort during window teardown. The backend drops pending task
      // senders immediately, so a secret prompt can never outlive this UI.
      void desktopApi.setRemoteExecInteractionRendererReady(registrationId, false).catch(() => undefined)
    }
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi) return
    return () => {
      // This hook owns the only renderer route for these task-local prompts.
      // Fail closed if the workspace unmounts; values must never be retained
      // or redirected into the visible terminal.
      for (const request of pendingRequestsRef.current) {
        void desktopApi.resolveRemoteExecInteraction(request.requestId, true).catch(() => undefined)
      }
    }
  }, [desktopApi])

  const resolve = useCallback(
    async (requestId: string, cancelled: boolean, value?: string) => {
      if (!desktopApi || resolvingRequestIdsRef.current.has(requestId)) return
      resolvingRequestIdsRef.current.add(requestId)
      setResolvingRequestId(requestId)
      try {
        await desktopApi.resolveRemoteExecInteraction(requestId, cancelled, value)
        setRequests((current) => current.filter((item) => item.requestId !== requestId))
        setErrorMessage(null)
      } catch (error) {
        onError('响应远程命令交互', error)
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
    async (value: string) => {
      if (request) await resolve(request.requestId, false, value)
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
