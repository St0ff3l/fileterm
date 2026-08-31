import type { Dispatch, SetStateAction } from 'react'
import type {
  BackupDownloadMode,
  BackupUploadMode,
  FileTermDesktopApi,
  S3BackupConfig,
  WebDavSyncConfig
} from '@fileterm/core'
import { AppIcon } from '../../../common/AppIcon'
import { DropdownSelect } from '../../../common/DropdownSelect'
import { FeedbackText } from '../../../common/FeedbackText'
import { SelectionControl } from '../../../common/SelectionControl'
import { StableButtonContent } from '../../../common/StableButtonContent'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import { type SyncFeedback } from '../constants'

type SyncOperation =
  'load' | 'save' | 'test' | 'upload' | 'download' | 's3-save' | 's3-test' | 's3-upload' | 's3-download' | null

type SyncSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  syncConfig: WebDavSyncConfig | null
  setSyncConfig: Dispatch<SetStateAction<WebDavSyncConfig | null>>
  syncPassword: string
  setSyncPassword: Dispatch<SetStateAction<string>>
  syncFeedback: SyncFeedback | null
  setSyncFeedback: Dispatch<SetStateAction<SyncFeedback | null>>
  syncSubTab: 'webdav' | 's3'
  setSyncSubTab: Dispatch<SetStateAction<'webdav' | 's3'>>
  syncOperation: SyncOperation
  runSyncOperation(operation: Exclude<SyncOperation, 'load' | null>, action: () => Promise<void>): Promise<void>
  openSecuritySettings(focusBackupPassword?: boolean): void
  s3Config: S3BackupConfig | null
  setS3Config: Dispatch<SetStateAction<S3BackupConfig | null>>
  s3SecretAccessKey: string
  setS3SecretAccessKey: Dispatch<SetStateAction<string>>
  s3Feedback: SyncFeedback | null
  setS3Feedback: Dispatch<SetStateAction<SyncFeedback | null>>
  backupUploadMode: BackupUploadMode
  setBackupUploadMode: Dispatch<SetStateAction<BackupUploadMode>>
  backupDownloadMode: BackupDownloadMode
  setBackupDownloadMode: Dispatch<SetStateAction<BackupDownloadMode>>
}

