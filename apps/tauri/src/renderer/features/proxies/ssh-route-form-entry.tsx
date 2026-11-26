import { useEffect, useState, type ReactNode } from 'react'
import type {
  ConnectionProfile,
  CreateProfileInput,
  SshNetworkInterfaceInfo,
  SshRoutePolicy,
  SshRouteRule
} from '@fileterm/core'
import { outboundAddressFamily } from '../../app/ssh-network-interface'
import { Button } from '../../components/common/button/button'
import { useProxyLibrary } from '../../hooks/use-proxy-library'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { SshRouteEditor, sshRouteOptions, type SshRouteMode } from './ssh-route-editor'
import { SshRouteManager } from './ssh-route-manager'
import { fallbackSshRuleSource, sshRuleRoutes } from './ssh-route-rules'
import './ssh-route-manager.css'

export function formatServerInheritLabel(source: ReturnType<typeof fallbackSshRuleSource>): string {
  if (source.kind === 'manual') return t.sshRouteFollowManual
  if (source.kind === 'conflict') return t.sshRouteFollowConflict
  if (source.kind === 'tag') return t.sshRouteFollowTag.replace('{tag}', source.tag)
  if (source.kind === 'global') return t.sshRouteFollowGlobal
  return t.sshRouteFollowDirect
}

export function SshRouteFormEntry({
  host,
  profileId,
  profiles,
  children
}: {
  host?: string
  profileId: string
  profiles: ConnectionProfile[]
  children: ReactNode
}) {
  const { proxies } = useProxyLibrary()
  return (
    <SshRouteManager
      host={host}
      profileId={profileId}
      profiles={profiles}
      proxies={proxies}
      manualSettings={children}
    />
  )
}

export function NewSshRouteFormEntry({
  form,
  rule,
  onChange,
  children
}: {
  form: Pick<CreateProfileInput, 'host' | 'tags' | 'sshNetworkInterface' | 'proxyProfileId' | 'proxy'>
  rule: SshRouteRule | null
  onChange(rule: SshRouteRule | null): void
  children: ReactNode
}) {
  const onWindows = window.fileterm?.platform === 'win32'
  const { proxies } = useProxyLibrary()
  const [adapters, setAdapters] = useState<SshNetworkInterfaceInfo[]>([])
  const [policy, setPolicy] = useState<SshRoutePolicy>()
  const [emptyMode, setEmptyMode] = useState<SshRouteMode | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [refreshTick, setRefreshTick] = useState(0)

  useEffect(() => {
    let cancelled = false
    const api = window.fileterm
    if (!api) return
    void Promise.all([
      api.listSshNetworkInterfaces?.().catch((): SshNetworkInterfaceInfo[] => []),
      api.getSshRoutePolicy?.().catch(() => undefined)
    ])
      .then(([items, snapshot]) => {
        if (cancelled) return
        if (items) setAdapters(items)
        if (snapshot) setPolicy(snapshot.policy)
      })
      .finally(() => {
        if (!cancelled) setRefreshing(false)
      })
    return () => {
      cancelled = true
    }
  }, [refreshTick])

  const hasInterfaceRoute = sshRuleRoutes(rule).some((candidate) => candidate.type === 'interface')
  const showAddressWarning = hasInterfaceRoute && !outboundAddressFamily(form.host ?? '')
  const inheritLabel = formatServerInheritLabel(fallbackSshRuleSource(form, policy))

  return (
    <>
      <fieldset className="ssh-fieldset ssh-route-entry-fieldset">
        <legend>{t.sshRouteConnectionExit}</legend>
        <div className="network-card__body">
          <div className="ssh-route-entry-header">
            <p className="ssh-route-hint">{t.sshRouteNewSaveHint}</p>
            {onWindows ? (
              <Button
                size="panel"
                variant="secondary"
                icon={<AppIcon name="refresh" size={14} />}
                loading={refreshing}
                onClick={() => {
                  setRefreshing(true)
                  setRefreshTick((value) => value + 1)
                }}
              >
                {refreshing ? t.refreshingList : t.refreshList}
              </Button>
            ) : null}
          </div>
          <SshRouteEditor
            disabled={false}
            emptyMode={emptyMode}
            inheritLabel={inheritLabel}
            value={rule}
            options={sshRouteOptions(proxies, adapters)}
            onChange={onChange}
            onEmptyModeChange={setEmptyMode}
          />
          {showAddressWarning ? (
            <p className="ssh-route-interface-warning" role="status">
              {t.sshInterfaceAddressRequired}
            </p>
          ) : null}
        </div>
      </fieldset>
      {rule || emptyMode ? null : children}
    </>
  )
}
