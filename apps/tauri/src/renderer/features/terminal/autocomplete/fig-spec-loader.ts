import manifest from './specs/index.json'
import type { CommandSpec } from './completion-spec'

const files = import.meta.glob<CommandSpec>(['./specs/*.json', '!./specs/index.json'], { import: 'default' })
const cache = new Map<string, Promise<CommandSpec | null>>()
export const FIG_COMMANDS = Object.entries(manifest.commands)
  .filter(([name]) => !name.includes('/'))
  .map(([value, entry]) => ({ value, description: entry.description }))

export function loadCommandSpec(name: string): Promise<CommandSpec | null> {
  const entry = (manifest.commands as Record<string, { file: string }>)[name]
  if (!entry) return Promise.resolve(null)
  const existing = cache.get(name)
  if (existing) return existing
  const loader = files[`./specs/${entry.file}`]
  if (!loader) return Promise.resolve(null)
  const pending = loader().catch((error: unknown) => {
    cache.delete(name)
    throw error
  })
  cache.set(name, pending)
  return pending
}
