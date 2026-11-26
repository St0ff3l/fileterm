import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import type { ConnectionFolder, ConnectionProfile, OverviewSectionId } from '@fileterm/core'
import { DEFAULT_OVERVIEW_SECTION_ORDER } from '@fileterm/core'
import { t } from '../../i18n'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { deviceRoleIcon } from '../../app/device-role'
import { AppIcon } from '../common/app-icon'
import { buildConnectionTree, flattenConnectionProfiles } from '../connections/connection-tree'
import './overview-page.css'

export function OverviewPage({
  profiles,
  folders = [],
  showStats,
  showRecent,
  showAllConnections,
  showQuickActions,
  sectionOrder,
  onOpenProfile,
  onOpenLocalTerminal,
  onOpenNewConnection,
  onOpenConnectionManager,
  onOpenCommandManager,
  onOpenDocs
}: {
  profiles: ConnectionProfile[]
  folders?: ConnectionFolder[]
  showStats: boolean
  showRecent: boolean
  showAllConnections: boolean
  showQuickActions: boolean
  sectionOrder: OverviewSectionId[]
  onOpenProfile(profileId: string): void
  onOpenLocalTerminal(): void
  onOpenNewConnection(): void
  onOpenConnectionManager(): void
  onOpenCommandManager(): void
  onOpenDocs(): void
}) {
  const recentProfiles = profiles
    .filter((profile) => profile.lastUsedAt != null)
    .sort((left, right) => (right.lastUsedAt ?? 0) - (left.lastUsedAt ?? 0))
    .slice(0, 6)
  const allProfiles = flattenConnectionProfiles(buildConnectionTree(profiles, folders).roots)
  const sshCount = profiles.filter((profile) => profile.type === 'ssh').length
  const telnetCount = profiles.filter((profile) => profile.type === 'telnet').length
  const ftpCount = profiles.filter((profile) => profile.type === 'ftp').length

  const renderConnectionCard = (profile: ConnectionProfile) => (
    <div
      key={profile.id}
      className="recent-card"
      onClick={(event) => {
        // Cards open on a single click; drop the second click of a habitual
        // double-click so it does not open another session.
        if (event.detail > 1) return
        onOpenProfile(profile.id)
      }}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault()
          onOpenProfile(profile.id)
        }
      }}
      role="button"
      tabIndex={0}
    >
      <div className="recent-card-header">
        <div className={`recent-icon recent-icon-${profile.type.toLowerCase()}`}>
          {profile.type === 'serial' ? (
            <MaterialIcon name="usb" className="material-symbols-outlined" />
          ) : (
            <AppIcon name={deviceRoleIcon(profile.deviceRole)} size={18} />
          )}
        </div>
        <div className={`type-badge type-badge-${profile.type.toLowerCase()}`}>{profile.type.toUpperCase()}</div>
      </div>
      <div className="recent-card-body">
        <h3 className="recent-name">{profile.name}</h3>
        <div className="recent-info">
          <span className="recent-user" title={connectionProfileSummary(profile)}>
            {connectionProfileSummary(profile)}
          </span>
        </div>
      </div>
      <div className="recent-card-footer">
        <button
          className="recent-action"
          onClick={(event) => {
            event.stopPropagation()
            if (event.detail > 1) return
            onOpenProfile(profile.id)
          }}
          type="button"
        >
          <MaterialIcon name="terminal" className="material-symbols-outlined" />
        </button>
      </div>
    </div>
  )

  const renderSection = (sectionId: OverviewSectionId) => {
    if (sectionId === 'stats') {
      return showStats ? (
        <section className="overview-stats" key={sectionId}>
          <div className="stat-card">
            <div className="stat-icon stat-icon-total">
              <MaterialIcon name="dns" className="material-symbols-outlined" />
            </div>
            <div className="stat-content">
              <div className="stat-value">{profiles.length}</div>
              <div className="stat-label">{t.overviewTotalConnections}</div>
            </div>
          </div>
          <div className="stat-card">
            <div className="stat-icon stat-icon-ssh">
              <MaterialIcon name="terminal" className="material-symbols-outlined" />
            </div>
            <div className="stat-content">
              <div className="stat-value">{sshCount}</div>
              <div className="stat-label">{t.overviewSshConnections}</div>
            </div>
          </div>
          <div className="stat-card">
            <div className="stat-icon stat-icon-telnet">
              <MaterialIcon name="settings_ethernet" className="material-symbols-outlined" />
            </div>
            <div className="stat-content">
              <div className="stat-value">{telnetCount}</div>
              <div className="stat-label">{t.overviewTelnetConnections}</div>
            </div>
          </div>
          <div className="stat-card">
            <div className="stat-icon stat-icon-ftp">
              <MaterialIcon name="cloud" className="material-symbols-outlined" />
            </div>
            <div className="stat-content">
              <div className="stat-value">{ftpCount}</div>
              <div className="stat-label">{t.overviewFtpConnections}</div>
            </div>
          </div>
        </section>
      ) : null
    }

    if (sectionId === 'recent') {
      return showRecent && recentProfiles.length > 0 ? (
        <section className="overview-recent" key={sectionId}>
          <div className="section-header">
            <h2 className="section-title">{t.overviewRecentConnections}</h2>
          </div>
          <div className="recent-grid">{recentProfiles.map(renderConnectionCard)}</div>
        </section>
      ) : null
    }

    if (sectionId === 'quickActions') {
      return showQuickActions ? (
        <section className="overview-actions" key={sectionId}>
          <div className="section-header">
            <h2 className="section-title">{t.overviewQuickActions}</h2>
          </div>
          <div className="action-grid">
            <button className="action-card" onClick={onOpenLocalTerminal} type="button">
              <div className="action-icon">
                <MaterialIcon name="terminal" className="material-symbols-outlined" />
              </div>
              <div className="action-content">
                <h3 className="action-title">{t.localTerminal}</h3>
                <p className="action-desc">{t.localTerminalDescription}</p>
              </div>
            </button>
            <button className="action-card" onClick={onOpenCommandManager} type="button">
              <div className="action-icon">
                <MaterialIcon name="terminal" className="material-symbols-outlined" />
              </div>
              <div className="action-content">
                <h3 className="action-title">{t.commandManager}</h3>
                <p className="action-desc">{t.overviewCommandManagerDescription}</p>
              </div>
            </button>
            <button className="action-card" onClick={onOpenConnectionManager} type="button">
              <div className="action-icon">
                <MaterialIcon name="tune" className="material-symbols-outlined" />
              </div>
              <div className="action-content">
                <h3 className="action-title">{t.connectionManager}</h3>
                <p className="action-desc">{t.overviewConnectionManagerDescription}</p>
              </div>
            </button>
            <button className="action-card" onClick={onOpenDocs} type="button">
              <div className="action-icon">
                <MaterialIcon name="description" className="material-symbols-outlined" />
              </div>
              <div className="action-content">
                <h3 className="action-title">{t.overviewDocsTitle}</h3>
                <p className="action-desc">{t.overviewDocsDescription}</p>
              </div>
            </button>
          </div>
        </section>
      ) : null
    }

    return showAllConnections && allProfiles.length > 0 ? (
      <section className="overview-all-connections" key={sectionId}>
        <div className="section-header">
          <h2 className="section-title">{t.overviewAllConnections}</h2>
        </div>
        <div className="recent-grid">{allProfiles.map(renderConnectionCard)}</div>
      </section>
    ) : null
  }

  const orderedSectionIds = [...sectionOrder, ...DEFAULT_OVERVIEW_SECTION_ORDER].filter(
    (sectionId, index, sections) => sections.indexOf(sectionId) === index
  )

  return (
    <div className="overview-page">
      <section className="overview-hero">
        <div className="hero-content">
          <h1 className="hero-title">{t.overviewWelcomeTitle}</h1>
          <p className="hero-subtitle">{t.overviewWelcomeSubtitle}</p>
          <div className="hero-actions">
            <button className="hero-btn hero-btn-primary" onClick={onOpenNewConnection} type="button">
              <MaterialIcon name="add" className="material-symbols-outlined" />
              <span>{t.newConnection}</span>
            </button>
            <button className="hero-btn hero-btn-secondary" onClick={onOpenConnectionManager} type="button">
              <MaterialIcon name="settings_ethernet" className="material-symbols-outlined" />
              <span>{t.connectionManager}</span>
            </button>
            <button className="hero-btn hero-btn-secondary" onClick={onOpenLocalTerminal} type="button">
              <MaterialIcon name="terminal" className="material-symbols-outlined" />
              <span>{t.localTerminal}</span>
            </button>
          </div>
        </div>
      </section>

      {orderedSectionIds.map(renderSection)}
    </div>
  )
}
