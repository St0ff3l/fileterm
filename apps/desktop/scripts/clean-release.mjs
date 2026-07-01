import fs from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const releaseDir = path.resolve(__dirname, '../release')

try {
  const entries = await fs.readdir(releaseDir)

  await Promise.all(entries.map(async (entry) => {
    const targetPath = path.join(releaseDir, entry)
    await fs.rm(targetPath, { recursive: true, force: true })
  }))

  console.log(`[FileTerm] cleaned release directory: ${releaseDir}`)
} catch (error) {
  if ((error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT')) {
    console.log(`[FileTerm] release directory does not exist: ${releaseDir}`)
  } else {
    throw error
  }
}
