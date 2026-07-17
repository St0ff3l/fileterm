import path from 'node:path'

export function relativeRemoteTransferPath(rootPath: string, candidatePath: string): string {
  const normalizedRoot = path.posix.resolve('/', rootPath)
  const normalizedCandidate = path.posix.resolve('/', candidatePath)
  const relativePath = path.posix.relative(normalizedRoot, normalizedCandidate)

  if (!relativePath || relativePath === '..' || relativePath.startsWith('../') || path.posix.isAbsolute(relativePath)) {
    throw new Error(`远端路径不在下载目录内：${candidatePath}`)
  }

  return relativePath
}
