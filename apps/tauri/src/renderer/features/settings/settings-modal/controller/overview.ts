import type { OverviewSectionId, UiPreferences } from '@fileterm/core'
import { usePointerSortFallback, type PointerSortTarget } from '../../../../hooks/use-pointer-sort-fallback'
import { t } from '../../../../i18n'
import { resolveManagerDropPosition, type ManagerDropPosition } from '../../../common/manager-drag'
import { sameOverviewSectionOrder } from '../constants'
import type { SettingsModalState } from './state'

export function useOverviewSettingsController({ state }: { state: SettingsModalState }) {
  const {
    desktopApi,
    overviewShowStats,
    setOverviewShowStats,
    overviewShowRecent,
    setOverviewShowRecent,
    overviewShowAllConnections,
    setOverviewShowAllConnections,
    overviewShowQuickActions,
    setOverviewShowQuickActions,
    overviewSectionOrder,
    setOverviewSectionOrder,
    draggingOverviewSection,
    setDraggingOverviewSection,
    dragOverOverviewSection,
    setDragOverOverviewSection,
    overviewDragPosition,
    setOverviewDragPosition,
    isSavingOverviewPreference,
    setIsSavingOverviewPreference,
    overviewPreferenceError,
    setOverviewPreferenceError,
    overviewDragStateRef,
    suppressOverviewCardClickRef
  } = state

  const applyOverviewPreferences = (preferences: UiPreferences) => {
    setOverviewShowStats(preferences.overviewShowStats)
    setOverviewShowRecent(preferences.overviewShowRecent)
    setOverviewShowAllConnections(preferences.overviewShowAllConnections)
    setOverviewShowQuickActions(preferences.overviewShowQuickActions)
    setOverviewSectionOrder((currentOrder) =>
      sameOverviewSectionOrder(currentOrder, preferences.overviewSectionOrder)
        ? currentOrder
        : preferences.overviewSectionOrder
    )
  }

  const setOverviewShowStatsPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowStats) {
      return
    }

    const previousValue = overviewShowStats
    setOverviewShowStats(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowStats: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowStats(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const setOverviewShowRecentPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowRecent) {
      return
    }

    const previousValue = overviewShowRecent
    setOverviewShowRecent(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowRecent: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowRecent(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const setOverviewShowAllConnectionsPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowAllConnections) {
      return
    }

    const previousValue = overviewShowAllConnections
    setOverviewShowAllConnections(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowAllConnections: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowAllConnections(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const setOverviewShowQuickActionsPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowQuickActions) {
      return
    }

    const previousValue = overviewShowQuickActions
    setOverviewShowQuickActions(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowQuickActions: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowQuickActions(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const clearOverviewDragState = () => {
    overviewDragStateRef.current = { source: null, target: null, position: null }
    setDraggingOverviewSection(null)
    setDragOverOverviewSection(null)
    setOverviewDragPosition(null)
    window.setTimeout(() => {
      suppressOverviewCardClickRef.current = false
    }, 0)
  }

  const setOverviewDropTarget = (target: OverviewSectionId, position: ManagerDropPosition) => {
    if (overviewDragStateRef.current.target === target && overviewDragStateRef.current.position === position) {
      return
    }

    overviewDragStateRef.current.target = target
    overviewDragStateRef.current.position = position
    setDragOverOverviewSection(target)
    setOverviewDragPosition(position)
  }

  const positionForOverviewTarget = (target: PointerSortTarget | HTMLElement, clientY: number) => {
    if ('kind' in target && target.kind === 'overview-section-top') {
      return 'top' as const
    }

    const element = 'element' in target ? target.element : target
    return resolveManagerDropPosition(element, clientY, false)
  }

  const persistOverviewSectionOrder = (nextOrder: OverviewSectionId[], previousOrder: OverviewSectionId[]) => {
    if (!desktopApi || isSavingOverviewPreference) return

    setOverviewSectionOrder(nextOrder)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewSectionOrder: nextOrder })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewSectionOrder(previousOrder)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const applyOverviewSectionDrop = (
    source: OverviewSectionId,
    target: OverviewSectionId,
    position: ManagerDropPosition
  ) => {
    if (source === target || position === 'inside' || isSavingOverviewPreference) return

    const previousOrder = overviewSectionOrder
    const nextOrder = overviewSectionOrder.filter((sectionId) => sectionId !== source)
    const targetIndex = nextOrder.indexOf(target)
    if (targetIndex === -1) return

    nextOrder.splice(position === 'bottom' ? targetIndex + 1 : targetIndex, 0, source)
    if (nextOrder.every((sectionId, index) => sectionId === previousOrder[index])) return
    persistOverviewSectionOrder(nextOrder, previousOrder)
  }

  const handleOverviewPointerDown = usePointerSortFallback<OverviewSectionId>({
    onStart: (sectionId) => {
      if (isSavingOverviewPreference) return
      suppressOverviewCardClickRef.current = true
      overviewDragStateRef.current = { source: sectionId, target: null, position: null }
      setDraggingOverviewSection(sectionId)
    },
    onTarget: (source, target, clientY) => {
      if (source === target.id || (target.kind !== 'overview-section' && target.kind !== 'overview-section-top')) {
        return
      }
      setOverviewDropTarget(target.id as OverviewSectionId, positionForOverviewTarget(target, clientY))
    },
    onDrop: (source, target, clientY) => {
      if (
        target &&
        (target.kind === 'overview-section' || target.kind === 'overview-section-top') &&
        source !== target.id
      ) {
        applyOverviewSectionDrop(source, target.id as OverviewSectionId, positionForOverviewTarget(target, clientY))
      }
      clearOverviewDragState()
    },
    onCancel: clearOverviewDragState
  })

  const overviewSectionMeta: Record<OverviewSectionId, { title: string; hint: string }> = {
    stats: { title: t.overviewShowStats, hint: t.overviewShowStatsHint },
    recent: { title: t.overviewShowRecent, hint: t.overviewShowRecentHint },
    allConnections: { title: t.overviewShowAllConnections, hint: t.overviewShowAllConnectionsHint },
    quickActions: { title: t.overviewShowQuickActions, hint: t.overviewShowQuickActionsHint }
  }

  return {
    overviewShowStats,
    overviewShowRecent,
    overviewShowAllConnections,
    overviewShowQuickActions,
    overviewSectionOrder,
    overviewSectionMeta,
    draggingOverviewSection,
    dragOverOverviewSection,
    overviewDragPosition,
    isSavingOverviewPreference,
    overviewPreferenceError,
    suppressOverviewCardClickRef,
    setOverviewShowStatsPreference,
    setOverviewShowRecentPreference,
    setOverviewShowAllConnectionsPreference,
    setOverviewShowQuickActionsPreference,
    handleOverviewPointerDown,
    applyOverviewPreferences
  }
}
