export type ServerManagementPage =
  | 'overview'
  | 'connection-manager'
  | 'proxy-manager'
  | 'command-manager'
  | 'ssh-key-manager'
  | 'background-sessions'
  | 'mcp-call-log'
  | 'settings'

export interface ServerManagementNavigation {
  page: ServerManagementPage
  selectedFolderId: string | null
  navigate(page: ServerManagementPage, folderId?: string | null): void
}
