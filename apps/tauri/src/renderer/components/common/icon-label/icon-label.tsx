import { useLayoutEffect, useRef } from 'react'

/** Align the visible glyphs, rather than font-dependent empty space, with the adjacent icon. */
export function SidebarAlignedLabel({ children, className }: { children: string; className?: string }) {
  const ref = useRef<HTMLSpanElement>(null)

  useLayoutEffect(() => {
    const element = ref.current
    const context = document.createElement('canvas').getContext('2d')
    if (!element || !context) return

    let frame = 0
    const measure = () => {
      element.style.transform = ''
      const style = getComputedStyle(element)
      context.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
      const metrics = context.measureText(element.textContent ?? '')
      const lineHeight = Number.parseFloat(style.lineHeight)
      const fontHeight = metrics.fontBoundingBoxAscent + metrics.fontBoundingBoxDescent
      let paddingTop = 0
      let paddingBottom = 0
      if (
        Number.isFinite(lineHeight) &&
        Number.isFinite(fontHeight) &&
        fontHeight > 0 &&
        Number.isFinite(metrics.actualBoundingBoxAscent) &&
        Number.isFinite(metrics.actualBoundingBoxDescent)
      ) {
        const leading = (lineHeight - fontHeight) / 2
        // Keep one pixel for antialiasing and fractional glyph boundaries.
        paddingTop =
          1 + Math.ceil(Math.max(0, metrics.actualBoundingBoxAscent - metrics.fontBoundingBoxAscent - leading))
        paddingBottom =
          1 + Math.ceil(Math.max(0, metrics.actualBoundingBoxDescent - metrics.fontBoundingBoxDescent - leading))
        element.style.paddingBlockStart = `${paddingTop}px`
        element.style.paddingBlockEnd = `${paddingBottom}px`
      } else {
        element.style.removeProperty('padding-block-start')
        element.style.removeProperty('padding-block-end')
      }
      const textOffset =
        (metrics.actualBoundingBoxAscent -
          metrics.actualBoundingBoxDescent -
          metrics.fontBoundingBoxAscent +
          metrics.fontBoundingBoxDescent -
          paddingTop +
          paddingBottom) /
        2
      const icon = element.parentElement?.querySelector('svg')
      const iconBox = icon?.getBBox()
      const matrix = icon?.getScreenCTM()
      const labelBox = element.getBoundingClientRect()
      const iconCenter =
        iconBox && iconBox.width && iconBox.height && matrix && labelBox.height
          ? matrix.b * (iconBox.x + iconBox.width / 2) + matrix.d * (iconBox.y + iconBox.height / 2) + matrix.f
          : labelBox.top + labelBox.height / 2
      const offset = textOffset + iconCenter - (labelBox.top + labelBox.height / 2)
      element.style.transform = Number.isFinite(offset) ? `translateY(${offset}px)` : ''
    }
    const schedule = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(measure)
    }
    measure()
    const observer = new MutationObserver(schedule)
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['style', 'class', 'data-theme'] })
    if (element.parentElement) observer.observe(element.parentElement, { attributes: true, attributeFilter: ['class'] })
    const sidebar = element.closest('aside')
    if (sidebar) observer.observe(sidebar, { attributes: true, attributeFilter: ['class'] })
    document.fonts.addEventListener('loadingdone', schedule)
    document.fonts.addEventListener('loadingerror', schedule)
    return () => {
      cancelAnimationFrame(frame)
      observer.disconnect()
      document.fonts.removeEventListener('loadingdone', schedule)
      document.fonts.removeEventListener('loadingerror', schedule)
    }
  }, [children])

  return (
    <span className={className} ref={ref}>
      {children}
    </span>
  )
}
