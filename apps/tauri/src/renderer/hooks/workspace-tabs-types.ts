import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import type {
  CommandExecutionOptions,
  ConnectionProfile,
  FileTermDesktopApi,
  LocalTerminalLaunchOptions,
  PaneFocusDirection,
  SessionSnapshot,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import type { OrderedTabEntry, TabContextTarget } from '../features/layout/tab-bar'
import type { SendScope, SessionSendTarget } from '../features/common/session-send-targets'
import type { AppLocale } from '../i18n'

export type LocalTab =
  | { id: string; kind: 'home'; title: string }
  | { id: string; kind: 'system'; title: string; sessionTabId: string; sourceTabTitle: string }

export type StoredMainTabUiState = {
  localTabs: LocalTab[]
  activeLocalTabId: string | null
  nextHomeTabNumber: number
  tabOrder: string[]
  systemSidebarCollapsedByTabId: Record<string, boolean>
}

export type TerminalDockSendState = {
  scope: SendScope
  selectedTabIds: string[]
  rememberSelection: boolean
}

export type WorkspaceTabContextMenu = {
  x: number
  y: number
  target: TabContextTarget
}

export type ShortcutCloseConfirm = {
  tabId: string
  title: string
  variant: 'connecting' | 'active-session' | 'active-last-session'
}

export type WorkspaceTabContextAction =
  'copy' | 'clone' | 'connect' | 'connectAll' | 'disconnect' | 'saveSessionLog' | 'close' | 'closeOthers' | 'closeAll'

export type WorkspaceStageKind = 'home' | 'session' | 'system'
export type WorkspaceNavigationDirection = 'up' | 'down'

export type UseWorkspaceTabsOptions = {
  desktopApi?: FileTermDesktopApi
  workspace: WorkspaceSnapshot
  isMainWorkspaceWindow: boolean
  hasLoadedInitialSnapshot: boolean
  locale: AppLocale
  isBusy: boolean
  closeActiveRequestVersion: number
  newTabRequestVersion: number
  splitPaneRequest: { id: number; direction: 'row' | 'column' } | null
  paneFocusRequest: { id: number; direction: PaneFocusDirection } | null
  onSnapshot(snapshot: WorkspaceSnapshot): void
  onBusyChange(isBusy: boolean): void
  onStatusMessage(message: string | null): void
  onError(scope: string, error: unknown): void
  onCloseCurrentWindow(): void
  onRequestQuit(): void
}

export type WorkspaceTabsDerivedState = {
  localTabs: LocalTab[]
  activeLocalTabId: string | null
  tabOrder: string[]
  visibleWorkspaceTabs: WorkspaceTab[]
  backgroundWorkspaceTabs: WorkspaceTab[]
  activeLocalTab: LocalTab | null
  activeTab: WorkspaceTab | null
  activeSession: SessionSnapshot | null
  activePaneTab: WorkspaceTab | null
  activePaneSession: SessionSnapshot | null
  activeProfile: ConnectionProfile | null
  activePaneProfile: ConnectionProfile | null
  activeLocalTabIdForUi: string | null
  activeWorkspaceOrderKey: string
  workspaceNavDirection: WorkspaceNavigationDirection
  orderedTabs: OrderedTabEntry[]
  sessionSendTargets: SessionSendTarget[]
  activeTerminalDockSendState: TerminalDockSendState
  isHomeWorkspaceVisible: boolean
  showSidebar: boolean
}

export type WorkspaceTabsActionContext = Pick<
  UseWorkspaceTabsOptions,
  | 'desktopApi'
  | 'workspace'
  | 'isMainWorkspaceWindow'
  | 'hasLoadedInitialSnapshot'
  | 'isBusy'
  | 'closeActiveRequestVersion'
  | 'newTabRequestVersion'
  | 'splitPaneRequest'
  | 'paneFocusRequest'
  | 'onSnapshot'
  | 'onBusyChange'
  | 'onStatusMessage'
  | 'onError'
  | 'onCloseCurrentWindow'
  | 'onRequestQuit'
> & {
  localTabs: LocalTab[]
  setLocalTabs: Dispatch<SetStateAction<LocalTab[]>>
  activeLocalTabId: string | null
  setActiveLocalTabId: Dispatch<SetStateAction<string | null>>
  nextHomeTabNumber: number
  setNextHomeTabNumber: Dispatch<SetStateAction<number>>
  tabOrder: string[]
  setTabOrder: Dispatch<SetStateAction<string[]>>
  visibleWorkspaceTabs: WorkspaceTab[]
  visibleActiveSessionTabId: string | null
  activeLocalTab: LocalTab | null
  activeTab: WorkspaceTab | null
  activePaneTab: WorkspaceTab | null
  isHomeWorkspaceVisible: boolean
  activeLocalTabIdForUi: string | null
  hasHydratedMainTabUiState: boolean
  activeTerminalDockSendState: TerminalDockSendState
  sessionSendTargets: SessionSendTarget[]
  setTerminalDockSendStateByTabId: Dispatch<SetStateAction<Record<string, TerminalDockSendState>>>
  setClosingSessionTabIds: Dispatch<SetStateAction<string[]>>
  shortcutCloseConfirm: ShortcutCloseConfirm | null
  setShortcutCloseConfirm: Dispatch<SetStateAction<ShortcutCloseConfirm | null>>
  tabContextMenu: WorkspaceTabContextMenu | null
  setTabContextMenu: Dispatch<SetStateAction<WorkspaceTabContextMenu | null>>
  draggingTabKey: string | null
  setDraggingTabKey: Dispatch<SetStateAction<string | null>>
  localTabsRef: MutableRefObject<LocalTab[]>
  pendingHomeReplacementKeyRef: MutableRefObject<string | null>
  pendingProfileOpenIdRef: MutableRefObject<string | null>
}

export type { CommandExecutionOptions, LocalTerminalLaunchOptions, PaneFocusDirection, SendScope, SessionSendTarget }
