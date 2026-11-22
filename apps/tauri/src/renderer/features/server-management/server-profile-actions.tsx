import type { ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { t } from '../../i18n'
import { lastSessionTabIdForProfile } from '../../hooks/session-tab-memory'
import { statusForProfile } from './server-profile-status'
import './server-profile-actions.css'

export function ServerProfileActions({
  profile,
  workspaceTabs,
  sessions,
  focusedSessionTabId,
  onConnectProfile,
  onDisconnectSession,
  onActivateSession,
  onEditConnection
}: {
  profile: ConnectionProfile
  workspaceTabs: WorkspaceTab[]
  sessions: Record<string, SessionSnapshot>
  focusedSessionTabId?: string | null
  onConnectProfile(profileId: string): void
  onDisconnectSession(tabId: string): void | Promise<void>
  onActivateSession(tabId: string): void
  onEditConnection(profile: ConnectionProfile): void
}) {
  const status = statusForProfile(profile.id, workspaceTabs, sessions)
  const sessionTab = status?.tab ?? null
  const sessionConnected = Boolean(sessionTab && sessions[sessionTab.id]?.connected)
  const rememberedTab = [lastSessionTabIdForProfile(profile.id), focusedSessionTabId]
    .filter((tabId): tabId is string => Boolean(tabId))
    .map((tabId) => workspaceTabs.find((tab) => tab.id === tabId && tab.profileId === profile.id))
    .find((tab) => tab && sessions[tab.id]?.connected)

  return (
    <div className="server-profile-actions">
      {sessionConnected && sessionTab ? (
        <Button
          className="server-profile-actions__disconnect"
          icon={<MaterialIcon name="stop" />}
          onClick={() => void onDisconnectSession(sessionTab.id)}
          size="panel"
        >
          {t.disconnect}
        </Button>
      ) : (
        <Button
          disabled={status?.status === 'connecting'}
          icon={<MaterialIcon name="cloud_sync" />}
          onClick={() => onConnectProfile(profile.id)}
          size="panel"
        >
          <span className="server-profile-actions__label">
            {status?.status === 'connecting' ? t.serverDashboardConnecting : t.connect}
          </span>
        </Button>
      )}
      <Button
        disabled={!sessionConnected || !sessionTab}
        icon={<MaterialIcon name="terminal" />}
        onClick={() => {
          const target = rememberedTab ?? (sessionTab && sessions[sessionTab.id]?.connected ? sessionTab : null)
          if (target) onActivateSession(target.id)
        }}
        size="panel"
      >
        <span className="server-profile-actions__label">{t.serverInspectorTerminal}</span>
      </Button>
      <Button icon={<MaterialIcon name="edit" />} onClick={() => onEditConnection(profile)} size="panel">
        {t.edit}
      </Button>
    </div>
  )
}
