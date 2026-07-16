export type ManagerDropPosition = 'top' | 'bottom' | 'inside'

export function resolveManagerDropPosition(
  element: HTMLElement,
  clientY: number,
  canDropInside: boolean
): ManagerDropPosition {
  const rect = element.getBoundingClientRect()
  const y = clientY - rect.top
  if (canDropInside) {
    if (y < rect.height * 0.25) return 'top'
    if (y > rect.height * 0.75) return 'bottom'
    return 'inside'
  }
  return y < rect.height * 0.5 ? 'top' : 'bottom'
}

export function managerDropClass(isActiveTarget: boolean, position: ManagerDropPosition | null) {
  return isActiveTarget && position ? `drop-${position}` : ''
}
