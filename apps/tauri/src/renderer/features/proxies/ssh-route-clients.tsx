import { useRef, useState } from 'react'
import type { ProxyProfile, SshRouteClientKind, SshRoutePolicySnapshot } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { Input } from '../../components/common/input/input'
import { SegmentedControl } from '../../components/common/segmented-control/segmented-control'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'

const clients = {
  v2ray: 'V2Ray',
  clashVerge: 'Clash Verge',
  amnezia: 'AmneziaVPN',
  easyTier: 'EasyTier'
} as const

function isDesktopGuiClient(kind: SshRouteClientKind): boolean {
  return kind === 'clashVerge' || kind === 'amnezia'
}

export function SshRouteClients({
  snapshot,
  proxies,
  onSnapshotChange
}: {
  snapshot: SshRoutePolicySnapshot
  proxies: ProxyProfile[]
  onSnapshotChange(snapshot: SshRoutePolicySnapshot, policyChanged: boolean): void
}) {
  const [kind, setKind] = useState<SshRouteClientKind>('v2ray')
  const [executableEdit, setExecutable] = useState<string>()
  const [configEdit, setConfig] = useState<string>()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [confirmStop, setConfirmStop] = useState(false)
  const inFlight = useRef(false)
  const saved = snapshot.policy.clients[kind]
  const executable = executableEdit ?? saved?.executable ?? ''
  const config = configEdit ?? saved?.config ?? ''
  const dirty = executableEdit !== undefined || configEdit !== undefined
  const status = snapshot.clients.find((client) => client.kind === kind)
  const reset = () => {
    setExecutable(undefined)
    setConfig(undefined)
    setError('')
  }
  const run = async (action: (api: NonNullable<typeof window.fileterm>) => Promise<void>) => {
    if (inFlight.current) return
    inFlight.current = true
    setBusy(true)
    setError('')
    try {
      if (!window.fileterm) throw new Error(t.proxyAssignmentApiUnavailable)
      await action(window.fileterm)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      inFlight.current = false
      setBusy(false)
    }
  }
  const control = (start: boolean) =>
    void run(async (api) => {
      onSnapshotChange(await api.controlSshRouteClient(kind, start), false)
    })
  const running = Boolean(status?.running)
  const desktopGui = isDesktopGuiClient(kind)

  return (
    <section className="ssh-route-panel ssh-route-clients">
      <header className="ssh-route-panel-header">
        <div className="ssh-route-panel-heading">
          <div className="ssh-route-panel-title-row">
            <AppIcon className="ssh-route-panel-icon" name="terminal" size={16} />
            <h3>{t.sshRouteClient}</h3>
            <span className={`ssh-route-status-pill ${running ? 'is-running' : 'is-idle'}`} role="status">
              <span aria-hidden="true" className={`ssh-route-dot ${running ? 'is-ok' : 'is-muted'}`} />
              <span>
                {running ? t.sshRouteClientRunning : t.sshRouteClientIdle}
                {status?.lastExit != null ? ` · ${t.sshRouteClientExitCode} ${status.lastExit}` : ''}
              </span>
            </span>
          </div>
          <p className="ssh-route-hint">{t.sshRouteClientHint}</p>
        </div>
        <div className="ssh-route-panel-header-actions">
          <SegmentedControl
            ariaLabel={t.sshRouteClient}
            variant="neutral"
            value={kind}
            options={Object.entries(clients).map(([value, label]) => ({
              value: value as SshRouteClientKind,
              label,
              disabled: busy
            }))}
            onChange={(value) => {
              setKind(value)
              reset()
            }}
          />
        </div>
      </header>

      <div className="ssh-route-panel-body">
        <div className="ssh-route-clients-grid">
          <label className="ssh-route-field">
            <span className="ssh-route-field-label">{t.sshRouteClientExecutable}</span>
            <Input
              className="ssh-route-mono-input"
              disabled={busy || running}
              value={executable}
              onChange={(event) => setExecutable(event.target.value)}
            />
          </label>
          {desktopGui ? (
            <div className="ssh-route-callout">
              <AppIcon className="ssh-route-callout-icon" name="info" size={14} />
              <p>{kind === 'clashVerge' ? t.sshRouteClashVergeHint : t.sshRouteAmneziaHint}</p>
            </div>
          ) : (
            <label className="ssh-route-field">
              <span className="ssh-route-field-label">{t.sshRouteClientConfig}</span>
              <Input
                className="ssh-route-mono-input"
                disabled={busy || running}
                value={config}
                onChange={(event) => setConfig(event.target.value)}
              />
            </label>
          )}
        </div>

        {error ? (
          <p className="ssh-route-banner ssh-route-banner--danger" role="alert">
            {error}
          </p>
        ) : null}

        {status?.endpoints.length ? (
          <div className="ssh-route-endpoints">
            {status.endpoints.map((endpoint) => {
              const existing = proxies.some(
                (proxy) => proxy.type === endpoint.type && proxy.host === endpoint.host && proxy.port === endpoint.port
              )
              return (
                <div className="ssh-route-endpoint-row" key={`${endpoint.type}:${endpoint.host}:${endpoint.port}`}>
                  <div className="ssh-route-endpoint-info">
                    <span className="ssh-route-chip is-info">
                      {endpoint.type === 'http' ? 'HTTP CONNECT' : 'SOCKS5'}
                    </span>
                    <span className="ssh-route-endpoint-addr">
                      {endpoint.host}:{endpoint.port}
                    </span>
                  </div>
                  <Button
                    size="panel"
                    icon={<AppIcon name="plus" size={14} />}
                    disabled={existing || busy}
                    onClick={() =>
                      void run(async (api) => {
                        await api.saveProxyProfile({ ...endpoint, name: `V2Ray ${endpoint.port}` })
                      })
                    }
                  >
                    {t.addProxy}
                  </Button>
                </div>
              )
            })}
          </div>
        ) : null}
      </div>

      <footer className="ssh-route-panel-footer">
        <div className="ssh-route-actions">
          <Button
            size="panel"
            variant="secondary"
            icon={<AppIcon name="play" size={14} />}
            disabled={busy || dirty || !saved || running}
            onClick={() => control(true)}
          >
            {t.sshRouteClientStart}
          </Button>
          {!desktopGui ? (
            <Button
              className="ssh-route-stop-btn"
              size="panel"
              variant="secondary"
              icon={<AppIcon name="close" size={14} />}
              disabled={busy || !running}
              onClick={() => setConfirmStop(true)}
            >
              {t.sshRouteClientStop}
            </Button>
          ) : null}
          <Button
            size="panel"
            variant="primary"
            icon={<AppIcon name="check" size={14} />}
            disabled={busy || running || !dirty || !executable.trim() || (!desktopGui && !config.trim())}
            onClick={() =>
              void run(async (api) => {
                const policy = {
                  ...snapshot.policy,
                  clients: {
                    ...snapshot.policy.clients,
                    [kind]: { executable: executable.trim(), config: desktopGui ? null : config.trim() }
                  }
                }
                onSnapshotChange(await api.saveSshRoutePolicy(snapshot.policy, policy), true)
                reset()
              })
            }
          >
            {t.save}
          </Button>
        </div>
      </footer>

      {confirmStop ? (
        <ConfirmActionDialog
          title={t.sshRouteClientStop}
          description={clients[kind]}
          confirmLabel={t.sshRouteClientStop}
          onClose={() => setConfirmStop(false)}
          onConfirm={() => {
            setConfirmStop(false)
            control(false)
          }}
        />
      ) : null}
    </section>
  )
}
