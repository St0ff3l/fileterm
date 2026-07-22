import { t } from '../../i18n'

export function WorkspaceLoadingState({
  className = '',
  label = t.loadingWorkspace
}: {
  className?: string
  label?: string
}) {
  return (
    <div aria-label={label} aria-live="polite" className={`workspace-loading-state ${className}`.trim()} role="status">
      <span aria-hidden="true" className="workspace-loading-state__spinner" />
    </div>
  )
}
