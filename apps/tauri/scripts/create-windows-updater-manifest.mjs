import fs from 'node:fs/promises'
import path from 'node:path'

const bundleDirectory = path.resolve(process.argv[2] ?? 'src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis')
const repository = process.env.GITHUB_REPOSITORY
const tag = process.env.GITHUB_REF_NAME

if (!repository || !tag) {
  throw new Error('GITHUB_REPOSITORY and GITHUB_REF_NAME are required to create latest.json.')
}

const artifacts = await fs.readdir(bundleDirectory)
const installers = artifacts.filter((artifact) => artifact.endsWith('-setup.exe'))
if (installers.length !== 1) {
  throw new Error(`Expected exactly one NSIS installer in ${bundleDirectory}, found ${installers.length}.`)
}

const installer = installers[0]
const signaturePath = path.join(bundleDirectory, `${installer}.sig`)
const signature = (await fs.readFile(signaturePath, 'utf8')).trim()
if (!signature) {
  throw new Error(`Updater signature is empty: ${signaturePath}`)
}

const portableExecutables = artifacts.filter((artifact) => artifact.endsWith('-portable.exe'))
if (portableExecutables.length !== 1) {
  throw new Error(`Expected exactly one portable executable, found ${portableExecutables.length}.`)
}
const portable = portableExecutables[0]
const portableSignature = (await fs.readFile(path.join(bundleDirectory, `${portable}.sig`), 'utf8')).trim()
if (!portableSignature) {
  throw new Error('Portable updater signature is empty.')
}
const portableUrl = new URL(
  `/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(portable)}`,
  'https://github.com'
)

const version = tag.replace(/^v/, '')
const downloadUrl = new URL(
  `/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(installer)}`,
  'https://github.com'
)

const manifest = {
  version,
  notes: process.env.FILETERM_UPDATE_NOTES ?? `FileTerm ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature,
      url: downloadUrl.toString()
    },
    'windows-x86_64-portable': {
      signature: portableSignature,
      url: portableUrl.toString()
    }
  }
}

const outputPath = path.join(bundleDirectory, 'latest.json')
await fs.writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`)
console.log(`[FileTerm] created signed Windows updater manifest: ${outputPath}`)
