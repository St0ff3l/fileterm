import { useEffect, useMemo, useState } from 'react'
import type { PermissionChangeOptions } from '@termdock/core'
import { AppIcon } from '../common/AppIcon'
import { t } from '../../i18n'

type PermissionMatrix = {
  owner: { read: boolean; write: boolean; execute: boolean }
  group: { read: boolean; write: boolean; execute: boolean }
  other: { read: boolean; write: boolean; execute: boolean }
}

export function FilePermissionModal({
  errorMessage,
  fileName,
  initialPermission,
  onClose,
  onSubmit,
  supportsRecursive
}: {
  errorMessage: string | null
  fileName: string
  initialPermission?: string
  onClose(): void
  onSubmit(options: PermissionChangeOptions): void
  supportsRecursive: boolean
}) {
  const initialMatrix = useMemo(() => parsePermission(initialPermission), [initialPermission])
  const [matrix, setMatrix] = useState<PermissionMatrix>(initialMatrix)
  const [recursive, setRecursive] = useState(false)
  const [applyTo, setApplyTo] = useState<PermissionChangeOptions['applyTo']>('all')

  useEffect(() => {
    setMatrix(initialMatrix)
    setRecursive(false)
    setApplyTo('all')
  }, [initialMatrix, fileName])

  return (
    <div className="modal-backdrop">
      <div className="modal-card file-permission-modal">
        <div className="modal-header">
          <span>{t.permissionDialogTitle}</span>
          <button className="icon-button" onClick={onClose} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>

        <div className="file-permission-name">{fileName}</div>

        <PermissionRow
          label={t.permissionOwner}
          value={matrix.owner}
          onChange={(nextValue) => setMatrix((prev) => ({ ...prev, owner: nextValue }))}
        />
        <PermissionRow
          label={t.permissionGroup}
          value={matrix.group}
          onChange={(nextValue) => setMatrix((prev) => ({ ...prev, group: nextValue }))}
        />
        <PermissionRow
          label={t.permissionOther}
          value={matrix.other}
          onChange={(nextValue) => setMatrix((prev) => ({ ...prev, other: nextValue }))}
        />

        {supportsRecursive ? (
          <div className="permission-recursive-box">
            <label className="permission-checkline">
              <input checked={recursive} type="checkbox" onChange={(event) => setRecursive(event.target.checked)} />
              <span>{t.permissionRecursive}</span>
            </label>
            <label className="permission-radioline">
              <input checked={applyTo === 'all'} disabled={!recursive} type="radio" onChange={() => setApplyTo('all')} />
              <span>{t.permissionApplyAll}</span>
            </label>
            <label className="permission-radioline">
              <input checked={applyTo === 'files'} disabled={!recursive} type="radio" onChange={() => setApplyTo('files')} />
              <span>{t.permissionApplyFiles}</span>
            </label>
            <label className="permission-radioline">
              <input checked={applyTo === 'directories'} disabled={!recursive} type="radio" onChange={() => setApplyTo('directories')} />
              <span>{t.permissionApplyDirectories}</span>
            </label>
          </div>
        ) : null}

        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

        <div className="form-actions">
          <button className="flat-button" onClick={onClose} type="button">{t.cancel}</button>
          <button
            className="primary-button"
            onClick={() => {
              onSubmit({
                mode: matrixToMode(matrix),
                recursive,
                applyTo
              })
            }}
            type="button"
          >
            {t.confirm}
          </button>
        </div>
      </div>
    </div>
  )
}

function PermissionRow({
  label,
  onChange,
  value
}: {
  label: string
  onChange(value: PermissionMatrix['owner']): void
  value: PermissionMatrix['owner']
}) {
  return (
    <div className="permission-row">
      <div className="permission-row-label">{label}</div>
      <div className="permission-row-box">
        <label className="permission-checkline">
          <input checked={value.read} type="checkbox" onChange={(event) => onChange({ ...value, read: event.target.checked })} />
          <span>{t.permissionRead}</span>
        </label>
        <label className="permission-checkline">
          <input checked={value.write} type="checkbox" onChange={(event) => onChange({ ...value, write: event.target.checked })} />
          <span>{t.permissionWrite}</span>
        </label>
        <label className="permission-checkline">
          <input checked={value.execute} type="checkbox" onChange={(event) => onChange({ ...value, execute: event.target.checked })} />
          <span>{t.permissionExecute}</span>
        </label>
      </div>
    </div>
  )
}

function parsePermission(permission?: string): PermissionMatrix {
  const normalized = permission?.replace(/^[d-]/, '') || 'rwxr-xr-x'
  const groups = [normalized.slice(0, 3), normalized.slice(3, 6), normalized.slice(6, 9)]
  const [owner, group, other] = groups.map((value) => ({
    read: value[0] === 'r',
    write: value[1] === 'w',
    execute: value[2] === 'x'
  }))
  return { owner, group, other }
}

function matrixToMode(matrix: PermissionMatrix) {
  const rows = [matrix.owner, matrix.group, matrix.other]
  return rows.map((row) => {
    let value = 0
    if (row.read) value += 4
    if (row.write) value += 2
    if (row.execute) value += 1
    return String(value)
  }).join('')
}
