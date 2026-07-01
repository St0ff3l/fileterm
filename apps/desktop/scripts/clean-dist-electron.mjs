import { rm } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = path.dirname(fileURLToPath(import.meta.url))
const desktopDir = path.resolve(scriptDir, '..')
const outputDir = path.join(desktopDir, 'dist-electron')

await rm(outputDir, { recursive: true, force: true })
console.log(`[FileTerm] cleaned Electron build directory: ${outputDir}`)
