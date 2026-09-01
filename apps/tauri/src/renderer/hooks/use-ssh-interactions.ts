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
import {
  createSshInteractionQueue,
  enqueueSshInteraction,
  getActiveSshInteraction,
  removeSshInteraction
} from './ssh-interaction-queue'

export type SshCredentialsInput = {
  username: string
  password: string
}

export type UseSshInteractionsOptions = {
  desktopApi?: FileTermDesktopApi
  isMainWorkspaceWindow?: boolean
  isConnectionFormWindow?: boolean
  isConnectionFormOpen?: boolean
  onError(scope: string, error: unknown): void
}

export type UseSshInteractionsResult = {
  request: SshInteractionRequest | null
  credentialsRequest: SshCredentialsPromptRequest | null
  keyboardInteractiveRequest: SshKeyboardInteractiveRequest | null
  hostVerificationRequest: SshHostVerificationRequest | null
  keyPassphraseRequest: SshKeyPassphrasePromptRequest | null
  errorMessage: string | null
  isResolving: boolean
  waitForSshInteractionListener(): Promise<void>
  resolve(requestId: string, response: SshInteractionResponse): Promise<void>
  cancelCredentials(): Promise<void>
  submitCredentials(input: SshCredentialsInput): Promise<void>
  cancelKeyboardInteractive(): Promise<void>
  submitKeyboardInteractive(answers: string[]): Promise<void>
  cancelKeyPassphrase(): Promise<void>
  submitKeyPassphrase(input: { passphrase: string; savePassphrase: boolean }): Promise<void>
  rejectHost(): Promise<void>
  acceptHostOnce(): Promise<void>
  acceptHostAndSave(): Promise<void>
}

export function useSshInteractions({
  desktopApi,
  isMainWorkspaceWindow = false,
  isConnectionFormWindow = false,
  isConnectionFormOpen = false,
  onError
}: UseSshInteractionsOptions): UseSshInteractionsResult {
  const [queue, setQueue] = useState(createSshInteractionQueue)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [resolvingRequestId, setResolvingRequestId] = useState<string | null>(null)
  const resolvingRequestIdsRef = useRef(new Set<string>())
  const sshInteractionRegistrationRef = useRef<Promise<void>>(Promise.resolve())
  const onErrorRef = useRef(onError)
  onErrorRef.current = onError

  useEffect(() => {
    if (!desktopApi) {
      sshInteractionRegistrationRef.current = Promise.resolve()
      return
    }

    let active = true
    let unsubscribe: (() => void) | null = null
    const registration = desktopApi
      .onSshInteraction((nextRequest) => {
        // Connection tests are owned by the form that started them. Normal
        // SSH sessions are owned by the main workspace. Every Tauri window
        // receives the app-wide event, so filter here before a request can be
        // resolved by the wrong renderer or become invisible in a child
        // window.
        const isConnectionTest = nextRequest.tabId.startsWith('connection-test-')
        const canHandleRequest = isConnectionTest
          ? isConnectionFormWindow || isConnectionFormOpen
          : isMainWorkspaceWindow
        if (!canHandleRequest) {
          return
        }

        void desktopApi.showCurrentWindow().catch(() => undefined)
        setQueue((current) => enqueueSshInteraction(current, nextRequest))
        setErrorMessage(null)
      })
      .then((stopListening) => {
        if (!active) {
          stopListening()
          return
        }
        unsubscribe = stopListening
      })
      .catch((error) => {
        if (active) {
          onErrorRef.current('注册 SSH 交互监听', error)
          setErrorMessage(error instanceof Error ? error.message : String(error))
        }
        throw error
      })
    sshInteractionRegistrationRef.current = registration
    // A renderer can exist without an active connection test. Avoid an
    // unhandled rejection in that case while still letting a test that was
    // waiting for registration receive the original error.
    void registration.catch(() => undefined)

    return () => {
      active = false
      void registration.then(
        () => {
          unsubscribe?.()
        },
        () => undefined
      )
    }
  }, [desktopApi, isConnectionFormOpen, isConnectionFormWindow, isMainWorkspaceWindow])

  const waitForSshInteractionListener = useCallback(async () => {
    await sshInteractionRegistrationRef.current
  }, [])

  const resolve = useCallback(
    async (requestId: string, response: SshInteractionResponse) => {
      if (!desktopApi || resolvingRequestIdsRef.current.has(requestId)) {
        return
      }

      resolvingRequestIdsRef.current.add(requestId)
      setResolvingRequestId(requestId)
      try {
        await desktopApi.resolveSshInteraction(requestId, response)
        setQueue((current) => removeSshInteraction(current, requestId))
        setErrorMessage(null)
      } catch (error) {
        onError('响应 SSH 交互', error)
        setErrorMessage(error instanceof Error ? error.message : String(error))
      } finally {
        resolvingRequestIdsRef.current.delete(requestId)
        setResolvingRequestId((current) => (current === requestId ? null : current))
      }
    },
    [desktopApi, onError]
  )

  const request = getActiveSshInteraction(queue)
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
        setErrorMessage(t.sshKeyPassphraseEmpty)
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
    isResolving: Boolean(request && resolvingRequestId === request.requestId),
    waitForSshInteractionListener,
    resolve,
    cancelCredentials,
    submitCredentials,
    cancelKeyboardInteractive,
    submitKeyboardInteractive,
    cancelKeyPassphrase,
    submitKeyPassphrase,
    rejectHost,
    acceptHostOnce,
    acceptHostAndSave
  }
}
