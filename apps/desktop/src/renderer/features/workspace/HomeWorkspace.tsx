import type { ConnectionProfile, ConnectionFolder } from '@termdock/core'
import { useState } from 'react'
import { t } from '../../i18n'
import { OverviewPage } from './OverviewPage'
import { QuickLinksPage } from './QuickLinksPage'

export function HomeWorkspace({
  profiles,
  folders = [],
  onOpen
}: {
  profiles: ConnectionProfile[]
  folders?: ConnectionFolder[]
  onOpen(profileId: string): void
}) {
  const [activeTab, setActiveTab] = useState<'overview' | 'quick-links' | 'ssh-manager' | 'settings'>('overview')

  const desktopApi = window.termdock

  const handleOpenNewConnection = () => {
    if (desktopApi) {
      void desktopApi.openConnectionFormWindow('create')
    }
  }

  const handleOpenConnectionManager = () => {
    if (desktopApi) {
      void desktopApi.openConnectionManagerWindow()
    }
  }

  const handleOpenDocs = () => {
    if (desktopApi) {
      void desktopApi.openExternalUrl('https://github.com/St0ff3l/termdock')
    }
  }

  const handleQuitApp = () => {
    if (desktopApi) {
      void desktopApi.requestQuitApp()
    }
  }

  return (
    <section className="home-workspace">
      {/* SideNavBar Component */}
      <aside className="home-sidebar">
        {/* macOS style Window Controls */}
        <div className="window-controls-decorator">
          <div className="dot dot-close"></div>
          <div className="dot dot-minimize"></div>
          <div className="dot dot-maximize"></div>
        </div>

        {/* Brand Header */}
        <div className="sidebar-brand">
          <h2 className="brand-title">TermDock</h2>
          <span className="brand-version">v1.2.0-stable</span>
        </div>

        {/* Navigation Section */}
        <nav className="sidebar-nav">
          <button
            className={`sidebar-nav-link ${activeTab === 'overview' ? 'active' : ''}`}
            onClick={() => setActiveTab('overview')}
            type="button"
          >
            <span className="material-symbols-outlined">dashboard</span>
            <span>概览</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'quick-links' ? 'active' : ''}`}
            onClick={() => setActiveTab('quick-links')}
            type="button"
          >
            <span className="material-symbols-outlined">link</span>
            <span>{t.quickConnect}</span>
          </button>
          <button
            className={`sidebar-nav-link`}
            onClick={handleOpenNewConnection}
            type="button"
          >
            <span className="material-symbols-outlined">terminal</span>
            <span>{t.newConnection}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'ssh-manager' ? 'active' : ''}`}
            onClick={handleOpenConnectionManager}
            type="button"
          >
            <span className="material-symbols-outlined">settings_ethernet</span>
            <span>{t.connectionManager}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={handleOpenConnectionManager}
            type="button"
          >
            <span className="material-symbols-outlined">settings</span>
            <span>{t.settings}</span>
          </button>
        </nav>

        {/* Sidebar Footer */}
        <div className="sidebar-footer">
          <button className="sidebar-nav-link" onClick={handleOpenDocs} type="button">
            <span className="material-symbols-outlined">description</span>
            <span>Docs</span>
          </button>
          <button className="sidebar-nav-link" onClick={handleOpenDocs} type="button">
            <span className="material-symbols-outlined">star</span>
            <span>GitHub</span>
          </button>
        </div>
      </aside>

      {/* Main Content Area */}
      <main className="home-main-content">
        <div className="home-content-body scrollbar-scroll">
          {activeTab === 'overview' && (
            <OverviewPage
              profiles={profiles}
              folders={folders}
              onOpenProfile={onOpen}
              onOpenNewConnection={handleOpenNewConnection}
              onOpenConnectionManager={handleOpenConnectionManager}
              onOpenDocs={handleOpenDocs}
            />
          )}
          {activeTab === 'quick-links' && (
            <QuickLinksPage
              profiles={profiles}
              folders={folders}
              onOpen={onOpen}
              onOpenNewConnection={handleOpenNewConnection}
            />
          )}
        </div>

        {/* Custom Footer */}
        <footer className="home-footer">
          <div className="footer-copyright">© 2026 TermDock Team. MIT Licensed. System: 0.1ms latency</div>
          <nav className="footer-nav">
            <button onClick={handleOpenDocs} type="button">Changelog</button>
            <button onClick={handleOpenDocs} type="button">API Reference</button>
            <button className="footer-status-link" type="button">
              <span className="footer-status-dot"></span>
              <span>Status</span>
            </button>
          </nav>
        </footer>
      </main>
    </section>
  )
}
