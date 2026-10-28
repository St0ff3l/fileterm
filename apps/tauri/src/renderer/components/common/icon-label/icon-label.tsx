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
      const textOffset =
        (metrics.actualBoundingBoxAscent -
          metrics.actualBoundingBoxDescent -
          metrics.fontBoundingBoxAscent +
          metrics.fontBoundingBoxDescent) /
        2
      const icon = element.parentElement?.querySelector('svg')
      const iconBox = icon?.getBBox()
      const matrix = icon?.getScreenCTM()
      const labelBox = element.getBoundingClientRect()
      const iconCenter =
        iconBox && iconBox.width && iconBox.height && matrix && labelBox.height
          ? matrix.b * (iconBox.x + iconBox.width / 2) + matrix.d * (iconBox.y + iconBox.height / 2) + matrix.f
          : labelBox.top + labelBox.height / 2
      // These CJK glyphs appear high at compact Windows UI sizes.
      const opticalAdjustment =
        window.fileterm?.platform === 'win32' && /^['"]?(?:Inter|Noto Sans SC)['"]?(?:\s*,|$)/i.test(style.fontFamily)
          ? 1
          : 0
      const offset = textOffset + iconCenter - (labelBox.top + labelBox.height / 2) + opticalAdjustment
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