export function SyncSettingsPanel() {
  const {
    t,
    desktopApi,
    syncConfig,
    setSyncConfig,
    syncPassword,
    setSyncPassword,
    syncFeedback,
    setSyncFeedback,
    syncSubTab,
    setSyncSubTab,
    syncOperation,
    runSyncOperation,
    openSecuritySettings,
    s3Config,
    setS3Config,
    s3SecretAccessKey,
    setS3SecretAccessKey,
    s3Feedback,
    setS3Feedback,
    backupUploadMode,
    setBackupUploadMode,
    backupDownloadMode,
    setBackupDownloadMode
  } = useSettingsModalContext<SyncSettingsPanelContext>()

  if (!syncConfig) {
    return (
      <div className="settings-panel">
        <section aria-busy="true" className="settings-section">
          <h3>{t.webdavConfigSync}</h3>
          <p className="settings-tools-hint">
            <span aria-hidden="true" className="button-spinner" /> {t.loadingSyncConfig}
          </p>
          {syncFeedback?.kind === 'error' ? <FeedbackText message={syncFeedback.message} tone="error" /> : null}
        </section>
      </div>
    )
  }

  return (
    <div className="settings-panel">
      <div className="sync-subtabs">
        <button
          type="button"
          className={`sync-subtab-button ${syncSubTab === 'webdav' ? 'active' : ''}`}
          onClick={() => setSyncSubTab('webdav')}
        >
          <AppIcon name="cloud" size={15} />
          <span>WebDAV</span>
        </button>
        <button
          type="button"
          className={`sync-subtab-button ${syncSubTab === 's3' ? 'active' : ''}`}
          onClick={() => setSyncSubTab('s3')}
        >
          <AppIcon name="database" size={15} />
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
                  <SelectionControl
                    type="checkbox"
                    checked={syncConfig.enabled}
                    onChange={(event) => setSyncConfig({ ...syncConfig, enabled: event.target.checked })}
                  />
                  {t.enableWebdavSync}
                </label>
                <label className="webdav-checkbox ssh-checkbox">
                  <SelectionControl
                    type="checkbox"
                    checked={syncConfig.allowInsecureTls === true}
                    onChange={(event) => setSyncConfig({ ...syncConfig, allowInsecureTls: event.target.checked })}
                  />
                  {t.allowInsecureHttp}
                </label>
              </div>
            </div>

            <div className="sync-config-actions-row">
              <div className="sync-config-primary-buttons">
                <button
                  aria-busy={syncOperation === 'save'}
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
                      setSyncFeedback({ kind: 'success', message: t.syncConfigSaved })
                    })
                  }}
                >
                  <StableButtonContent
                    busy={syncOperation === 'save'}
                    icon={<AppIcon name="disk" size={14} />}
                    label={t.save}
                  />
                </button>
                <button
                  aria-busy={syncOperation === 'test'}
                  className="flat-button compact"
                  disabled={syncOperation !== null}
                  type="button"
                  onClick={() => {
                    if (!desktopApi) return
                    void runSyncOperation('test', async () => {
                      const result = await desktopApi.testWebDavSync()
                      setSyncFeedback({ kind: 'success', message: result.message })
                    })
                  }}
                >
                  <StableButtonContent
                    busy={syncOperation === 'test'}
                    icon={<AppIcon name="flash" size={14} />}
                    label={t.webdavTestConnection}
                  />
                </button>
                {syncFeedback ? (
                  <FeedbackText
                    className="sync-feedback-text"
                    message={syncFeedback.message}
                    tone={syncFeedback.kind}
                  />
                ) : null}
              </div>

              {syncConfig.lastSyncedAt ? (
                <div className="sync-last-synced-badge">
                  <AppIcon name="history" size={13} />
                  <span>{t.lastSync.replace('{time}', new Date(syncConfig.lastSyncedAt).toLocaleString())}</span>
                </div>
              ) : null}
            </div>

            <div className="sync-operations-card">
              <div className="sync-operations-card-header">
                <div className="sync-operations-card-title">
                  <AppIcon name="refresh" size={15} />
                  <h4>{t.manualSyncTitle}</h4>
                </div>
                <div className="sync-operations-card-subtitle">
                  <span>{t.manualSyncDescription}</span>
                  <button className="sync-security-link" type="button" onClick={() => openSecuritySettings(true)}>
                    <AppIcon name="shield-check" size={12} />
                    <span>{t.securityOpenSettings}</span>
                  </button>
                </div>
              </div>

              <div className="sync-operations-grid">
                <div className="sync-op-box">
                  <div className="sync-op-box-header">
                    <div className="sync-op-badge upload">
                      <AppIcon name="upload" size={14} />
                    </div>
                    <div>
                      <span className="sync-op-name">{t.syncUploadTitle}</span>
                      <p className="sync-op-help">{t.syncUploadHint}</p>
                    </div>
                  </div>
                  <div className="sync-op-controls-row">
                    <div className="sync-op-select-field">
                      <span className="sync-op-label">{t.syncUploadMode}</span>
                      <DropdownSelect
                        value={backupUploadMode}
                        options={[
                          { value: 'overwrite-cloud', label: t.syncUploadOverwriteCloud },
                          { value: 'merge-cloud', label: t.syncUploadMergeCloud }
                        ]}
                        onChange={(value) => setBackupUploadMode(value as BackupUploadMode)}
                      />
                    </div>
                    <button
                      aria-busy={syncOperation === 'upload'}
                      className="flat-button compact sync-op-btn"
                      disabled={!syncConfig.enabled || syncOperation !== null}
                      type="button"
                      onClick={() => {
                        if (!desktopApi) return
                        void runSyncOperation('upload', async () => {
                          const result = await desktopApi.uploadWebDavSync(backupUploadMode)
                          setSyncFeedback({ kind: 'success', message: result.message })
                        })
                      }}
                    >
                      <StableButtonContent
                        busy={syncOperation === 'upload'}
                        icon={<AppIcon name="upload" size={13} />}
                        label={t.syncUpload}
                      />
                    </button>
                  </div>
                </div>

                <div className="sync-op-box">
                  <div className="sync-op-box-header">
                    <div className="sync-op-badge download">
                      <AppIcon name="download" size={14} />
                    </div>
                    <div>
                      <span className="sync-op-name">{t.syncDownloadTitle}</span>
                      <p className="sync-op-help">{t.syncDownloadHint}</p>
                    </div>
                  </div>
                  <div className="sync-op-controls-row">
                    <div className="sync-op-select-field">
                      <span className="sync-op-label">{t.syncDownloadMode}</span>
                      <DropdownSelect
                        value={backupDownloadMode}
                        options={[
                          { value: 'merge-local', label: t.syncDownloadMergeLocal },
                          { value: 'overwrite-local', label: t.syncDownloadOverwriteLocal }
                        ]}
                        onChange={(value) => setBackupDownloadMode(value as BackupDownloadMode)}
                      />
                    </div>
                    <button
                      aria-busy={syncOperation === 'download'}
                      className="flat-button compact sync-op-btn"
                      disabled={!syncConfig.enabled || syncOperation !== null}
                      type="button"
                      onClick={() => {
                        if (!desktopApi) return
                        void runSyncOperation('download', async () => {
                          const result = await desktopApi.downloadWebDavSync(backupDownloadMode)
                          setSyncFeedback({ kind: 'success', message: result.message })
                        })
                      }}
                    >
                      <StableButtonContent
                        busy={syncOperation === 'download'}
                        icon={<AppIcon name="download" size={13} />}
                        label={t.syncDownload}
                      />
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </fieldset>
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
                  <SelectionControl
                    type="checkbox"
                    checked={s3Config.enabled}
                    onChange={(event) => setS3Config({ ...s3Config, enabled: event.target.checked })}
                  />
                  {t.enableS3Backup}
                </label>
                <label className="webdav-checkbox ssh-checkbox">
                  <SelectionControl
                    type="checkbox"
                    disabled={s3Config.provider === 'cloudflare-r2' || s3Config.provider === 'bitiful-s4'}
                    checked={s3Config.pathStyleAccessEnabled}
                    onChange={(event) => setS3Config({ ...s3Config, pathStyleAccessEnabled: event.target.checked })}
                  />
                  {t.s3PathStyle}
                </label>
              </div>
            </div>

            <div className="sync-config-actions-row">
              <div className="sync-config-primary-buttons">
                <button
                  aria-busy={syncOperation === 's3-save'}
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
                      setS3Feedback({ kind: 'success', message: t.s3BackupSaved })
                    })
                  }}
                >
                  <StableButtonContent
                    busy={syncOperation === 's3-save'}
                    icon={<AppIcon name="disk" size={14} />}
                    label={t.save}
                  />
                </button>
                <button
                  aria-busy={syncOperation === 's3-test'}
                  className="flat-button compact"
                  disabled={syncOperation !== null}
                  type="button"
                  onClick={() => {
                    if (!desktopApi) return
                    void runSyncOperation('s3-test', async () => {
                      const result = await desktopApi.testS3Backup()
                      setS3Feedback({ kind: 'success', message: result.message })
                    })
                  }}
                >
                  <StableButtonContent
                    busy={syncOperation === 's3-test'}
                    icon={<AppIcon name="flash" size={14} />}
                    label={t.s3TestConnection}
                  />
                </button>
                {s3Feedback ? (
                  <FeedbackText className="sync-feedback-text" message={s3Feedback.message} tone={s3Feedback.kind} />
                ) : null}
              </div>

              {s3Config.lastSyncedAt ? (
                <div className="sync-last-synced-badge">
                  <AppIcon name="history" size={13} />
                  <span>{t.lastSync.replace('{time}', new Date(s3Config.lastSyncedAt).toLocaleString())}</span>
                </div>
              ) : null}
            </div>

            <div className="sync-operations-card">
              <div className="sync-operations-card-header">
                <div className="sync-operations-card-title">
                  <AppIcon name="refresh" size={15} />
                  <h4>{t.manualSyncTitle}</h4>
                </div>
                <div className="sync-operations-card-subtitle">
                  <span>{t.manualSyncDescription}</span>
                  <button className="sync-security-link" type="button" onClick={() => openSecuritySettings(true)}>
                    <AppIcon name="shield-check" size={12} />
                    <span>{t.securityOpenSettings}</span>
                  </button>
                </div>
              </div>

              <div className="sync-operations-grid">
                <div className="sync-op-box">
                  <div className="sync-op-box-header">
                    <div className="sync-op-badge upload">
                      <AppIcon name="upload" size={14} />
                    </div>
                    <div>
                      <span className="sync-op-name">{t.syncUploadTitle}</span>
                      <p className="sync-op-help">{t.syncUploadHint}</p>
                    </div>
                  </div>
                  <div className="sync-op-controls-row">
                    <div className="sync-op-select-field">
                      <span className="sync-op-label">{t.syncUploadMode}</span>
                      <DropdownSelect
                        value={backupUploadMode}
                        options={[
                          { value: 'overwrite-cloud', label: t.syncUploadOverwriteCloud },
                          { value: 'merge-cloud', label: t.syncUploadMergeCloud }
                        ]}
                        onChange={(value) => setBackupUploadMode(value as BackupUploadMode)}
                      />
                    </div>
                    <button
                      aria-busy={syncOperation === 's3-upload'}
                      className="flat-button compact sync-op-btn"
                      disabled={!s3Config.enabled || syncOperation !== null}
                      type="button"
                      onClick={() => {
                        if (!desktopApi) return
                        void runSyncOperation('s3-upload', async () => {
                          const result = await desktopApi.uploadS3Backup(backupUploadMode)
                          setS3Feedback({ kind: 'success', message: result.message })
                        })
                      }}
                    >
                      <StableButtonContent
                        busy={syncOperation === 's3-upload'}
                        icon={<AppIcon name="upload" size={13} />}
                        label={t.syncUpload}
                      />
                    </button>
                  </div>
                </div>

                <div className="sync-op-box">
                  <div className="sync-op-box-header">
                    <div className="sync-op-badge download">
                      <AppIcon name="download" size={14} />
                    </div>
                    <div>
                      <span className="sync-op-name">{t.syncDownloadTitle}</span>
                      <p className="sync-op-help">{t.syncDownloadHint}</p>
                    </div>
                  </div>
                  <div className="sync-op-controls-row">
                    <div className="sync-op-select-field">
                      <span className="sync-op-label">{t.syncDownloadMode}</span>
                      <DropdownSelect
                        value={backupDownloadMode}
                        options={[
                          { value: 'merge-local', label: t.syncDownloadMergeLocal },
                          { value: 'overwrite-local', label: t.syncDownloadOverwriteLocal }
                        ]}
                        onChange={(value) => setBackupDownloadMode(value as BackupDownloadMode)}
                      />
                    </div>
                    <button
                      aria-busy={syncOperation === 's3-download'}
                      className="flat-button compact sync-op-btn"
                      disabled={!s3Config.enabled || syncOperation !== null}
                      type="button"
                      onClick={() => {
                        if (!desktopApi) return
                        void runSyncOperation('s3-download', async () => {
                          const result = await desktopApi.downloadS3Backup(backupDownloadMode)
                          setS3Feedback({ kind: 'success', message: result.message })
                        })
                      }}
                    >
                      <StableButtonContent
                        busy={syncOperation === 's3-download'}
                        icon={<AppIcon name="download" size={13} />}
                        label={t.syncDownload}
                      />
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </fieldset>
        </section>
      )}
    </div>
  )
}
