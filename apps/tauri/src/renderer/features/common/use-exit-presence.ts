import { useEffect, useState } from 'react'

/** Keep only the closing surface mounted; reopening cancels its removal. */
export function useExitPresence(open: boolean) {
  const [present, setPresent] = useState(open)
  useEffect(() => {
    if (open) {
      setPresent(true)
      return
    }
    const duration =
      Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--duration-exit')) || 150
    const timer = window.setTimeout(() => setPresent(false), duration)
    return () => window.clearTimeout(timer)
  }, [open])
  return open || present
}
