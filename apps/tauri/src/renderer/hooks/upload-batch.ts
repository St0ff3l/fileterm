/** Try every selected path even when an earlier task cannot be created. */
export async function createUploadBatch<T>(paths: string[], upload: (path: string) => Promise<T>) {
  let latestSnapshot: T | null = null
  const failures: { path: string; error: unknown }[] = []
  for (const path of new Set(paths)) {
    try {
      latestSnapshot = await upload(path)
    } catch (error) {
      failures.push({ path, error })
    }
  }
  return { latestSnapshot, failures }
}
