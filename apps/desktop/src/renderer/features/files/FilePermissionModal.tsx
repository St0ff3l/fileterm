import { useEffect, useMemo, useState } from 'react'
import type { PermissionChangeOptions } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { t } from '../../i18n'

type PermissionState = { read: boolean; write: boolean; execute: boolean }

type PermissionMatrix = {
  owner: PermissionState
  group: PermissionState
  other: PermissionState
}

export function FilePermissionModal({
  errorMessage,
  fileName,
  fileType,
  initialPermission,
  onClose,
  onSubmit,
  ownerGroup,
  supportsRecursive,
  targetPath
}: {
  errorMessage: string | null
  fileName: string
  fileType: 'file' | 'folder'
  initialPermission?: string
  onClose(): void
  onSubmit(options: PermissionChangeOptions): void
  ownerGroup?: string
  supportsRecursive: boolean
  targetPath: string
}) {
  const initialMatrix = useMemo(() => parsePermission(initialPermission), [initialPermission])
  const [matrix, setMatrix] = useState<PermissionMatrix>(initialMatrix)
  const [modeValue, setModeValue] = useState(() => matrixToMode(initialMatrix))
  const [recursive, setRecursive] = useState(false)
  const [applyTo, setApplyTo] = useState<PermissionChangeOptions['applyTo']>('all')
  const fileDirectory = useMemo(() => getParentPath(targetPath, fileName), [fileName, targetPath])
  const isModeValid = isPermissionMode(modeValue)
  const effectiveError = isModeValid ? errorMessage : t.permissionModeInvalid

  useEffect(() => {
    setMatrix(initialMatrix)
    setModeValue(matrixToMode(initialMatrix))
    setRecursive(false)
    setApplyTo('all')
  }, [initialMatrix, fileName])

  return (
    <div className="modal-backdrop">
      <div className="modal-card file-permission-dialog">
        <div className="file-permission-dialog__header">
          <span className="file-permission-dialog__eyebrow">{t.permissionDialogTitle}</span>
          <button className="icon-button file-permission-dialog__close" onClick={onClose} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>

        <div className="file-permission-dialog__scroll">
          <div className="file-permission-dialog__hero">
            <div className="file-permission-dialog__hero-icon" aria-hidden="true">
              <AppIcon name={fileType === 'folder' ? 'folder' : 'file'} size={24} />
            </div>
            <div className="file-permission-dialog__hero-copy">
              <div className="file-permission-dialog__file-name">{fileName}</div>
              <div className="file-permission-dialog__file-path">{fileDirectory}</div>
            </div>
          </div>

          <section className="file-permission-dialog__section">
            <div className="file-permission-dialog__section-title">{t.permissionMatrixTitle}</div>
            <div className="file-permission-dialog__matrix-card">
              <div className="file-permission-dialog__matrix-head">
                <span />
                <span>{t.permissionRead}</span>
                <span>{t.permissionWrite}</span>
                <span>{t.permissionExecute}</span>
              </div>
              <PermissionRow
                label={t.permissionOwner}
                value={matrix.owner}
                onChange={(nextValue) => {
                  const nextMatrix = { ...matrix, owner: nextValue }
                  setMatrix(nextMatrix)
                  setModeValue(mergeMatrixIntoMode(modeValue, nextMatrix))
                }}
              />
              <PermissionRow
                label={t.permissionGroup}
                value={matrix.group}
                onChange={(nextValue) => {
                  const nextMatrix = { ...matrix, group: nextValue }
                  setMatrix(nextMatrix)
                  setModeValue(mergeMatrixIntoMode(modeValue, nextMatrix))
                }}
              />
              <PermissionRow
                label={t.permissionOther}
                value={matrix.other}
                onChange={(nextValue) => {
                  const nextMatrix = { ...matrix, other: nextValue }
                  setMatrix(nextMatrix)
                  setModeValue(mergeMatrixIntoMode(modeValue, nextMatrix))
                }}
              />
            </div>
          </section>

          <section className="file-permission-dialog__section">
            <div className="file-permission-dialog__section-title">{t.permissionAdvancedTitle}</div>
            <div className="file-permission-dialog__advanced-grid">
              <div className="file-permission-dialog__info-card">
                <div className="file-permission-dialog__info-copy">
                  <span className="file-permission-dialog__info-title">{t.permissionOctalTitle}</span>
                  <span className="file-permission-dialog__info-subtitle">{t.permissionOctalSubtitle}</span>
                  <div className="file-permission-dialog__mode-inline">
                    <span>{t.permissionModeLabel}</span>
                    <strong>{modeValue.trim() || '---'}</strong>
                  </div>
                  <span className="file-permission-dialog__info-hint">{t.permissionOctalHint}</span>
                </div>
                <label className="file-permission-dialog__mode-input">
                  <input
                    aria-label={t.permissionMode}
                    inputMode="numeric"
                    maxLength={4}
                    onChange={(event) => {
                      const nextValue = event.target.value.replace(/[^\d]/g, '').slice(0, 4)
                      setModeValue(nextValue)
                      if (isPermissionMode(nextValue)) {
                        setMatrix(modeToMatrix(nextValue))
                      }
                    }}
                    type="text"
                    value={modeValue}
                  />
                </label>
              </div>

              <div className="file-permission-dialog__info-card">
                <div className="file-permission-dialog__info-copy">
                  <span className="file-permission-dialog__info-title">{t.ownerGroup}</span>
                  <span className="file-permission-dialog__info-value">{ownerGroup || '-'}</span>
                </div>
              </div>
            </div>

            {supportsRecursive ? (
              <div className="file-permission-dialog__recursive-card">
                <div className="file-permission-dialog__recursive-copy">
                  <span className="file-permission-dialog__info-title">{t.permissionRecursiveTitle}</span>
                  <span className="file-permission-dialog__info-subtitle">
                    {recursive ? t.permissionRecursiveEnabledHint : t.permissionRecursiveDisabledHint}
                  </span>
                </div>
                <label className="file-permission-dialog__switch">
                  <input checked={recursive} type="checkbox" onChange={(event) => setRecursive(event.target.checked)} />
                  <span className="file-permission-dialog__switch-track" />
                </label>
              </div>
            ) : null}

            {supportsRecursive && recursive ? (
              <div className="file-permission-dialog__apply-grid">
                <label className="file-permission-dialog__apply-option">
                  <input checked={applyTo === 'all'} type="radio" onChange={() => setApplyTo('all')} />
                  <span>{t.permissionApplyAll}</span>
                </label>
                <label className="file-permission-dialog__apply-option">
                  <input checked={applyTo === 'files'} type="radio" onChange={() => setApplyTo('files')} />
                  <span>{t.permissionApplyFiles}</span>
                </label>
                <label className="file-permission-dialog__apply-option">
                  <input checked={applyTo === 'directories'} type="radio" onChange={() => setApplyTo('directories')} />
                  <span>{t.permissionApplyDirectories}</span>
                </label>
              </div>
            ) : null}
          </section>

          {effectiveError ? <div className="modal-error">{effectiveError}</div> : null}
        </div>

        <div className="form-actions file-permission-dialog__actions">
          <button className="flat-button compact" onClick={onClose} type="button">{t.cancel}</button>
          <button
            className="primary-button compact"
            disabled={!isModeValid}
            onClick={() => {
              if (!isModeValid) {
                return
              }

              onSubmit({
                mode: modeValue.trim(),
                recursive,
                applyTo
              })
            }}
            type="button"
          >
            {t.permissionApplyChanges}
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
  onChange(value: PermissionState): void
  value: PermissionState
}) {
  return (
    <div className="file-permission-dialog__matrix-row">
      <div className="file-permission-dialog__matrix-label">{label}</div>
      <PermissionCell checked={value.read} label={`${label} ${t.permissionRead}`} onChange={(checked) => onChange({ ...value, read: checked })} />
      <PermissionCell checked={value.write} label={`${label} ${t.permissionWrite}`} onChange={(checked) => onChange({ ...value, write: checked })} />
      <PermissionCell checked={value.execute} label={`${label} ${t.permissionExecute}`} onChange={(checked) => onChange({ ...value, execute: checked })} />
    </div>
  )
}

