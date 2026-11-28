export type WebLoginStatus = { mode: 'login' | 'deployment' } | { mode: 'setup' | 'managed'; username: string }

let webLoginStatus: WebLoginStatus | null = null

export function getWebLoginStatus(): WebLoginStatus | null {
  return webLoginStatus
}

export function setWebLoginStatus(status: WebLoginStatus | null): void {
  webLoginStatus = status
}

export async function saveWebLogin(
  mode: 'login' | 'setup' | 'managed',
  credentials: { username: string; password: string; currentPassword?: string }
): Promise<number> {
  const endpoint = mode === 'login' ? '/api/web/login' : '/api/web/credentials'
  const response = await fetch(endpoint, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(credentials)
  })
  return response.status
}
