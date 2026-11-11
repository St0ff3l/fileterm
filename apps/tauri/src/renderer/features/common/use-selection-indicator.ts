import { useLayoutEffect, useRef } from 'react'
import '../../components/common/selection-indicator/selection-indicator.css'

/** One highlight follows the selected control without moving its hit target. */
export function useSelectionIndicator<Element extends HTMLElement = HTMLDivElement>(
  value: string,
  selector: string,
  layoutKey?: unknown
) {
  const rootRef = useRef<Element>(null)
  const indicatorRef = useRef<HTMLSpanElement>(null)
  const animationRef = useRef<Animation | null>(null)
  const targetRef = useRef('')

  useLayoutEffect(() => {
    const root = rootRef.current
    const indicator = indicatorRef.current
    if (!root || !indicator) return
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)')
    const place = (animate: boolean) => {
      const selected = root.querySelector<HTMLElement>(selector)
      if (!selected) {
        delete root.dataset.selectionMotion
        targetRef.current = ''
        animationRef.current?.cancel()
        return
      }
      const box = selected.getBoundingClientRect()
      const parent = root.getBoundingClientRect()
      const x = box.left - parent.left - root.clientLeft + root.scrollLeft
      const y = box.top - parent.top - root.clientTop + root.scrollTop
      const target = `${x},${y},${box.width},${box.height}`
      if (target === targetRef.current) return
      const previous = indicator.getBoundingClientRect()
      const wasReady = root.dataset.selectionMotion === 'ready'
      animationRef.current?.cancel()
      Object.assign(indicator.style, {
        left: `${x}px`,
        top: `${y}px`,
        width: `${box.width}px`,
        height: `${box.height}px`
      })
      root.dataset.selectionMotion = 'ready'
      targetRef.current = target
      if (animate && wasReady && !reduced.matches && box.width && box.height && indicator.animate) {
        const next = indicator.getBoundingClientRect()
        const tokens = getComputedStyle(root)
        animationRef.current = indicator.animate(
          [
            {
              transform: `translate(${previous.left - next.left}px, ${previous.top - next.top}px) scale(${previous.width / next.width}, ${previous.height / next.height})`
            },
            { transform: 'none' }
          ],
          {
            duration: Number.parseFloat(tokens.getPropertyValue('--duration-move')) || 180,
            easing: tokens.getPropertyValue('--ease-in-out').trim() || 'ease-in-out'
          }
        )
      }
    }
    place(true)
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => place(false))
    observer?.observe(root)
    const selected = root.querySelector(selector)
    if (selected) observer?.observe(selected)
    const stopMotion = () => {
      if (reduced.matches) animationRef.current?.cancel()
    }
    reduced.addEventListener('change', stopMotion)
    return () => {
      observer?.disconnect()
      reduced.removeEventListener('change', stopMotion)
    }
  }, [value, selector, layoutKey])

  useLayoutEffect(
    () => () => {
      animationRef.current?.cancel()
      targetRef.current = ''
    },
    []
  )
  return { rootRef, indicatorRef }
}
