import { MaterialIcon } from '../../../../components/common/material-icon/material-icon'
import { FeedbackBubble } from '../../../../components/common/feedback-bubble/feedback-bubble'
import type { PointerEvent as ReactPointerEvent } from 'react'
import type { FileTermDesktopApi, OverviewSectionId } from '@fileterm/core'
import { managerDropClass } from '../../../common/manager-drag'
import { targetsNestedManagerControl } from '../../../common/manager-interactions'
import { SelectionControl } from '../../../common/selection-control'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type OverviewContentSettingsContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  overviewSectionMeta: Record<OverviewSectionId, { title: string; hint: string }>
  draggingOverviewSection: OverviewSectionId | null
  dragOverOverviewSection: OverviewSectionId | null
  overviewDragPosition: 'top' | 'bottom' | 'inside' | null
  isSavingOverviewPreference: boolean
  overviewPreferenceError: string | null
  suppressOverviewCardClickRef: { current: boolean }
  setOverviewShowStatsPreference(nextValue: boolean): void
  setOverviewShowRecentPreference(nextValue: boolean): void
  setOverviewShowAllConnectionsPreference(nextValue: boolean): void
  setOverviewShowQuickActionsPreference(nextValue: boolean): void
  handleOverviewPointerDown(event: ReactPointerEvent<HTMLElement>, source: OverviewSectionId): void
}

/** Which overview sections show, in an order set by dragging. */
export function OverviewContentSettingsSection() {
  const {
    t,
    desktopApi,
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
    handleOverviewPointerDown
  } = useSettingsModalContext<OverviewContentSettingsContext>()

  return (
    <section className="settings-section">
      <h3>{t.overviewContentSettings}</h3>
      <p className="settings-tools-hint">{t.overviewContentSettingsHint}</p>
      <div
        aria-busy={isSavingOverviewPreference}
        className="overview-preference-list"
        inert={isSavingOverviewPreference}
      >
        {draggingOverviewSection && overviewSectionOrder[0] ? (
          <div
            aria-hidden="true"
            className="overview-preference-top-drop-zone"
            data-fileterm-sort-id={overviewSectionOrder[0]}
            data-fileterm-sort-kind="overview-section-top"
          />
        ) : null}
        {overviewSectionOrder.map((sectionId) => {
          const isDragging = draggingOverviewSection === sectionId
          const isDragOver = dragOverOverviewSection === sectionId
          const sectionMeta = overviewSectionMeta[sectionId]
          const checked =
            sectionId === 'stats'
              ? overviewShowStats
              : sectionId === 'recent'
                ? overviewShowRecent
                : sectionId === 'allConnections'
                  ? overviewShowAllConnections
                  : overviewShowQuickActions

          return (
            <label
              className={`overview-preference-row ${isDragging ? 'dragging' : ''} ${managerDropClass(isDragOver, overviewDragPosition)}`}
              data-fileterm-sort-id={sectionId}
              data-fileterm-sort-kind="overview-section"
              draggable={false}
              key={sectionId}
              onClick={(event) => {
                if (suppressOverviewCardClickRef.current) {
                  event.preventDefault()
                  event.stopPropagation()
                }
              }}
              onPointerDown={(event) => {
                if (!isSavingOverviewPreference && !targetsNestedManagerControl(event)) {
                  handleOverviewPointerDown(event, sectionId)
                }
              }}
            >
              <MaterialIcon
                name="drag_indicator"
                aria-label={t.overviewDragToReorder}
                className="material-symbols-outlined overview-preference-drag-handle"
                title={t.overviewDragToReorder}
              />
              <span className="overview-preference-copy">
                <strong>{sectionMeta.title}</strong>
                <p>{sectionMeta.hint}</p>
              </span>
              <span className="command-toggle overview-preference-toggle">
                <SelectionControl
                  checked={checked}
                  disabled={!desktopApi}
                  onChange={(event) => {
                    if (sectionId === 'stats') setOverviewShowStatsPreference(event.target.checked)
                    else if (sectionId === 'recent') setOverviewShowRecentPreference(event.target.checked)
                    else if (sectionId === 'allConnections') {
                      setOverviewShowAllConnectionsPreference(event.target.checked)
                    } else {
                      setOverviewShowQuickActionsPreference(event.target.checked)
                    }
                  }}
                  type="checkbox"
                />
              </span>
            </label>
          )
        })}
      </div>
      <FeedbackBubble message={overviewPreferenceError} sourceKey="settings.overview-content" tone="error" />
    </section>
  )
}
