import { useCallback, useEffect, useRef, useState } from 'react'
import type { BackupPasswordRequest, FileTermDesktopApi } from '@fileterm/core'

export type UseBackupPasswordInteractionsOptions = {
  desktopApi?: FileTermDesktopApi
  onError(scope: string, error: unknown): void
}

export type UseBackupPasswordInteractionsResult = {
  request: BackupPasswordRequest | null
  errorMessage: string | null
  isResolving: boolean
  cancel(): Promise<void>
  submit(value: string): Promise<void>
}

/**
 * Keeps remote-backup passwords in a short-lived renderer queue. This is a
 * separate route from terminal input and isolated remote-exec prompts so a
 * backup password can never be mistaken for shell input.
 */
export function useBackupPasswordInteractions({
  desktopApi,
  onError
}: UseBackupPasswordInteractionsOptions): UseBackupPasswordInteractionsResult {
  const [requests, setRequests] = useState<BackupPasswordRequest[]>([])
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [resolvingRequestId, setResolvingRequestId] = useState<string | null>(null)
  const resolvingRequestIdsRef = useRef(new Set<string>())
  const pendingRequestsRef = useRef<BackupPasswordRequest[]>([])
  const onErrorRef = useRef(onError)

  useEffect(() => {
    pendingRequestsRef.current = requests
  }, [requests])

  useEffect(() => {
    onErrorRef.current = onError
  }, [onError])

  useEffect(() => {
    if (!desktopApi) return
    const registrationId = `backup-password-renderer-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`}`
    let disposed = false
    let unsubscribe: (() => void) | undefined
    void desktopApi
      .onBackupPasswordRequest((nextRequest) => {
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
        await desktopApi.setBackupPasswordRendererReady(registrationId, true)
      })
      .catch((error) => {
        if (!disposed) {
          onErrorRef.current('启用远程备份密码输入', error)
          setErrorMessage(error instanceof Error ? error.message : String(error))
        }
      })
    return () => {
      disposed = true
      unsubscribe?.()
      void desktopApi.setBackupPasswordRendererReady(registrationId, false).catch(() => undefined)
    }
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi) return
    return () => {
      for (const request of pendingRequestsRef.current) {
        void desktopApi.resolveBackupPassword(request.requestId, true).catch(() => undefined)
      }
    }
  }, [desktopApi])

  const resolve = useCallback(
    async (requestId: string, cancelled: boolean, value?: string) => {
      if (!desktopApi || resolvingRequestIdsRef.current.has(requestId)) return
      resolvingRequestIdsRef.current.add(requestId)
      setResolvingRequestId(requestId)
      try {
        await desktopApi.resolveBackupPassword(requestId, cancelled, value)
        setRequests((current) => current.filter((item) => item.requestId !== requestId))
        setErrorMessage(null)
      } catch (error) {
        onError('响应远程备份密码输入', error)
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
