import { readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

const rootDir = process.cwd()
const rootPackagePath = path.join(rootDir, 'package.json')
const rootPackage = JSON.parse(await readFile(rootPackagePath, 'utf8'))
const nextVersion = rootPackage.version?.trim()

if (!nextVersion) {
  console.error('Root package.json is missing a version field.')
  process.exit(1)
}

const packageJsonPaths = [
  'apps/desktop/package.json',
  'packages/core/package.json',
  'packages/shared/package.json',
  'packages/storage/package.json'
]

const internalPackages = new Set([
  '@fileterm/core',
  '@fileterm/shared',
  '@fileterm/storage'
])

function updateInternalDependencyVersions(record) {
  if (!record || typeof record !== 'object') {
    return
  }

  for (const packageName of internalPackages) {
    if (packageName in record) {
      record[packageName] = nextVersion
    }
  }
}

async function updateJsonFile(relativePath, mutate) {
  const targetPath = path.join(rootDir, relativePath)
  const raw = await readFile(targetPath, 'utf8')
  const parsed = JSON.parse(raw)
  mutate(parsed)
  await writeFile(targetPath, `${JSON.stringify(parsed, null, 2)}\n`, 'utf8')
}

await Promise.all(
  packageJsonPaths.map((relativePath) =>
    updateJsonFile(relativePath, (pkg) => {
      pkg.version = nextVersion
      updateInternalDependencyVersions(pkg.dependencies)
      updateInternalDependencyVersions(pkg.devDependencies)
      updateInternalDependencyVersions(pkg.peerDependencies)
      updateInternalDependencyVersions(pkg.optionalDependencies)
    })
  )
)

await updateJsonFile('package-lock.json', (lockfile) => {
  lockfile.version = nextVersion

  if (lockfile.packages && typeof lockfile.packages === 'object') {
    if (lockfile.packages[''] && typeof lockfile.packages[''] === 'object') {
      lockfile.packages[''].version = nextVersion
    }

    for (const [packagePath, pkg] of Object.entries(lockfile.packages)) {
      if (!pkg || typeof pkg !== 'object') {
        continue
      }

      if (
        packagePath === 'apps/desktop'
        || packagePath === 'packages/core'
        || packagePath === 'packages/shared'
        || packagePath === 'packages/storage'
      ) {
        pkg.version = nextVersion
      }

      updateInternalDependencyVersions(pkg.dependencies)
      updateInternalDependencyVersions(pkg.devDependencies)
      updateInternalDependencyVersions(pkg.peerDependencies)
      updateInternalDependencyVersions(pkg.optionalDependencies)
    }
  }
})

console.log(`Synced workspace package versions from root: ${nextVersion}`)
