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
  'apps/tauri/package.json',
  'packages/core/package.json',
  'packages/shared/package.json',
  'packages/storage/package.json'
]

const internalPackages = new Set(['@fileterm/core', '@fileterm/shared', '@fileterm/storage'])

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
  await writeFileWithExistingLineEndings(targetPath, raw, `${JSON.stringify(parsed, null, 2)}\n`)
}

async function updateTextFile(relativePath, mutate) {
  const targetPath = path.join(rootDir, relativePath)
  const raw = await readFile(targetPath, 'utf8')
  await writeFileWithExistingLineEndings(targetPath, raw, mutate(raw))
}

function normalizeLineEndings(value, lineEnding) {
  // Git for Windows may check out CRLF while Linux/CI use LF; keep version
  // synchronization idempotent in either worktree convention.
  return value.replace(/\r\n?/g, '\n').replaceAll('\n', lineEnding)
}

async function writeFileWithExistingLineEndings(targetPath, raw, value) {
  const lineEnding = raw.includes('\r\n') ? '\r\n' : '\n'
  await writeFile(targetPath, normalizeLineEndings(value, lineEnding), 'utf8')
}

function updateCargoPackageVersion(raw) {
  const packageStart = raw.indexOf('[package]')
  if (packageStart < 0) {
    throw new Error('apps/tauri/src-tauri/Cargo.toml is missing [package].')
  }
  const nextSection = raw.indexOf('\n[', packageStart + '[package]'.length)
  const packageEnd = nextSection < 0 ? raw.length : nextSection
  const packageBlock = raw.slice(packageStart, packageEnd)
  if (!/^version\s*=\s*"[^"]+"\s*$/m.test(packageBlock)) {
    throw new Error('apps/tauri/src-tauri/Cargo.toml is missing package.version.')
  }
  const updatedBlock = packageBlock.replace(/^version\s*=\s*"[^"]+"\s*$/m, `version = "${nextVersion}"`)
  return `${raw.slice(0, packageStart)}${updatedBlock}${raw.slice(packageEnd)}`
}

function updateCargoLockVersion(raw) {
  const packagePattern = /(\[\[package\]\]\r?\nname = "fileterm"\r?\nversion = ")[^"]+("\r?\n)/
  if (!packagePattern.test(raw)) {
    throw new Error('apps/tauri/src-tauri/Cargo.lock is missing the fileterm package entry.')
  }
  return raw.replace(packagePattern, `$1${nextVersion}$2`)
}

function updateTauriConfigVersion(raw) {
  const config = JSON.parse(raw)
  if (typeof config.version !== 'string') {
    throw new Error('apps/tauri/src-tauri/tauri.conf.json is missing version.')
  }
  return raw.replace(/("version"\s*:\s*")[^"]+("\s*,)/, `$1${nextVersion}$2`)
}

function updateLinuxMetainfoVersion(raw) {
  // 只在版本号真正变化时才更新 date，避免每次 sync:version 都把 date 改成当天。
  // AppStream 规范要求 date 反映该版本的发布日期，不是同步日期。
  const releaseWithDatePattern = /<release\s+version="([^"]*)"\s+date="([^"]*)">/
  const match = releaseWithDatePattern.exec(raw)
  if (match) {
    const [, currentVersion] = match
    if (currentVersion === nextVersion) {
      // 版本号未变，保留原 date
      return raw
    }
    // 版本号变化，更新 version 和 date
    const today = new Date().toISOString().slice(0, 10)
    return raw.replace(releaseWithDatePattern, `<release version="${nextVersion}" date="${today}">`)
  }
  // 没有 date 属性的旧格式，补上 version 和 date
  const today = new Date().toISOString().slice(0, 10)
  if (/<release\s+version="[^"]*"/.test(raw)) {
    return raw.replace(/<release\s+version="[^"]*"/, `<release version="${nextVersion}" date="${today}"`)
  }
  // 完全没有 release 标签，原样返回（不应发生）
  return raw
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
        packagePath === 'apps/tauri' ||
        packagePath === 'packages/core' ||
        packagePath === 'packages/shared' ||
        packagePath === 'packages/storage'
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

await updateTextFile('apps/tauri/src-tauri/tauri.conf.json', updateTauriConfigVersion)
await updateTextFile('apps/tauri/src-tauri/Cargo.toml', updateCargoPackageVersion)
await updateTextFile('apps/tauri/src-tauri/Cargo.lock', updateCargoLockVersion)
await updateTextFile('apps/tauri/src-tauri/linux/com.fileterm.desktop.metainfo.xml', updateLinuxMetainfoVersion)

console.log(`Synced workspace and Tauri bundle versions from root: ${nextVersion}`)
