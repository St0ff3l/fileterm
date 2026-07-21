export function WorkspaceLoadingState({ className = '', label = '正在加载…' }: { className?: string; label?: string }) {
  return (
    <div aria-label={label} aria-live="polite" className={`workspace-loading-state ${className}`.trim()} role="status">
      <span aria-hidden="true" className="workspace-loading-state__spinner" />
    </div>
  )
}
