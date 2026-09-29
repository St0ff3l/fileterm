/** Bound UI waiting without cancelling a command already accepted by Rust. */
export async function waitForMonitoringRequest<T>(request: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      request,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error('Monitoring request timed out')), 20000)
      })
    ])
  } finally {
    clearTimeout(timer)
  }
}
