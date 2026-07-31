import { useEffect, useRef, useState } from 'react'
import type { AppUpdateStatus, S3BackupConfig, WebDavSyncConfig } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'

export function SettingsModal({
  theme,
  onSetTheme,
  locale,
  onSetLocale,
  onOpenCommandManager,
  onOpenConnectionManager,
  onOpenLogsDirectory,
  onClose,
  standalone = false,
  inline = false
}: {
  theme: 'default-dark' | 'default-light'
  onSetTheme(value: 'default-dark' | 'default-light'): void
  locale: 'zhCN' | 'enUS'
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenCommandManager(): void
  onOpenConnectionManager(): void
  onOpenLogsDirectory(): void
  onClose(): void
  standalone?: boolean
  inline?: boolean
}) {
  const [activeTab, setActiveTab] = useState<'general' | 'sync' | 'tools' | 'updates' | 'system'>('general')
  const [syncSubTab, setSyncSubTab] = useState<'webdav' | 's3'>('webdav')
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(true)
  const [isSavingUpdatePreference, setIsSavingUpdatePreference] = useState(false)
  const [updatePreferenceError, setUpdatePreferenceError] = useState<string | null>(null)
  const [syncConfig, setSyncConfig] = useState<WebDavSyncConfig | null>(null)
  const [syncPassword, setSyncPassword] = useState('')
  const [syncMessage, setSyncMessage] = useState<string | null>(null)
  const [s3Config, setS3Config] = useState<S3BackupConfig | null>(null)
  const [s3SecretAccessKey, setS3SecretAccessKey] = useState('')
  const [s3Message, setS3Message] = useState<string | null>(null)
  const [syncOperation, setSyncOperation] = useState<
    'load' | 'save' | 'test' | 'upload' | 'download' | 's3-save' | 's3-test' | 's3-upload' | 's3-download' | null
  >(null)
  const syncOperationRef = useRef<typeof syncOperation>(null)
  const desktopApi = window.fileterm
  const updatePreviewState = import.meta.env.DEV ? import.meta.env.VITE_UPDATE_PREVIEW : undefined

  useEffect(() => {
    if (updatePreviewState) {
      setUpdateStatus({
        currentVersion: desktopApi?.appVersion ?? '1.0.0',
        state:
          updatePreviewState === 'downloading' || updatePreviewState === 'downloaded' || updatePreviewState === 'error'
            ? updatePreviewState
            : 'available',
        availableVersion: '1.1.0',
        progress: updatePreviewState === 'downloading' ? 62 : updatePreviewState === 'downloaded' ? 100 : undefined,
        message: updatePreviewState === 'error' ? t.updateServerUnavailable : undefined
      })
      return
    }
    if (!desktopApi) {
      return
    }
    void desktopApi.getUpdateStatus().then(setUpdateStatus)
    return desktopApi.onUpdateStatus(setUpdateStatus)
  }, [desktopApi, updatePreviewState])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    let canceled = false
    void desktopApi
      .getUiPreferences()
      .then((preferences) => {
        if (!canceled) {
          setAutoCheckUpdates(preferences.autoCheckUpdates)
        }
      })
      .catch(() => {
        if (!canceled) {
          setUpdatePreferenceError(t.updatePreferenceLoadFailed)
        }
      })

    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) {
        setAutoCheckUpdates(preferences.autoCheckUpdates)
      }
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi])

  useEffect(() => {
    if (activeTab !== 'sync' || !desktopApi) return
    if (syncOperationRef.current) return
    syncOperationRef.current = 'load'
    setSyncOperation('load')
    void desktopApi
      .getWebDavSyncConfig()
      .then(async (webDavConfig) => {
        setSyncConfig(webDavConfig)
        setS3Config(await desktopApi.getS3BackupConfig())
      })
      .catch((error: unknown) => setSyncMessage(error instanceof Error ? error.message : String(error)))
      .finally(() => {
        if (syncOperationRef.current === 'load') {
          syncOperationRef.current = null
          setSyncOperation(null)
        }
      })
  }, [activeTab, desktopApi])

  const runSyncOperation = async (
    operation: Exclude<typeof syncOperation, 'load' | null>,
    action: () => Promise<void>
  ) => {
    if (syncOperationRef.current) return
    syncOperationRef.current = operation
    setSyncOperation(operation)
    if (operation.startsWith('s3-')) {
      setS3Message(null)
    } else {
      setSyncMessage(null)
    }
    try {
      await action()
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (operation.startsWith('s3-')) {
        setS3Message(message)
      } else {
        setSyncMessage(message)
      }
    } finally {
      if (syncOperationRef.current === operation) {
        syncOperationRef.current = null
        setSyncOperation(null)
      }
    }
  }

  const platformLabel = (() => {
    const platform = desktopApi?.platform ?? 'unknown'
    const arch = desktopApi?.arch ?? 'unknown'
    if (platform === 'darwin') {
      if (arch === 'arm64') return 'macOS (Apple Silicon)'
      if (arch === 'x64' || arch === 'x86_64') return 'macOS (Intel)'
      return `macOS (${arch})`
    }
    if (platform === 'win32') {
      return arch === 'arm64' ? 'Windows (ARM)' : `Windows (${arch})`
    }
    if (platform === 'linux') {
      return `Linux (${arch})`
    }
    return `${platform} / ${arch}`
  })()

  const managerToolsHint = inline ? t.settingsManagersInlineHint : t.settingsManagersWindowHint
  const managerToolsActionLabel = inline ? t.switchToManagerPage : t.openInSeparateWindow

  const setUpdateCheckPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingUpdatePreference || nextValue === autoCheckUpdates) {
      return
    }

    const previousValue = autoCheckUpdates
    setAutoCheckUpdates(nextValue)
    setUpdatePreferenceError(null)
    setIsSavingUpdatePreference(true)
    void desktopApi
      .setUiPreferences({ autoCheckUpdates: nextValue })
      .then((preferences) => setAutoCheckUpdates(preferences.autoCheckUpdates))
      .catch(() => {
        setAutoCheckUpdates(previousValue)
        setUpdatePreferenceError(t.updatePreferenceSaveFailed)
      })
      .finally(() => setIsSavingUpdatePreference(false))
  }

  const content = (
    <div
      className={`modal-card manager-modal connection-manager-modal settings-modal ${standalone ? 'standalone' : ''} ${inline ? 'manager-inline' : ''}`}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">settings</span>
          <span>{t.settings}</span>
        </span>
        {!inline && (
          <div className="connection-manager-header-actions">
            <CloseButton disabled={syncOperation !== null} onClick={onClose} />
          </div>
        )}
      </div>
      <div className="connection-manager-layout">
        <aside className="connection-manager-sidebar" aria-label={t.settings}>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'general' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('general')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">tune</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.generalSettings}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'sync' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('sync')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">cloud_sync</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.configSync}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'updates' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('updates')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">system_update</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.appUpdates}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'tools' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('tools')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">apps</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.managerToolsShortcut}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'system' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('system')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">info</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.systemLogsInfo}</span>
          </button>
        </aside>

        <main className="connection-manager-main">
          {activeTab === 'general' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.appearanceTheme}</h3>
                <div className="theme-options-grid">
                  <button
                    className={`theme-card dark ${theme === 'default-dark' ? 'active' : ''}`}
                    onClick={() => onSetTheme('default-dark')}
                    type="button"
                  >
                    <div className="theme-card-preview">
                      <div className="preview-header"></div>
                      <div className="preview-body">
                        <div className="preview-sidebar"></div>
                        <div className="preview-content"></div>
                      </div>
                    </div>
                    <span>
                      {t.theme}: {t.defaultDark}
                    </span>
                  </button>
                  <button
                    className={`theme-card light ${theme === 'default-light' ? 'active' : ''}`}
                    onClick={() => onSetTheme('default-light')}
                    type="button"
                  >
                    <div className="theme-card-preview">
                      <div className="preview-header"></div>
                      <div className="preview-body">
                        <div className="preview-sidebar"></div>
                        <div className="preview-content"></div>
                      </div>
                    </div>
                    <span>
                      {t.theme}: {t.defaultLight}
                    </span>
                  </button>
                </div>
              </section>

              <section className="settings-section">
                <h3>{t.languageSelection}</h3>
                <div className="language-selector-row">
                  <button
                    className={`lang-card ${locale === 'zhCN' ? 'active' : ''}`}
                    onClick={() => onSetLocale('zhCN')}
                    type="button"
                  >
                    {t.languageZhCN}
                  </button>
                  <button
                    className={`lang-card ${locale === 'enUS' ? 'active' : ''}`}
                    onClick={() => onSetLocale('enUS')}
                    type="button"
                  >
                    {t.languageEnglish}
                  </button>
                </div>
              </section>
            </div>
          ) : null}

          {activeTab === 'tools' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.managerToolsShortcut}</h3>
                <p className="settings-tools-hint">{managerToolsHint}</p>
                <div className="tools-shortcuts-grid">
                  <div className="tool-shortcut-card">
                    <span className="material-symbols-outlined tool-card-icon">settings_ethernet</span>
                    <div className="tool-card-details">
                      <strong>{t.connectionManager}</strong>
                      <p>{t.settingsConnectionManagerDescription}</p>
                      <button className="primary-button compact" onClick={onOpenConnectionManager} type="button">
                        {managerToolsActionLabel}
                      </button>
                    </div>
                  </div>
                  <div className="tool-shortcut-card">
                    <span className="material-symbols-outlined tool-card-icon">terminal</span>
                    <div className="tool-card-details">
                      <strong>{t.commandManager}</strong>
                      <p>{t.settingsCommandManagerDescription}</p>
                      <button className="primary-button compact" onClick={onOpenCommandManager} type="button">
                        {managerToolsActionLabel}
                      </button>
                    </div>
                  </div>
                </div>
              </section>
            </div>
          ) : null}

          {activeTab === 'sync' && syncConfig ? (
            <div className="settings-panel">
              <div className="sync-subtabs">
                <button
                  type="button"
                  className={`sync-subtab-button ${syncSubTab === 'webdav' ? 'active' : ''}`}
                  onClick={() => setSyncSubTab('webdav')}
                >
                  <span className="material-symbols-outlined">cloud_sync</span>
                  <span>WebDAV</span>
                </button>
                <button
                  type="button"
                  className={`sync-subtab-button ${syncSubTab === 's3' ? 'active' : ''}`}
                  onClick={() => setSyncSubTab('s3')}
                >
                  <span className="material-symbols-outlined">database</span>
                  <span>S3</span>
                </button>
              </div>

              {syncSubTab === 'webdav' && (
                <section className="settings-section">
                  <h3>{t.webdavConfigSync}</h3>
                  <p className="settings-tools-hint">{t.webdavConfigSyncDescription}</p>
                  <fieldset disabled={syncOperation !== null} style={{ border: 0, margin: 0, padding: 0 }}>
                    <div className="webdav-sync-form">
                      <label>
                        <span>{t.webdavUrl}</span>
                        <input
                          value={syncConfig.url}
                          placeholder="https://dav.example.com/remote.php/dav/files/me"
                          onChange={(event) => setSyncConfig({ ...syncConfig, url: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.webdavRemoteFile}</span>
                        <input
                          value={syncConfig.remotePath}
                          placeholder="fileterm-connections.json"
                          onChange={(event) => setSyncConfig({ ...syncConfig, remotePath: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.webdavUsername}</span>
                        <input
                          value={syncConfig.username ?? ''}
                          onChange={(event) => setSyncConfig({ ...syncConfig, username: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.webdavPassword}</span>
                        <input
                          type="password"
                          autoComplete="new-password"
                          value={syncPassword}
                          placeholder={t.webdavPasswordPlaceholder}
                          onChange={(event) => setSyncPassword(event.target.value)}
                        />
                      </label>
                      <div className="webdav-sync-options">
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            checked={syncConfig.enabled}
                            onChange={(event) => setSyncConfig({ ...syncConfig, enabled: event.target.checked })}
                          />
                          {t.enableWebdavSync}
                        </label>
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            checked={syncConfig.allowInsecureTls === true}
                            onChange={(event) =>
                              setSyncConfig({ ...syncConfig, allowInsecureTls: event.target.checked })
                            }
                          />
                          {t.allowInsecureHttp}
                        </label>
                      </div>
                    </div>
                    <div className="settings-update-actions webdav-sync-actions">
                      <button
                        className="primary-button compact"
                        disabled={syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('save', async () => {
                            const config = await desktopApi.saveWebDavSyncConfig({
                              ...syncConfig,
                              ...(syncPassword ? { password: syncPassword } : {})
                            })
                            setSyncConfig(config)
                            setSyncPassword('')
                            setSyncMessage(t.syncConfigSaved)
                          })
                        }}
                      >
                        {syncOperation === 'save' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.save}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('test', async () => {
                            const result = await desktopApi.testWebDavSync()
                            setSyncMessage(result.message)
                          })
                        }}
                      >
                        {syncOperation === 'test' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.webdavTestConnection}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={!syncConfig.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('upload', async () => {
                            const result = await desktopApi.uploadWebDavSync()
                            setSyncMessage(result.message)
                          })
                        }}
                      >
                        {syncOperation === 'upload' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.syncUpload}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={!syncConfig.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('download', async () => {
                            const result = await desktopApi.downloadWebDavSync()
                            setSyncMessage(result.message)
                          })
                        }}
                      >
                        {syncOperation === 'download' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.syncDownload}</span>
                      </button>
                    </div>
                  </fieldset>
                  {syncConfig.lastSyncedAt ? (
                    <p className="settings-tools-hint">
                      {t.lastSync.replace('{time}', new Date(syncConfig.lastSyncedAt).toLocaleString())}
                    </p>
                  ) : null}
                  {syncMessage ? <p className="settings-tools-hint">{syncMessage}</p> : null}
                </section>
              )}

              {syncSubTab === 's3' && s3Config && (
                <section className="settings-section">
                  <h3>{t.s3Backup}</h3>
                  <p className="settings-tools-hint">{t.s3BackupDescription}</p>
                  <fieldset disabled={syncOperation !== null} style={{ border: 0, margin: 0, padding: 0 }}>
                    <div className="webdav-sync-form">
                      <label>
                        <span>{t.s3Provider}</span>
                        <DropdownSelect
                          value={s3Config.provider}
                          options={[
                            { value: 'cloudflare-r2', label: t.s3ProviderCloudflareR2 },
                            { value: 'bitiful-s4', label: t.s3ProviderBitifulS4 },
                            { value: 'custom', label: t.s3ProviderCustom }
                          ]}
                          onChange={(provider) => {
                            const isR2 = provider === 'cloudflare-r2'
                            const isBitiful = provider === 'bitiful-s4'
                            setS3Config({
                              ...s3Config,
                              provider: isR2 ? 'cloudflare-r2' : isBitiful ? 'bitiful-s4' : 'custom',
                              endpoint: isBitiful ? 'https://s3.bitiful.net' : s3Config.endpoint,
                              region: isR2
                                ? 'auto'
                                : isBitiful
                                  ? 'cn-east-1'
                                  : s3Config.region === 'auto'
                                    ? 'us-east-1'
                                    : s3Config.region,
                              pathStyleAccessEnabled: isR2 ? true : isBitiful ? false : s3Config.pathStyleAccessEnabled
                            })
                          }}
                        />
                      </label>
                      <label>
                        <span>{t.s3Endpoint}</span>
                        <input
                          readOnly={s3Config.provider === 'bitiful-s4'}
                          value={s3Config.endpoint}
                          placeholder={
                            s3Config.provider === 'bitiful-s4'
                              ? 'https://s3.bitiful.net'
                              : 'https://<account-id>.r2.cloudflarestorage.com'
                          }
                          onChange={(event) => setS3Config({ ...s3Config, endpoint: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3Region}</span>
                        <input
                          disabled={s3Config.provider === 'cloudflare-r2' || s3Config.provider === 'bitiful-s4'}
                          value={s3Config.region}
                          placeholder="auto"
                          onChange={(event) => setS3Config({ ...s3Config, region: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3Bucket}</span>
                        <input
                          value={s3Config.bucket}
                          onChange={(event) => setS3Config({ ...s3Config, bucket: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3ObjectKey}</span>
                        <input
                          value={s3Config.remotePath}
                          placeholder="fileterm/connections.json"
                          onChange={(event) => setS3Config({ ...s3Config, remotePath: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3AccessKeyId}</span>
                        <input
                          autoComplete="off"
                          value={s3Config.accessKeyId ?? ''}
                          onChange={(event) => setS3Config({ ...s3Config, accessKeyId: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3SecretAccessKey}</span>
                        <input
                          type="password"
                          autoComplete="new-password"
                          value={s3SecretAccessKey}
                          placeholder={s3Config.hasSavedSecret ? t.s3SecretAccessKeyPlaceholder : undefined}
                          onChange={(event) => setS3SecretAccessKey(event.target.value)}
                        />
                      </label>
                      <div className="webdav-sync-options">
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            checked={s3Config.enabled}
                            onChange={(event) => setS3Config({ ...s3Config, enabled: event.target.checked })}
                          />
                          {t.enableS3Backup}
                        </label>
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            disabled={s3Config.provider === 'cloudflare-r2' || s3Config.provider === 'bitiful-s4'}
                            checked={s3Config.pathStyleAccessEnabled}
                            onChange={(event) =>
                              setS3Config({ ...s3Config, pathStyleAccessEnabled: event.target.checked })
                            }
                          />
                          {t.s3PathStyle}
                        </label>
                      </div>
                    </div>
                    <div className="settings-update-actions webdav-sync-actions">
                      <button
                        className="primary-button compact"
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('s3-save', async () => {
                            const config = await desktopApi.saveS3BackupConfig({
                              ...s3Config,
                              ...(s3SecretAccessKey ? { secretAccessKey: s3SecretAccessKey } : {})
                            })
                            setS3Config(config)
                            setS3SecretAccessKey('')
                            setS3Message(t.s3BackupSaved)
                          })
                        }}
                      >
                        {syncOperation === 's3-save' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.save}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('s3-test', async () => {
                            const result = await desktopApi.testS3Backup()
                            setS3Message(result.message)
                          })
                        }}
                      >
                        {syncOperation === 's3-test' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.s3TestConnection}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={!s3Config.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('s3-upload', async () => {
                            const result = await desktopApi.uploadS3Backup()
                            setS3Message(result.message)
                          })
                        }}
                      >
                        {syncOperation === 's3-upload' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.syncUpload}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={!s3Config.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('s3-download', async () => {
                            const result = await desktopApi.downloadS3Backup()
                            setS3Message(result.message)
                          })
                        }}
                      >
                        {syncOperation === 's3-download' ? (
                          <span aria-hidden="true" className="button-spinner" />
                        ) : null}
                        <span>{t.syncDownload}</span>
                      </button>
                    </div>
                  </fieldset>
                  {s3Config.lastSyncedAt ? (
                    <p className="settings-tools-hint">
                      {t.lastSync.replace('{time}', new Date(s3Config.lastSyncedAt).toLocaleString())}
                    </p>
                  ) : null}
                  {s3Message ? <p className="settings-tools-hint">{s3Message}</p> : null}
                </section>
              )}
            </div>
          ) : null}

          {activeTab === 'sync' && !syncConfig ? (
            <div className="settings-panel">
              <section aria-busy="true" className="settings-section">
                <h3>{t.webdavConfigSync}</h3>
                <p className="settings-tools-hint">
                  <span aria-hidden="true" className="button-spinner" /> {t.loadingSyncConfig}
                </p>
                {syncMessage ? <p className="modal-error">{syncMessage}</p> : null}
              </section>
            </div>
          ) : null}

          {activeTab === 'updates' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.appUpdates}</h3>
                <div className="update-check-preference">
                  <div>
                    <strong>{t.updateCheckPreference}</strong>
                    <p>{t.updateCheckPreferenceHint}</p>
                  </div>
                  <DropdownSelect
                    className="update-check-preference-select"
                    disabled={!desktopApi || isSavingUpdatePreference}
                    onChange={(value) => setUpdateCheckPreference(value === 'auto')}
                    value={autoCheckUpdates ? 'auto' : 'manual'}
                    options={[
                      { value: 'auto', label: t.autoCheckUpdates },
                      { value: 'manual', label: t.doNotAutoUpdate }
                    ]}
                  />
                </div>
                {updatePreferenceError ? <p className="modal-error">{updatePreferenceError}</p> : null}
                <div className="update-status-card" aria-live="polite">
                  <div>
                    <strong>{t.updateStatus}</strong>
                    <p>{getUpdateStatusLabel(updateStatus, t, autoCheckUpdates)}</p>
                  </div>
                  <span className={`update-status-indicator ${updateStatus?.state ?? 'idle'}`} />
                </div>
                {updateStatus?.state === 'downloading' ? (
                  <div className="update-progress" aria-label={t.updateDownloading}>
                    <span style={{ width: `${updateStatus.progress ?? 0}%` }} />
                  </div>
                ) : null}
                <div className="settings-update-actions">
                  {updateStatus?.state === 'available' ? (
                    <button
                      className="primary-button compact"
                      onClick={() => {
                        if (updateStatus.updateMode === 'release-page') {
                          void desktopApi?.openExternalUrl(
                            updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases'
                          )
                        } else {
                          void desktopApi?.downloadUpdate()
                        }
                      }}
                      type="button"
                    >
                      {updateStatus.updateMode === 'release-page' ? t.openReleasePage : t.downloadUpdate}
                    </button>
                  ) : null}
                  {updateStatus?.state === 'downloaded' ? (
                    <button
                      className="primary-button compact"
                      onClick={() => void desktopApi?.installUpdate()}
                      type="button"
                    >
                      {t.restartToUpdate}
                    </button>
                  ) : null}
                  {updateStatus?.state !== 'downloading' && updateStatus?.state !== 'downloaded' ? (
                    <button
                      className="flat-button compact"
                      disabled={updateStatus?.state === 'checking' || updateStatus?.state === 'unsupported'}
                      onClick={() => void desktopApi?.checkForUpdates()}
                      type="button"
                    >
                      {updateStatus?.state === 'checking' ? t.checkingForUpdates : t.checkForUpdates}
                    </button>
                  ) : null}
                </div>
              </section>
            </div>
          ) : null}

          {activeTab === 'system' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.aboutAppInfo}</h3>
                <div className="about-info-list">
                  <div className="about-info-item">
                    <span className="info-label">{t.versionLabel}</span>
                    <span className="info-value">v{desktopApi?.appVersion ?? '—'}</span>
                  </div>
                  <div className="about-info-item">
                    <span className="info-label">{desktopApi?.runtimeName ?? '—'}</span>
                    <span className="info-value">v{desktopApi?.runtimeVersion ?? '—'}</span>
                  </div>
                  <div className="about-info-item">
                    <span className="info-label">{t.environmentInfo}</span>
                    <span className="info-value">{platformLabel}</span>
                  </div>
                </div>
              </section>

              <section className="settings-section">
                <h3>{t.systemLogsInfo}</h3>
                <div className="logs-shortcut-card">
                  <p>{t.settingsLogsDescription}</p>
                  <button className="flat-button compact" onClick={onOpenLogsDirectory} type="button">
                    <span
                      className="material-symbols-outlined"
                      style={{ fontSize: '14px', marginRight: '4px', verticalAlign: 'middle' }}
                    >
                      folder_open
                    </span>
                    {t.openLogsDirectory}
                  </button>
                </div>
              </section>
            </div>
          ) : null}
        </main>
      </div>
    </div>
  )

  if (inline) {
    return content
  }

  if (standalone) {
    return <div className="manager-window">{content}</div>
  }

  return (
    <div className="modal-backdrop" onClick={syncOperation ? undefined : onClose}>
      {content}
    </div>
  )
}

function getUpdateStatusLabel(status: AppUpdateStatus | null, labels: typeof t, autoCheckUpdates: boolean) {
  if (!status) return autoCheckUpdates ? labels.updateStatusIdle : labels.updateStatusManual
  if (status.state === 'available') {
    const label = status.updateMode === 'release-page' ? labels.updateAvailableManual : labels.updateAvailable
    return label.replace('{version}', status.availableVersion ?? '—')
  }
  if (status.state === 'downloaded') return labels.updateDownloaded.replace('{version}', status.availableVersion ?? '—')
  if (status.state === 'downloading')
    return labels.updateDownloading.replace('{progress}', String(status.progress ?? 0))
  if (status.state === 'not-available') return labels.updateNotAvailable
  if (status.state === 'checking') return labels.checkingForUpdates
  if (status.state === 'error') return `${labels.updateFailed}: ${status.message ?? '—'}`
  if (status.state === 'unsupported') return labels.updateUnsupported
  return labels.updateStatusIdle
}
