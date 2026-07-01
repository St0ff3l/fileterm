import type { ConnectionProfile, ConnectionFolder } from '@fileterm/core'
import { t } from '../../i18n'

export function OverviewPage({
  profiles,
  folders = [],
  onOpenProfile,
  onOpenNewConnection,
  onOpenConnectionManager,
  onOpenCommandManager,
  onOpenDocs
}: {
  profiles: ConnectionProfile[]
  folders?: ConnectionFolder[]
  onOpenProfile(profileId: string): void
  onOpenNewConnection(): void
  onOpenConnectionManager(): void
  onOpenCommandManager(): void
  onOpenDocs(): void
}) {
  const usedProfiles = profiles.filter(p => p.lastUsedAt != null)
  const recentProfiles = usedProfiles.length > 0
    ? [
        ...[...usedProfiles].sort((a, b) => (b.lastUsedAt ?? 0) - (a.lastUsedAt ?? 0)),
        ...profiles.filter(p => p.lastUsedAt == null)
      ]
    : profiles
  const sshCount = profiles.filter(p => p.type === 'ssh').length
  const ftpCount = profiles.filter(p => p.type === 'ftp').length
  const secureFtpCount = profiles.filter(p => p.type === 'ftp' && p.secure).length

  return (
    <div className="overview-page">
      {/* Hero Section */}
      <section className="overview-hero">
        <div className="hero-content">
          <h1 className="hero-title">{t.overviewWelcomeTitle}</h1>
          <p className="hero-subtitle">
            {t.overviewWelcomeSubtitle}
          </p>
          <div className="hero-actions">
            <button
              className="hero-btn hero-btn-primary"
              onClick={onOpenNewConnection}
              type="button"
            >
              <span className="material-symbols-outlined">add</span>
              <span>{t.newConnection}</span>
            </button>
            <button
              className="hero-btn hero-btn-secondary"
              onClick={onOpenConnectionManager}
              type="button"
            >
              <span className="material-symbols-outlined">settings_ethernet</span>
              <span>{t.connectionManager}</span>
            </button>
          </div>
        </div>
      </section>

      {/* Stats Cards */}
      <section className="overview-stats">
        <div className="stat-card">
          <div className="stat-icon stat-icon-total">
            <span className="material-symbols-outlined">dns</span>
          </div>
          <div className="stat-content">
            <div className="stat-value">{profiles.length}</div>
            <div className="stat-label">{t.overviewTotalConnections}</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon stat-icon-ssh">
            <span className="material-symbols-outlined">terminal</span>
          </div>
          <div className="stat-content">
            <div className="stat-value">{sshCount}</div>
            <div className="stat-label">{t.overviewSshConnections}</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon stat-icon-ftp">
            <span className="material-symbols-outlined">folder_open</span>
          </div>
          <div className="stat-content">
            <div className="stat-value">{secureFtpCount}</div>
            <div className="stat-label">{t.overviewSecureFtpConnections}</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon stat-icon-ftp">
            <span className="material-symbols-outlined">cloud</span>
          </div>
          <div className="stat-content">
            <div className="stat-value">{ftpCount}</div>
            <div className="stat-label">{t.overviewFtpConnections}</div>
          </div>
        </div>
      </section>

      {/* Recent Connections */}
      {recentProfiles.length > 0 && (
        <section className="overview-recent">
          <div className="section-header">
            <h2 className="section-title">{t.overviewRecentConnections}</h2>
          </div>
          <div className="recent-grid">
            {recentProfiles.map((profile) => (
              <div
                key={profile.id}
                className="recent-card"
                onClick={() => onOpenProfile(profile.id)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    onOpenProfile(profile.id)
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <div className="recent-card-header">
                  <div className={`recent-icon recent-icon-${profile.type.toLowerCase()}`}>
                    <span className="material-symbols-outlined">dns</span>
                  </div>
                  <div className={`type-badge type-badge-${profile.type.toLowerCase()}`}>
                    {profile.type.toUpperCase()}
                  </div>
                </div>
                <div className="recent-card-body">
                  <h3 className="recent-name">{profile.name}</h3>
                  <div className="recent-info">
                    <span className="recent-user">{profile.username}@{profile.host}</span>
                  </div>
                </div>
                <div className="recent-card-footer">
                  <button
                    className="recent-action"
                    onClick={(e) => {
                      e.stopPropagation()
                      onOpenProfile(profile.id)
                    }}
                    type="button"
                  >
                    <span className="material-symbols-outlined">terminal</span>
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Quick Actions */}
      <section className="overview-actions">
        <div className="section-header">
          <h2 className="section-title">{t.overviewQuickActions}</h2>
        </div>
        <div className="action-grid">
          <button
            className="action-card"
            onClick={onOpenCommandManager}
            type="button"
          >
            <div className="action-icon">
              <span className="material-symbols-outlined">terminal</span>
            </div>
            <div className="action-content">
              <h3 className="action-title">{t.commandManager}</h3>
              <p className="action-desc">{t.overviewCommandManagerDescription}</p>
            </div>
          </button>
          <button
            className="action-card"
            onClick={onOpenConnectionManager}
            type="button"
          >
            <div className="action-icon">
              <span className="material-symbols-outlined">tune</span>
            </div>
            <div className="action-content">
              <h3 className="action-title">{t.connectionManager}</h3>
              <p className="action-desc">{t.overviewConnectionManagerDescription}</p>
            </div>
          </button>
          <button
            className="action-card"
            onClick={onOpenDocs}
            type="button"
          >
            <div className="action-icon">
              <span className="material-symbols-outlined">description</span>
            </div>
            <div className="action-content">
              <h3 className="action-title">{t.overviewDocsTitle}</h3>
              <p className="action-desc">{t.overviewDocsDescription}</p>
            </div>
          </button>
        </div>
      </section>
    </div>
  )
}
