import { FILE_PANEL_SNAP_TARGET_SELECTORS } from './file-panel-snap'

/** Observe sidebar layout without subscribing to terminal repaint mutations. */
export function observeFilePanelLayout(
  root: HTMLElement,
  resizeObserver: Pick<ResizeObserver, 'observe' | 'unobserve'>,
  onLayout: () => void
): () => void {
  let sidebar: HTMLElement | null = null
  let observed: HTMLElement[] = []
  const sidebarObserver = new MutationObserver(() => {
    bindTargets()
    onLayout()
  })
  const bindTargets = () => {
    const nextSidebar = root.querySelector<HTMLElement>('.fs-sidebar')
    if (nextSidebar !== sidebar) {
      sidebarObserver.disconnect()
      sidebar = nextSidebar
      if (sidebar) {
        sidebarObserver.observe(sidebar, {
          attributes: true,
          attributeFilter: ['class', 'style'],
          childList: true,
          subtree: true
        })
      }
    }
    const targets =
      sidebar && !sidebar.classList.contains('is-collapsed')
        ? [
            sidebar,
            ...sidebar.querySelectorAll<HTMLElement>(
              ['.sys-card', ...Object.values(FILE_PANEL_SNAP_TARGET_SELECTORS)].join(',')
            )
          ]
        : []
    for (const element of observed) if (!targets.includes(element)) resizeObserver.unobserve(element)
    for (const element of targets) if (!observed.includes(element)) resizeObserver.observe(element)
    observed = targets
  }
  const shellObserver = new MutationObserver(() => {
    bindTargets()
    onLayout()
  })
  // The sidebar is a direct shell child. Replacement and shell sizing matter;
  // mutations under the sibling terminal/file tree do not affect snap targets.
  shellObserver.observe(root, { attributes: true, attributeFilter: ['class', 'style'], childList: true })
  bindTargets()
  return () => {
    shellObserver.disconnect()
    sidebarObserver.disconnect()
    observed.forEach((element) => resizeObserver.unobserve(element))
    observed = []
    sidebar = null
  }
}

export function shouldSyncFilePanelForResize(
  entries: ResizeObserverEntry[],
  workspace: HTMLElement,
  previousHeights: WeakMap<Element, number>
): boolean {
  let changed = false
  for (const entry of entries) {
    const previousHeight = previousHeights.get(entry.target)
    previousHeights.set(entry.target, entry.contentRect.height)
    if (
      (entry.target !== workspace && !entry.target.classList.contains('fs-sidebar')) ||
      previousHeight !== entry.contentRect.height
    ) {
      changed = true
    }
  }
  return changed
}
