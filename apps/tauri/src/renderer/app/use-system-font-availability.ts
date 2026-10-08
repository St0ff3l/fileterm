import { systemFontAvailability } from './font-stacks'

export function useSystemFontAvailability() {
  // System generic families can access Apple's protected UI fonts even when
  // local("SF Mono") fails. The bridge is the authoritative platform source.
  return systemFontAvailability(window.fileterm?.platform)
}
