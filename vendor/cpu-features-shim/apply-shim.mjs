import fs from 'node:fs'
import path from 'node:path'

const packageDir = path.resolve(process.cwd(), 'node_modules/cpu-features')

if (!fs.existsSync(packageDir)) {
  console.log('[cpu-features-shim] skip: cpu-features is not installed')
  process.exit(0)
}

const packageJsonPath = path.join(packageDir, 'package.json')

let packageJson = {
  name: 'cpu-features',
  version: '0.0.10',
  main: 'index.js'
}

if (fs.existsSync(packageJsonPath)) {
  packageJson = {
    ...JSON.parse(fs.readFileSync(packageJsonPath, 'utf8')),
    main: 'index.js',
    scripts: {},
    gypfile: false
  }
}

const shimSource = `'use strict';

module.exports = function getCpuFeatures() {
  return {
    arch: process.arch === 'ia32' ? 'x86' : process.arch,
    flags: {}
  };
};
`

for (const entry of ['binding.gyp', 'src', 'build', 'deps']) {
  const target = path.join(packageDir, entry)
  if (fs.existsSync(target)) {
    fs.rmSync(target, { recursive: true, force: true })
  }
}

fs.writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`)
fs.writeFileSync(path.join(packageDir, 'index.js'), shimSource)

console.log('[cpu-features-shim] applied shim at', packageDir)
