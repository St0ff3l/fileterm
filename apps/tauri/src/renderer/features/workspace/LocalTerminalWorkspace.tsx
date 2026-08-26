import type { SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { TerminalView } from '../../components/TerminalView'
import { t } from '../../i18n'
import { LocalTerminalFrame } from './LocalTerminalFrame'
import { SplitPaneLayout } from './SplitPaneLayout'

/** A local shell is a normal workspace tab with its own PTY, not a home-page view. */
export function LocalTerminalWorkspace({
  activeTab,
  activeSession,
  onRestart,
  onCloseTab,
  splitRootTab,
  splitPaneSessions,
  activePaneTabId,
  onClosePane,
  onSplitPane,
  onActivatePane,
  onSetPaneWeights
}: {
  activeTab: WorkspaceTab
  activeSession: SessionSnapshot
  onRestart(tabId: string): Promise<void>
  onCloseTab(): void
  splitRootTab?: WorkspaceTab
  splitPaneSessions: Record<string, SessionSnapshot>
  activePaneTabId?: string
  onClosePane(paneTabId: string): void
  onSplitPane(paneTabId: string, direction: 'row' | 'column'): void
  onActivatePane(paneTabId: string): void
  onSetPaneWeights(panePath: number[], weights: number[]): void
}) {
  const isSplit = Boolean(splitRootTab?.paneRoot)

  return (
    <section className={`local-terminal-workspace ${isSplit ? 'is-terminal-split' : ''}`} aria-label={t.localTerminal}>
      <LocalTerminalFrame>
        {splitRootTab?.paneRoot ? (
          <SplitPaneLayout
            rootTab={splitRootTab}
            sessions={splitPaneSessions}
            activePaneTabId={activePaneTabId}
            onClosePane={onClosePane}
            onCloseTab={onCloseTab}
            onSplitPane={onSplitPane}
            onActivatePane={onActivatePane}
            onResizeEnd={onSetPaneWeights}
            onReconnectPane={onRestart}
            closedMessage={t.localTerminalExited}
            reconnectHint={t.pressEnterToRestartLocalTerminal}
          />
        ) : (
          <TerminalView
            profileId={activeTab.profileId}
            bootText={activeSession.terminalTranscript ?? ''}
            sessionType="local"
            closedMessage={t.localTerminalExited}
            connected={activeSession.connected}
            connecting={activeTab.status === 'connecting'}
            onReconnect={() => onRestart(activeTab.id)}
            onSplitPane={(direction) => onSplitPane(activeTab.id, direction)}
            onCloseTab={onCloseTab}
            reconnectHint={t.pressEnterToRestartLocalTerminal}
            tabId={activeTab.id}
          />
        )}
      </LocalTerminalFrame>
    </section>
  )
}
