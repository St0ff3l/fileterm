import { useEffect, useRef, useState, type FormEvent } from 'react'
import { createPortal } from 'react-dom'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../../components/common/confirm-action-dialog/confirm-action-dialog'
import { Dialog } from '../../components/common/dialog/dialog'
import { DropdownSelect } from '../../components/common/dropdown-select/dropdown-select'
import { Input } from '../../components/common/input/input'
import { t } from '../../i18n'
import {
  decodeServerSavedViews,
  SERVER_SAVED_VIEWS_KEY,
  type ServerSavedView,
  type ServerViewFilters
} from './server-saved-views-model'

export function ServerSavedViews({
  filters,
  onApply
}: {
  filters: ServerViewFilters
  onApply(filters: ServerViewFilters): void
}) {
  const api = window.fileterm
  const [views, setViews] = useState<ServerSavedView[] | null>(null)
  const [selectedId, setSelectedId] = useState('')
  const [name, setName] = useState('')
  const [dialog, setDialog] = useState<'save' | 'remove' | null>(null)
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)
  const inFlight = useRef(false)
  const active = views?.find(
    (view) =>
      view.id === selectedId &&
      view.tag === filters.tag &&
      view.status === filters.status &&
      view.query === filters.query
  )

  useEffect(() => {
    if (!api) return
    let mounted = true
    void api
      .getUiStateItem(SERVER_SAVED_VIEWS_KEY)
      .then(decodeServerSavedViews)
      .then(
        (entries) => {
          if (mounted) setViews(entries)
        },
        () => {
          if (mounted) setError(t.serverSavedViewsLoadError)
        }
      )
    return () => {
      mounted = false
    }
  }, [api])

  const close = () => {
    if (inFlight.current) return
    setError('')
    setDialog(null)
  }

  const persist = async (next: ServerSavedView[], id: string) => {
    if (!api || inFlight.current) return
    inFlight.current = true
    setSaving(true)
    setError('')
    try {
      await api.setUiStateItem(SERVER_SAVED_VIEWS_KEY, JSON.stringify(next))
      setViews(next)
      setSelectedId(id)
      setDialog(null)
    } catch {
      setError(t.serverSavedViewsWriteError)
    } finally {
      inFlight.current = false
      setSaving(false)
    }
  }

  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const trimmed = name.trim()
    if (!views || !trimmed || inFlight.current) return
    if (views.some((view) => view.name.toLocaleLowerCase() === trimmed.toLocaleLowerCase())) {
      setError(t.serverSavedViewsDuplicate)
      return
    }
    const view = { id: crypto.randomUUID(), name: trimmed, ...filters }
    void persist([...views, view], view.id)
  }

  if (!api) return null

  return (
    <div className="server-dashboard-saved-views">
      <DropdownSelect
        ariaLabel={t.serverSavedViews}
        className="server-dashboard-saved-views-select"
        disabled={!views?.length || saving}
        menuPlacement="auto"
        onChange={(id) => {
          const view = views?.find((view) => view.id === id)
          if (!view) return
          onApply(view)
          setSelectedId(id)
        }}
        options={[
          { value: '', label: t.serverSavedViews, disabled: true },
          ...(views ?? []).map(({ id, name }) => ({ value: id, label: name }))
        ]}
        value={active?.id ?? ''}
      />
      <Button
        disabled={!views || saving}
        onClick={() => {
          setName('')
          setDialog('save')
        }}
        size="panel"
      >
        {t.serverSavedViewsSave}
      </Button>
      {active && (
        <Button disabled={saving} onClick={() => setDialog('remove')} size="panel" variant="ghost">
          {t.serverSavedViewsRemove}
        </Button>
      )}
      {!dialog && error && <span role="alert">{error}</span>}
      {dialog === 'save' &&
        createPortal(
          <Dialog
            aria-label={t.serverSavedViewsSave}
            footer={
              <Button
                disabled={!name.trim()}
                form="server-saved-view-form"
                loading={saving}
                size="panel"
                type="submit"
                variant="primary"
              >
                {t.save}
              </Button>
            }
            isOpen
            onClose={close}
            size="sm"
            title={t.serverSavedViewsSave}
          >
            <form id="server-saved-view-form" onSubmit={save}>
              <p className="server-saved-view-hint" id="server-saved-view-hint">
                {t.serverSavedViewsHint}
              </p>
              <Input
                aria-describedby="server-saved-view-hint"
                aria-label={t.serverSavedViewsName}
                autoFocus
                disabled={saving}
                maxLength={64}
                onChange={(event) => {
                  setName(event.target.value)
                  setError('')
                }}
                placeholder={t.serverSavedViewsName}
                value={name}
              />
            </form>
            {error && <p role="alert">{error}</p>}
          </Dialog>,
          document.body
        )}
      {dialog === 'remove' && active && views && (
        <ConfirmActionDialog
          confirmLabel={t.serverSavedViewsRemove}
          description={active.name}
          errorMessage={error}
          isSubmitting={saving}
          onClose={close}
          onConfirm={() =>
            void persist(
              views.filter(({ id }) => id !== active.id),
              ''
            )
          }
          title={t.serverSavedViewsRemove}
        />
      )}
    </div>
  )
}
