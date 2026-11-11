import { useLayoutEffect, useRef, type RefObject } from 'react'

/** Sample the current frame before interrupting an entrance or exit. */
export function useSurfaceMotion(
  ref: RefObject<HTMLElement | null>,
  open: boolean,
  entryTransform: string,
  durationToken = '--duration-sm'
) {
  const nodeRef = useRef<HTMLElement | null>(null)
  const animationRef = useRef<Animation | null>(null)
  useLayoutEffect(() => {
    const node = ref.current
    if (!node?.animate) return
    if (nodeRef.current !== node) {
      animationRef.current?.cancel()
      animationRef.current = null
      nodeRef.current = node
    }
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)')
    const run = () => {
      node.style.animation = 'none'
      const style = getComputedStyle(node)
      const previous = animationRef.current
        ? { opacity: style.opacity, transform: style.transform }
        : { opacity: open ? '0' : '1', transform: open && !reduced.matches ? entryTransform : 'none' }
      animationRef.current?.cancel()
      const target = { opacity: open ? '1' : '0', transform: open || reduced.matches ? 'none' : entryTransform }
      if (reduced.matches) previous.transform = 'none'
      animationRef.current = node.animate([previous, target], {
        duration:
          Number.parseFloat(
            style.getPropertyValue(open ? (reduced.matches ? '--duration-fade' : durationToken) : '--duration-exit')
          ) || 150,
        easing: style.getPropertyValue(open ? '--ease-out' : '--ease-in').trim() || (open ? 'ease-out' : 'ease-in'),
        fill: 'both'
      })
    }
    run()
    reduced.addEventListener('change', run)
    return () => reduced.removeEventListener('change', run)
  }, [ref, open, entryTransform, durationToken])
  useLayoutEffect(
    () => () => {
      animationRef.current?.cancel()
      animationRef.current = null
    },
    []
  )
}
