import { useCallback, useEffect, type Dispatch, type SetStateAction } from 'react'
import type { WorkspaceSnapshot } from '@fileterm/core'
import {
  DEFAULT_SIDEBAR_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_SNAP_THRESHOLD,
  STATUS_MESSAGE_TIMEOUT_MS,
  getSidebarMaxWidth
} from '../app/app-shell-utils'

export type AppResizeOptions = {
  isHomeWorkspaceVisible: boolean
  isResizingSidebar: boolean
  setIsResizingSidebar: Dispatch<SetStateAction<boolean>>
  setSidebarWidth: Dispatch<SetStateAction<number>>
  isResizingAiCopilot: boolean
  setIsResizingAiCopilot: Dispatch<SetStateAction<boolean>>
  isSystemSidebarCollapsed: boolean
  sidebarWidth: number
  setAiCopilotWidth: Dispatch<SetStateAction<number>>
  shouldShowAiCopilot: boolean
  error: string | null
  setError: Dispatch<SetStateAction<string | null>>
  windowCloseRequest: { isQuit: boolean } | null
  workspace: WorkspaceSnapshot
  requestWindowCloseConfirmation(isQuit: boolean, hasActiveConnections: boolean): void
  clearWindowCloseRequest(): void
}

export function useAppResize({
  isHomeWorkspaceVisible,
  isResizingSidebar,
  setIsResizingSidebar,
  setSidebarWidth,
  isResizingAiCopilot,
  setIsResizingAiCopilot,
  isSystemSidebarCollapsed,
  sidebarWidth,
  setAiCopilotWidth,
  shouldShowAiCopilot,
  error,
  setError,
  windowCloseRequest,
  workspace,
  requestWindowCloseConfirmation,
  clearWindowCloseRequest
}: AppResizeOptions) {
  const startSidebarResize = useCallback(() => {
    window.getSelection()?.removeAllRanges()
    document.body.classList.add('is-resizing-sidebar')
    setIsResizingSidebar(true)
  }, [])

  useEffect(() => {
    if (!isResizingSidebar) {
      return
    }
    const onMouseMove = (event: globalThis.MouseEvent) => {
      const maxWidth = getSidebarMaxWidth(window.innerWidth, isHomeWorkspaceVisible)
      const nextWidth = Math.min(maxWidth, Math.max(SIDEBAR_MIN_WIDTH, event.clientX))
      setSidebarWidth(
        Math.abs(nextWidth - DEFAULT_SIDEBAR_WIDTH) <= SIDEBAR_SNAP_THRESHOLD ? DEFAULT_SIDEBAR_WIDTH : nextWidth
      )
    }
    const onMouseUp = () => {
      window.getSelection()?.removeAllRanges()
      setIsResizingSidebar(false)
    }
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    window.addEventListener('blur', onMouseUp)
    document.body.classList.add('is-resizing-sidebar')
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      window.removeEventListener('blur', onMouseUp)
      document.body.classList.remove('is-resizing-sidebar')
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isHomeWorkspaceVisible, isResizingSidebar])

  useEffect(() => {
    if (!isHomeWorkspaceVisible) {
      return
    }
    const clampSidebarWidth = () => {
      const maxWidth = getSidebarMaxWidth(window.innerWidth, true)
      setSidebarWidth((currentWidth) => (currentWidth > maxWidth ? maxWidth : currentWidth))
    }
    clampSidebarWidth()
    window.addEventListener('resize', clampSidebarWidth)
    return () => window.removeEventListener('resize', clampSidebarWidth)
  }, [isHomeWorkspaceVisible])

  const startAiCopilotResize = useCallback(() => {
    window.getSelection()?.removeAllRanges()
    document.body.classList.add('is-resizing-copilot')
    setIsResizingAiCopilot(true)
  }, [])

  useEffect(() => {
    if (!isResizingAiCopilot) {
      return
    }
    const onMouseMove = (event: globalThis.MouseEvent) => {
      const windowWidth = window.innerWidth
      const rawWidth = windowWidth - event.clientX
      const currentLeftWidth = isSystemSidebarCollapsed ? 44 : sidebarWidth
      const minMainWorkspaceWidth = 460
      const maxAllowedWidth = Math.max(340, Math.min(600, windowWidth - currentLeftWidth - minMainWorkspaceWidth))
      const nextWidth = Math.min(maxAllowedWidth, Math.max(340, rawWidth))
      const defaultCopilotWidth = 368
      setAiCopilotWidth(Math.abs(nextWidth - defaultCopilotWidth) <= 12 ? defaultCopilotWidth : nextWidth)
    }
    const onMouseUp = () => {
      window.getSelection()?.removeAllRanges()
      setIsResizingAiCopilot(false)
    }
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    window.addEventListener('blur', onMouseUp)
    document.body.classList.add('is-resizing-copilot')
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      window.removeEventListener('blur', onMouseUp)
      document.body.classList.remove('is-resizing-copilot')
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isResizingAiCopilot, isSystemSidebarCollapsed, sidebarWidth])

  useEffect(() => {
    if (!shouldShowAiCopilot) return
    const handleWindowResize = () => {
      const windowWidth = window.innerWidth
      const currentLeftWidth = isSystemSidebarCollapsed ? 44 : sidebarWidth
      const minMainWorkspaceWidth = 460
      const maxAllowed = Math.max(340, Math.min(600, windowWidth - currentLeftWidth - minMainWorkspaceWidth))
      setAiCopilotWidth((prev) => (prev > maxAllowed ? maxAllowed : prev))
    }
    window.addEventListener('resize', handleWindowResize)
    return () => window.removeEventListener('resize', handleWindowResize)
  }, [shouldShowAiCopilot, isSystemSidebarCollapsed, sidebarWidth])

  useEffect(() => {
    if (!error) {
      return
    }
    const timeoutId = window.setTimeout(() => {
      setError((current) => (current === error ? null : current))
    }, STATUS_MESSAGE_TIMEOUT_MS)
    return () => window.clearTimeout(timeoutId)
  }, [error])

  useEffect(() => {
    if (!windowCloseRequest) {
      return
    }
    const hasActive = workspace.tabs.some((tab) =>
      Boolean(tab && (tab.status === 'connecting' || tab.status === 'connected'))
    )
    requestWindowCloseConfirmation(windowCloseRequest.isQuit, hasActive)
    clearWindowCloseRequest()
  }, [windowCloseRequest, workspace.tabs, requestWindowCloseConfirmation, clearWindowCloseRequest])

  return { startSidebarResize, startAiCopilotResize }
}
