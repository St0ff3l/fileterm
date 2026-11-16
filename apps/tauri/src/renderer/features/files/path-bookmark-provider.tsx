import { createContext, use, useState, type ReactNode } from 'react'
import type { PathBookmark, RemoteFileItem } from '@fileterm/core'
import { t } from '../../i18n'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'

const BookmarkContext = createContext<(pane: 'local' | 'remote', item: RemoteFileItem) => void>(() => {})
const DefaultDirectoryContext = createContext<(pane: 'local' | 'remote', path: string) => Promise<void>>(
  async () => undefined
)
export const useAddPathBookmark = () => use(BookmarkContext)
export const useSetDefaultDirectory = () => use(DefaultDirectoryContext)

export function toPathBookmark(scope: string, item: Pick<RemoteFileItem, 'path' | 'name' | 'type'>): PathBookmark {
  return { id: '', scope, path: item.path, name: item.name, type: item.type }
}

export function PathBookmarkProvider({ profileId, children }: { profileId: string; children: ReactNode }) {
  const [error, setError] = useState<string | null>(null)
  const add = (pane: 'local' | 'remote', item: RemoteFileItem) => {
    const scope = pane === 'local' ? 'local' : `remote:${profileId}`
    if (!window.fileterm) return
    void window.fileterm.updatePathBookmark(scope, 'add', toPathBookmark(scope, item)).catch((cause: unknown) => {
      setError(String(cause))
    })
  }
  const setDefaultDirectory = (pane: 'local' | 'remote', path: string) => {
    if (!window.fileterm) return Promise.reject(new Error('Desktop API is unavailable'))
    return window.fileterm.setProfileDefaultDirectory(profileId, pane, path).then(() => undefined)
  }
  return (
    <BookmarkContext value={add}>
      <DefaultDirectoryContext value={setDefaultDirectory}>
        {children}
        {error !== null && (
          <ConfirmActionDialog
            title={t.pathBookmarks}
            description={error}
            confirmLabel={t.confirm}
            confirmVariant="primary"
            onClose={() => setError(null)}
            onConfirm={() => setError(null)}
          />
        )}
      </DefaultDirectoryContext>
    </BookmarkContext>
  )
}
