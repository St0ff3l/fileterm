import fs from 'node:fs/promises'
import path from 'node:path'

const arguments_ = process.argv.slice(2)
const mergeManifests = arguments_.includes('--merge')
const bundleDirectories = arguments_.filter((argument) => argument !== '--merge')
if (bundleDirectories.length === 0 && mergeManifests) {
  throw new Error('At least one Windows artifact directory is required when merging updater manifests.')
}
if (bundleDirectories.length === 0) {
  bundleDirectories.push('src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis')
}
const repository = process.env.GITHUB_REPOSITORY
const tag = process.env.GITHUB_REF_NAME

if (!repository || !tag) {
  throw new Error('GITHUB_REPOSITORY and GITHUB_REF_NAME are required to create latest.json.')
}

const version = tag.replace(/^v/, '')
const platforms = {}
for (const directory of bundleDirectories) {
  const bundleDirectory = path.resolve(directory)
  const artifacts = await fs.readdir(bundleDirectory)
  const installers = artifacts.filter((artifact) => artifact.endsWith('-setup.exe'))
  if (installers.length !== 1) {
    throw new Error(`Expected exactly one NSIS installer in ${bundleDirectory}, found ${installers.length}.`)
  }

  const installer = installers[0]
  const architecture =
    installer.includes('windows-arm64-') || bundleDirectory.includes('aarch64-pc-windows-msvc') ? 'arm64' : 'x64'
  const platform = architecture === 'arm64' ? 'windows-aarch64' : 'windows-x86_64'
  const signaturePath = path.join(bundleDirectory, `${installer}.sig`)
  const signature = (await fs.readFile(signaturePath, 'utf8')).trim()
  if (!signature) {
    throw new Error(`Updater signature is empty: ${signaturePath}`)
  }

  const portableExecutables = artifacts.filter((artifact) => artifact.endsWith('-portable.exe'))
  if (portableExecutables.length !== 1) {
    throw new Error(
      `Expected exactly one portable executable in ${bundleDirectory}, found ${portableExecutables.length}.`
    )
  }
  const portable = portableExecutables[0]
  const portableSignaturePath = path.join(bundleDirectory, `${portable}.sig`)
  const portableSignature = (await fs.readFile(portableSignaturePath, 'utf8')).trim()
  if (!portableSignature) {
    throw new Error(`Portable updater signature is empty: ${portableSignaturePath}`)
  }

  const assetUrl = (artifact) =>
    new URL(
      `/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(artifact)}`,
      'https://github.com'
    ).toString()

  platforms[platform] = { signature, url: assetUrl(installer) }
  platforms[`${platform}-portable`] = { signature: portableSignature, url: assetUrl(portable) }
}

const manifest = {
  version,
  notes: process.env.FILETERM_UPDATE_NOTES ?? `FileTerm ${version}`,
  pub_date: new Date().toISOString(),
  platforms
}

const outputPath = mergeManifests
  ? path.resolve('latest.json')
  : path.join(path.resolve(bundleDirectories[0]), 'latest.json')
await fs.writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`)
console.log(`[FileTerm] created signed Windows updater manifest${mergeManifests ? 's' : ''}: ${outputPath}`)
