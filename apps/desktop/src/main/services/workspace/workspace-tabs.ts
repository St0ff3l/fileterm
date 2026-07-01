import type { ConnectionProfile, TabLayout, WorkspaceTab } from '@fileterm/core'

function createTabLayout(profile: ConnectionProfile): TabLayout {
  return profile.type === 'ssh' ? 'terminal-file' : 'file-only'
}

export class WorkspaceTabsState {
  private tabs: WorkspaceTab[] = []
  private activeTabId: string | null = null

  list() {
    return [...this.tabs]
  }

  getActiveTabId() {
    return this.activeTabId
  }

  getById(tabId: string) {
    return this.tabs.find((tab) => tab.id === tabId) ?? null
  }

  has(tabId: string) {
    return this.tabs.some((tab) => tab.id === tabId)
  }

  open(tabId: string, profile: ConnectionProfile) {
    if (this.has(tabId)) {
      throw new Error(`Tab already exists: ${tabId}`)
    }

    const tab: WorkspaceTab = {
      id: tabId,
      profileId: profile.id,
      sessionType: profile.type,
      title: profile.name,
      layout: createTabLayout(profile),
      status: 'connecting'
    }

    this.tabs = [...this.tabs, tab]
    this.activeTabId = tabId
    return tab
  }

  activate(tabId: string) {
    if (!this.has(tabId)) {
      throw new Error(`Tab not found: ${tabId}`)
    }
    this.activeTabId = tabId
  }

  remove(tabId: string) {
    this.tabs = this.tabs.filter((tab) => tab.id !== tabId)
    if (this.activeTabId === tabId) {
      this.activeTabId = this.tabs.at(-1)?.id ?? null
    }
  }

  updateStatus(tabId: string, status: WorkspaceTab['status']) {
    this.tabs = this.tabs.map((tab) => (tab.id === tabId ? { ...tab, status } : tab))
  }
}
