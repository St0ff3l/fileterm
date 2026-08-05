import type { SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { TerminalView } from '../../components/TerminalView'
import { t } from '../../i18n'
import { LocalTerminalFrame } from './LocalTerminalFrame'

/** A local shell is a normal workspace tab with its own PTY, not a home-page view. */
export function LocalTerminalWorkspace({
  activeTab,
  activeSession,
  onRestart
}: {
  activeTab: WorkspaceTab
  activeSession: SessionSnapshot
  onRestart(): Promise<void>
}) {
  return (
    <section className="local-terminal-workspace" aria-label={t.localTerminal}>
      <LocalTerminalFrame>
        <TerminalView
          bootText={activeSession.terminalTranscript ?? ''}
          closedMessage={t.localTerminalExited}
          connected={activeSession.connected}
          connecting={activeTab.status === 'connecting'}
          onReconnect={onRestart}
          reconnectHint={t.pressEnterToRestartLocalTerminal}
          tabId={activeTab.id}
        />
      </LocalTerminalFrame>
    </section>
  )
}
