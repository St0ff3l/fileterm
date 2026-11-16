import { readFile, writeFile, mkdir, readdir, unlink } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { dirname, join } from 'node:path'
import completions from '@withfig/autocomplete/dynamic'

const require = createRequire(import.meta.url)
const packageRoot = dirname(dirname(require.resolve('@withfig/autocomplete')))
const destination = fileURLToPath(new URL('../src/renderer/features/terminal/autocomplete/specs/', import.meta.url))
const fields = new Set([
  'name',
  'description',
  'subcommands',
  'options',
  'args',
  'suggestions',
  'template',
  'loadSpec',
  'isOptional',
  'isVariadic',
  'isPersistent',
  'isRepeatable',
  'isCommand',
  'insertValue',
  'hidden',
  'exclusiveOn',
  'dependsOn'
])

// Keep declarative rules only. Never call generators, loadSpec functions or command scripts.
export function staticSpec(value, depth = 0) {
  if (depth > 48) throw new Error('Completion specification nesting exceeds 48 levels')
  if (['string', 'boolean', 'number'].includes(typeof value)) return value
  if (Array.isArray(value)) return value.map((item) => staticSpec(item, depth + 1)).filter((item) => item !== undefined)
  if (!value || typeof value !== 'object') return undefined
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => fields.has(key))
      .map(([key, item]) => [key, staticSpec(item, depth + 1)])
      .filter(([, item]) => item !== undefined)
  )
}

async function writeChanged(path, content) {
  const previous = await readFile(path, 'utf8').catch((error) => {
    if (error.code !== 'ENOENT') throw error
    return null
  })
  if (previous !== content) await writeFile(path, content)
}

async function main() {
  const { version } = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8'))
  await mkdir(destination, { recursive: true })
  const index = {}
  const skipped = []
  for (const [name, load] of Object.entries(completions).sort(([a], [b]) => a.localeCompare(b, 'en'))) {
    const module = await load()
    const spec = staticSpec(module.default)
    if (!spec?.name || (typeof spec.name !== 'string' && !Array.isArray(spec.name))) {
      skipped.push(name)
      continue
    }
    const file = encodeURIComponent(name).replaceAll('%', '_') + '.json'
    await writeChanged(join(destination, file), JSON.stringify(spec) + '\n')
    index[name] = { file, description: spec.description ?? '' }
  }
  const manifest = {
    source: 'https://github.com/withfig/autocomplete',
    package: '@withfig/autocomplete',
    version,
    license: 'MIT',
    copyright: 'Copyright (c) 2021 Hercules Labs Inc. (Fig)',
    count: Object.keys(index).length,
    skipped,
    commands: index
  }
  await writeChanged(join(destination, 'index.json'), JSON.stringify(manifest) + '\n')
  await writeChanged(join(destination, 'LICENSE.fig.txt'), await readFile(join(packageRoot, 'LICENSE'), 'utf8'))
  const expected = new Set(['index.json', 'LICENSE.fig.txt', ...Object.values(index).map((entry) => entry.file)])
  for (const file of await readdir(destination))
    if (!expected.has(file) && file.endsWith('.json')) await unlink(join(destination, file))
  console.log(
    `Bundled ${manifest.count} static Fig specifications from ${version}; ${skipped.length} function-only/version-selected definitions skipped.`
  )
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) await main()