function PermissionCell({
  checked,
  label,
  onChange
}: {
  checked: boolean
  label: string
  onChange(checked: boolean): void
}) {
  return (
    <label className="file-permission-dialog__matrix-cell">
      <input aria-label={label} checked={checked} type="checkbox" onChange={(event) => onChange(event.target.checked)} />
      <span />
    </label>
  )
}

function getParentPath(targetPath: string, fileName: string) {
  if (!targetPath) {
    return fileName
  }

  const normalized = targetPath.replace(/\\/g, '/')
  const suffix = `/${fileName}`
  if (normalized.endsWith(suffix)) {
    return normalized.slice(0, -suffix.length) || '/'
  }

  const lastSlashIndex = normalized.lastIndexOf('/')
  if (lastSlashIndex >= 0) {
    return normalized.slice(0, lastSlashIndex) || '/'
  }

  const lastBackslashIndex = targetPath.lastIndexOf('\\')
  if (lastBackslashIndex >= 0) {
    return targetPath.slice(0, lastBackslashIndex) || '\\'
  }

  return targetPath
}

function isPermissionMode(mode: string) {
  return /^[0-7]{3,4}$/.test(mode.trim())
}

function modeToMatrix(mode: string): PermissionMatrix {
  const digits = mode.trim().slice(-3)
  const [owner, group, other] = digits.split('').map((digit) => digitToPermission(Number(digit)))
  return { owner, group, other }
}

function mergeMatrixIntoMode(currentMode: string, matrix: PermissionMatrix) {
  const nextDigits = matrixToMode(matrix)
  const trimmedMode = currentMode.trim()
  if (/^[0-7]{4}$/.test(trimmedMode)) {
    return `${trimmedMode[0]}${nextDigits}`
  }
  return nextDigits
}

function digitToPermission(value: number): PermissionState {
  return {
    read: Boolean(value & 4),
    write: Boolean(value & 2),
    execute: Boolean(value & 1)
  }
}

function parsePermission(permission?: string): PermissionMatrix {
  if (permission && /^[0-7]{3,4}$/.test(permission.trim())) {
    return modeToMatrix(permission)
  }

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
