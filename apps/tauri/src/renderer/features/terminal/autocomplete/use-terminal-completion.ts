import { useEffect, useRef, useState, type RefObject } from 'react'
import type { Terminal } from '@xterm/xterm'
import type { ConnectionProfile, SessionSnapshot, SessionType, TerminalCommandHistoryEntry } from '@fileterm/core'
import { t } from '../../../i18n'
import { FIG_COMMANDS, loadCommandSpec } from './fig-spec-loader'
import { CompletionInput, type InputScreen } from './completion-input'
import {
  commandCompletions,
  hasControlCharacters,
  pathCompletions,
  pathRequest,
  uniqueCompletions,
  type Completion
} from './completion-model'

export type CompletionController = {
  key(event: KeyboardEvent): boolean
  input(data: string): void
}
export type CompletionPopup = {
  items: Completion[]
  selected: number
  left: number
  top: number
  width: number
  height: number
  above: boolean
}

type Options = {
  terminalRef: RefObject<Terminal | null>
  controllerRef: RefObject<CompletionController | null>
  tabId: string
  profileId: string
  sessionType: SessionType | 'local'
  active: boolean
  available: boolean
  blocked: boolean
  composing: RefObject<boolean>
  replaying: RefObject<boolean>
}

export function useTerminalCompletion({
  terminalRef,
  controllerRef,
  tabId,
  profileId,
  sessionType,
  active,
  available,
  blocked,
  composing,
  replaying
}: Options) {
  const activeRef = useRef(active)
  activeRef.current = active
  const [popup, setPopup] = useState<CompletionPopup | null>(null)
  const acceptRef = useRef<(index: number) => void>(() => {})
  const selectRef = useRef<(index: number) => void>(() => {})

  useEffect(() => {
    if (!active) setPopup(null)
  }, [active])

  useEffect(() => {
    const terminal = terminalRef.current
    const api = window.fileterm
    if (!terminal || !api || sessionType !== 'ssh' || !available || blocked) {
      setPopup(null)
      return
    }
    let tracker = new CompletionInput()
    let enabled = false
    let disposed = false
    let revision = 0
    let items: Completion[] = []
    let selected = 0
    let history: TerminalCommandHistoryEntry[] = []
    let session: SessionSnapshot | undefined
    let snapshotRevision = 0
    let timer: ReturnType<typeof setTimeout> | undefined
    let requestRunning = false
    let cachedPath = ''
    let cachedFiles: SessionSnapshot['remoteFiles'] = []

    const screen = (): InputScreen => {
      const buffer = terminal.buffer.active
      const row = buffer.baseY + buffer.cursorY
      const line = buffer.getLine(row)
      return {
        row,
        before: line?.translateToString(false, 0, buffer.cursorX) ?? '',
        after: line?.translateToString(true, buffer.cursorX) ?? '',
        normal: buffer.type === 'normal',
        wrapped: Boolean(line?.isWrapped),
        atBottom: buffer.viewportY === buffer.baseY
      }
    }
    const hide = () => {
      revision++
      clearTimeout(timer)
      items = []
      setPopup(null)
    }
    const reset = () => {
      tracker.reset()
      hide()
    }
    const valid = () =>
      enabled &&
      activeRef.current &&
      !disposed &&
      !replaying.current &&
      !composing.current &&
      session?.deviceMode !== 'network-device' &&
      document.body?.getAttribute('data-history-open') !== 'true' &&
      document.activeElement === terminal.textarea &&
      tracker.matches(screen())
    const publish = (next: Completion[]) => {
      items = next
      selected = Math.min(selected, Math.max(0, items.length - 1))
      const element = terminal.element?.querySelector('.xterm-screen')
      const bounds = element?.getBoundingClientRect()
      const buffer = terminal.buffer.active
      if (!items.length || !valid() || !bounds || bounds.width <= 0 || bounds.height <= 0) {
        items = []
        setPopup(null)
        return
      }
      const width = Math.min(520, window.innerWidth - 16)
      const cursorBottom = bounds.top + ((buffer.cursorY + 1) * bounds.height) / terminal.rows
      const cursorTop = cursorBottom - bounds.height / terminal.rows
      const below = Math.max(0, window.innerHeight - cursorBottom - 12)
      const above = Math.max(0, cursorTop - 12)
      const openAbove = below < Math.min(320, above)
      const height = Math.min(320, openAbove ? above : below)
      if (height < 64) {
        items = []
        setPopup(null)
        return
      }
      setPopup({
        items,
        selected,
        left: Math.max(
          8,
          Math.min(bounds.left + (buffer.cursorX * bounds.width) / terminal.cols, window.innerWidth - width - 8)
        ),
        top: openAbove ? cursorTop - 4 : cursorBottom + 4,
        width,
        height,
        above: openAbove
      })
    }
    const refresh = async () => {
      hide()
      tracker.synchronize(screen())
      if (!valid()) return
      const requestRevision = revision
      selected = 0
      const input = tracker.input
      let completion
      try {
        completion = await commandCompletions(
          input,
          history.map((entry) => entry.command),
          t.terminalCompletionHistory,
          FIG_COMMANDS,
          loadCommandSpec
        )
      } catch {
        completion = {
          items: history
            .filter((entry) => entry.command.startsWith(input) && !hasControlCharacters(entry.command))
            .map((entry) => ({ value: entry.command, description: t.terminalCompletionHistory })),
          pathMode: null
        }
      }
      if (requestRevision !== revision || !valid() || tracker.input !== input) return
      const local = completion.items
      publish(local)
      // Shell and SFTP can use different roots; do not guess a namespace mapping.
      if (
        !completion.pathMode ||
        !session?.shellCwd ||
        session.shellCwd !== session.remotePath ||
        session.sftpUnavailableReason
      )
        return
      const cwd = session.shellCwd
      const request = pathRequest(input, cwd)
      if (!request) return
      const merge = (files: SessionSnapshot['remoteFiles']) =>
        publish(
          uniqueCompletions(input, [
            ...local,
            ...pathCompletions(input, cwd, files, t.terminalCompletionPath, completion.pathMode === 'folders')
          ])
        )
      if (cachedPath === request.path) {
        merge(cachedFiles)
        return
      }
      const currentRevision = revision
      timer = setTimeout(() => {
        if (requestRunning || disposed || currentRevision !== revision || !valid()) return
        requestRunning = true
        void api
          .listRemoteDirectory(tabId, request.path)
          .then((files) => {
            if (disposed || currentRevision !== revision || !valid() || tracker.input !== input) return
            cachedPath = request.path
            cachedFiles = files
            merge(files)
          })
          .catch(() => {
            // A missing or unavailable file channel leaves command/history suggestions usable.
          })
          .finally(() => {
            requestRunning = false
            if (!disposed && currentRevision !== revision && valid()) void refresh()
          })
      }, 160)
    }
    const accept = (index: number) => {
      const item = items[index]
      if (!item || !valid() || !item.value.startsWith(tracker.input)) {
        hide()
        return
      }
      const suffix = item.value.slice(tracker.input.length)
      if (!suffix || hasControlCharacters(suffix)) {
        hide()
        return
      }
      hide()
      // xterm routes this through the existing guarded input queue; no Enter or editing sequence.
      terminal.input(suffix, true)
      terminal.focus()
    }
    selectRef.current = (index) => {
      if (!valid() || !items[index] || selected === index) return
      selected = index
      publish(items)
    }
    acceptRef.current = accept
    controllerRef.current = {
      input(data) {
        hide()
        if (!enabled || replaying.current) {
          tracker.reset()
          return
        }
        tracker.observe(data, screen())
      },
      key(event) {
        if (event.type !== 'keydown') return false
        if (event.isComposing || event.keyCode === 229 || composing.current) {
          hide()
          return false
        }
        if (event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) {
          hide()
          return false
        }
        if (!valid()) {
          hide()
          return false
        }
        if (items.length && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
          selected = (selected + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length
          publish(items)
          return true
        }
        if (items.length && event.key === 'Tab') {
          accept(selected)
          return true
        }
        if (items.length && event.key === 'Escape') {
          reset()
          return true
        }
        return false
      }
    }
    const applyProfile = (profile: ConnectionProfile | undefined) => {
      const next =
        profile?.type === 'ssh' &&
        (profile.connectionOverrides?.terminalCommandCompletion ?? profile.terminalCommandCompletion ?? true)
      if (next !== enabled) {
        enabled = next
        if (!enabled) reset()
        else {
          tracker = new CompletionInput()
          tracker.synchronize(screen())
          void refresh()
        }
      }
    }
    const applySession = (snapshot: { sessions: Record<string, SessionSnapshot>; profiles: ConnectionProfile[] }) => {
      applyProfile(snapshot.profiles.find((profile) => profile.id === profileId))
      snapshotRevision++
      const next = snapshot.sessions[tabId]
      if (
        session &&
        (next?.shellCwd !== session.shellCwd ||
          next?.remotePath !== session.remotePath ||
          next?.fileAccessMode !== session.fileAccessMode ||
          next?.connected !== session.connected)
      ) {
        cachedPath = ''
        hide()
        if (!next?.connected) tracker.reset()
      }
      session = next
    }
    const initialSnapshotRevision = snapshotRevision
    void api
      .getSnapshot()
      .then((snapshot) => {
        if (!disposed && initialSnapshotRevision === snapshotRevision) applySession(snapshot)
      })
      .catch(() => {})
    let offSnapshot: (() => void) | undefined
    void api
      .onWorkspaceSnapshot(applySession)
      .then((off) => {
        if (disposed) off()
        else offSnapshot = off
      })
      .catch(() => {})
    const loadHistory = () => {
      void api
        .getTerminalCommandHistory(profileId)
        .then((entries) => {
          if (!disposed) history = entries
        })
        .catch(() => {})
    }
    loadHistory()
    const prompt = terminal.parser.registerOscHandler(7, (payload) => {
      if (!replaying.current && payload.startsWith('file://') && terminal.buffer.active.type === 'normal') {
        tracker.markPrompt()
        hide()
        cachedPath = ''
        loadHistory()
      }
      return false
    })
    const parsed = terminal.onWriteParsed(() => {
      if (terminal.buffer.active.type !== 'normal') {
        reset()
        return
      }
      void refresh()
    })
    const scroll = terminal.onScroll(hide)
    const resize = terminal.onResize(() => {
      tracker = new CompletionInput()
      void refresh()
    })
    const blur = () => hide()
    const compositionStart = () => hide()
    terminal.textarea?.addEventListener('blur', blur)
    terminal.textarea?.addEventListener('compositionstart', compositionStart)
    window.addEventListener('resize', reset)
    return () => {
      disposed = true
      hide()
      controllerRef.current = null
      acceptRef.current = () => {}
      selectRef.current = () => {}
      offSnapshot?.()
      prompt.dispose()
      parsed.dispose()
      scroll.dispose()
      resize.dispose()
      terminal.textarea?.removeEventListener('blur', blur)
      terminal.textarea?.removeEventListener('compositionstart', compositionStart)
      window.removeEventListener('resize', reset)
    }
  }, [terminalRef, controllerRef, tabId, profileId, sessionType, available, blocked, composing, replaying])

  return {
    popup,
    acceptCompletion: (index: number) => acceptRef.current(index),
    selectCompletion: (index: number) => selectRef.current(index)
  }
}
