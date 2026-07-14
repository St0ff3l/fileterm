import { useCallback, useEffect, useRef, useState } from 'react'
import type {
  FileTermDesktopApi,
  SshCredentialsPromptRequest,
  SshHostVerificationRequest,
  SshKeyboardInteractiveRequest,
  SshInteractionRequest,
  SshInteractionResponse,
  SshKeyPassphrasePromptRequest
} from '@fileterm/core'
import { t } from '../i18n'

export type SshCredentialsInput = {
  username: string
  password: string
}

export type UseSshInteractionsOptions = {
  desktopApi?: FileTermDesktopApi
  onError(scope: string, error: unknown): void
}

export type UseSshInteractionsResult = {
  request: SshInteractionRequest | null
  credentialsRequest: SshCredentialsPromptRequest | null
  keyboardInteractiveRequest: SshKeyboardInteractiveRequest | null
  hostVerificationRequest: SshHostVerificationRequest | null
  keyPassphraseRequest: SshKeyPassphrasePromptRequest | null
  errorMessage: string | null
  resolve(requestId: string, response: SshInteractionResponse): Promise<void>
  cancelCredentials(): Promise<void>
  submitCredentials(input: SshCredentialsInput): Promise<void>
  cancelKeyPassphrase(): Promise<void>
  submitKeyPassphrase(input: { passphrase: string; savePassphrase: boolean }): Promise<void>
  cancelKeyboardInteractive(): Promise<void>
  submitKeyboardInteractive(answers: string[]): Promise<void>
  rejectHost(): Promise<void>
  acceptHostOnce(): Promise<void>
  acceptHostAndSave(): Promise<void>
}

export function useSshInteractions({ desktopApi, onError }: UseSshInteractionsOptions): UseSshInteractionsResult {
  const [requests, setRequests] = useState<SshInteractionRequest[]>([])
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const resolvingRequestIdsRef = useRef(new Set<string>())

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    const unsubscribe = desktopApi.onSshInteraction((nextRequest) => {
      setRequests((current) => {
        const existingIndex = current.findIndex((item) => item.requestId === nextRequest.requestId)
        if (existingIndex === -1) {
          return [...current, nextRequest]
        }

        const next = [...current]
        next[existingIndex] = nextRequest
        return next
      })
      setErrorMessage(null)
    })

    return () => {
      unsubscribe()
    }
  }, [desktopApi])

  const resolve = useCallback(
    async (requestId: string, response: SshInteractionResponse) => {
      if (!desktopApi || resolvingRequestIdsRef.current.has(requestId)) {
        return
      }

      resolvingRequestIdsRef.current.add(requestId)
      try {
        await desktopApi.resolveSshInteraction(requestId, response)
        setRequests((current) => current.filter((item) => item.requestId !== requestId))
        setErrorMessage(null)
      } catch (error) {
        onError('响应 SSH 交互', error)
        setErrorMessage(error instanceof Error ? error.message : String(error))
      } finally {
        resolvingRequestIdsRef.current.delete(requestId)
      }
    },
    [desktopApi, onError]
  )

  const request = requests[0] ?? null
  const credentialsRequest = request?.kind === 'credentials' ? request : null
  const keyboardInteractiveRequest = request?.kind === 'keyboard-interactive' ? request : null
  const hostVerificationRequest = request?.kind === 'host-verification' ? request : null
  const keyPassphraseRequest = request?.kind === 'key-passphrase' ? request : null

  const cancelCredentials = useCallback(async () => {
    if (!credentialsRequest) {
      return
    }

    await resolve(credentialsRequest.requestId, {
      kind: 'credentials',
      canceled: true
    })
  }, [credentialsRequest, resolve])

  const submitCredentials = useCallback(
    async ({ username: rawUsername, password }: SshCredentialsInput) => {
      if (!credentialsRequest) {
        return
      }

      const username = rawUsername.trim()
      if (!username || !password) {
        setErrorMessage(t.sshAuthPromptFillRequired)
        return
      }

      await resolve(credentialsRequest.requestId, {
        kind: 'credentials',
        canceled: false,
        username,
        password
      })
    },
    [credentialsRequest, resolve]
  )

  const cancelKeyPassphrase = useCallback(async () => {
    if (!keyPassphraseRequest) return
    await resolve(keyPassphraseRequest.requestId, { kind: 'key-passphrase', canceled: true })
  }, [keyPassphraseRequest, resolve])

  const submitKeyPassphrase = useCallback(
    async ({ passphrase, savePassphrase }: { passphrase: string; savePassphrase: boolean }) => {
      if (!keyPassphraseRequest) return
      if (!passphrase) {
        setErrorMessage('请输入私钥口令。')
        return
      }
      await resolve(keyPassphraseRequest.requestId, {
        kind: 'key-passphrase',
        canceled: false,
        passphrase,
        savePassphrase
      })
    },
    [keyPassphraseRequest, resolve]
  )

  const resolveHostVerification = useCallback(
    async (decision: 'accept-once' | 'accept-and-save' | 'cancel') => {
      if (!hostVerificationRequest) {
        return
      }

      await resolve(hostVerificationRequest.requestId, {
        kind: 'host-verification',
        decision
      })
    },
    [hostVerificationRequest, resolve]
  )

  const cancelKeyboardInteractive = useCallback(async () => {
    if (keyboardInteractiveRequest)
      await resolve(keyboardInteractiveRequest.requestId, { kind: 'keyboard-interactive', canceled: true })
  }, [keyboardInteractiveRequest, resolve])

  const submitKeyboardInteractive = useCallback(
    async (answers: string[]) => {
      if (keyboardInteractiveRequest)
        await resolve(keyboardInteractiveRequest.requestId, { kind: 'keyboard-interactive', canceled: false, answers })
    },
    [keyboardInteractiveRequest, resolve]
  )

  const rejectHost = useCallback(async () => {
    await resolveHostVerification('cancel')
  }, [resolveHostVerification])

  const acceptHostOnce = useCallback(async () => {
    await resolveHostVerification('accept-once')
  }, [resolveHostVerification])

  const acceptHostAndSave = useCallback(async () => {
    await resolveHostVerification('accept-and-save')
  }, [resolveHostVerification])

  return {
    request,
    credentialsRequest,
    keyboardInteractiveRequest,
    hostVerificationRequest,
    keyPassphraseRequest,
    errorMessage,
    resolve,
    cancelCredentials,
    submitCredentials,
    cancelKeyPassphrase,
    submitKeyPassphrase,
    cancelKeyboardInteractive,
    submitKeyboardInteractive,
    rejectHost,
    acceptHostOnce,
    acceptHostAndSave
  }
}
