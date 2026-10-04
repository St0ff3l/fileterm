type Rgb = [number, number, number]

function parseHex(value: string): { rgb: Rgb; alpha: number } | null {
  const match = /^#([\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i.exec(value)
  if (!match) return null
  const hex = match[1].length <= 4 ? [...match[1]].map((digit) => digit + digit).join('') : match[1]
  return {
    rgb: [0, 2, 4].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16)) as Rgb,
    alpha: hex.length === 8 ? Number.parseInt(hex.slice(6, 8), 16) / 255 : 1
  }
}

function luminance(rgb: Rgb): number {
  return rgb.reduce((sum, value, index) => {
    const channel = value / 255
    const linear = channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
    return sum + linear * [0.2126, 0.7152, 0.0722][index]
  }, 0)
}

/** Keep readable white labels; use a dark label on pale action backgrounds. */
export function actionTextColor(background: string, surface: string): string {
  const color = parseHex(background)
  if (!color) return '#ffffff'
  const backdrop = parseHex(surface)?.rgb ?? [255, 255, 255]
  const rgb = color.rgb.map((channel, index) => channel * color.alpha + backdrop[index] * (1 - color.alpha)) as Rgb
  const backgroundLuminance = luminance(rgb)
  const whiteContrast = 1.05 / (backgroundLuminance + 0.05)
  if (whiteContrast >= 4.5) return '#ffffff'
  const darkLuminance = luminance([26, 28, 31])
  const darkContrast =
    (Math.max(backgroundLuminance, darkLuminance) + 0.05) / (Math.min(backgroundLuminance, darkLuminance) + 0.05)
  return darkContrast > whiteContrast ? '#1a1c1f' : '#ffffff'
}
