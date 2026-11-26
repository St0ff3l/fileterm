import { useEffect, useRef, useState, type ReactNode } from 'react'
import type {
  ConnectionProfile,
  ProxyProfile,
  SshNetworkInterfaceInfo,
  SshRoutePolicySnapshot,
  SshRouteRule
} from '@fileterm/core'
import { outboundAddressFamily } from '../../app/ssh-network-interface'
import { Button } from '../../components/common/button/button'
import { FeedbackBubble } from '../../components/common/feedback-bubble/feedback-bubble'
import { Input } from '../../components/common/input/input'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { DropdownSelect } from '../common/dropdown-select'
import type { FeedbackTextTone } from '../common/feedback-text'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { SshRouteEditor, sshRouteOptions, type SshRouteMode } from './ssh-route-editor'
import { SshRouteClients } from './ssh-route-clients'
import {
  applySshRouteEdit,
  effectiveSshRule,
  eligibleSshRoutes,
  fallbackSshRuleSource,
  sshRouteKey,
  sshRuleRoutes,
  type SshRouteEdit
} from './ssh-route-rules'
import './ssh-route-manager.css'

export function SshRouteManager({
  host,
  profiles,
  proxies,
  profileId,
  manualSettings
}: {
  host?: string
  profiles: ConnectionProfile[]
  proxies: ProxyProfile[]
  profileId?: string
  manualSettings?: ReactNode
}) {
  const [snapshot, setSnapshot] = useState<SshRoutePolicySnapshot>()
  const [adapters, setAdapters] = useState<SshNetworkInterfaceInfo[]>([])
  const [targetKey, setTargetKey] = useState('default')
  const [previewServerId, setPreviewServerId] = useState('')
  const [draft, setDraft] = useState<SshRouteRule | null>()
  const [emptyMode, setEmptyMode] = useState<SshRouteMode | null>(null)
  const [priority, setPriority] = useState<string>()
  const hasChanges = draft !== undefined || priority !== undefined || emptyMode !== null
  const [feedback, setFeedback] = useState<{ id: number; message: string; tone: FeedbackTextTone } | null>(null)
  const [busy, setBusy] = useState(false)
  const [policyChanges, setPolicyChanges] = useState(0)
  const [review, setReview] = useState(false)
  const scrollRef = useRef<HTMLDivElement>(null)
  const activityScrollRef = useRef<HTMLUListElement>(null)
  const feedbackSeqRef = useRef(0)
  const inFlight = useRef(false)
  const pushFeedback = (message: string, tone: FeedbackTextTone) => {
    feedbackSeqRef.current += 1
    setFeedback({ id: feedbackSeqRef.current, message, tone })
  }
  const baseline = snapshot?.policy
  const servers = eligibleSshRoutes(profiles)
  const activeTags = [...new Set(servers.flatMap((item) => item.tags ?? []).filter(Boolean))].sort()
  const tags = [...new Set([...activeTags, ...Object.keys(baseline?.tags ?? {})])].sort()
  const targetOptions = [
    { value: 'default', label: t.sshRouteDefault, scope: 'default' as const, id: '' },
    ...servers.map(({ id, name }) => ({ value: `server:${id}`, label: name, scope: 'server' as const, id })),
    ...tags.map((id) => ({ value: `tag:${id}`, label: `${t.sshRouteTag} · ${id}`, scope: 'tag' as const, id })),
    ...activeTags.map((id) => ({
      value: `batch:${id}`,
      label: `${t.sshRouteBatchShort} · ${id}`,
      scope: 'batch' as const,
      id
    }))
  ]
  const target = targetOptions.find((item) => item.value === (profileId ? `server:${profileId}` : targetKey))
  const scope = target?.scope
  const tag = target?.scope === 'tag' || target?.scope === 'batch' ? target.id : ''
  const serverId = target?.scope === 'server' ? target.id : target ? previewServerId : ''
  const server = servers.find((item) => item.id === serverId)
  const tagPolicy = baseline && Object.hasOwn(baseline.tags, tag) ? baseline.tags[tag] : undefined
  const baseRule =
    scope === 'default'
      ? baseline?.default
      : scope === 'tag'
        ? tagPolicy?.rule
        : scope === 'server' && baseline && Object.hasOwn(baseline.servers, serverId)
          ? baseline.servers[serverId]
          : null
  const rule = draft === undefined ? (baseRule ?? null) : draft
  const tagPriority = priority ?? String(tagPolicy?.priority ?? 0)
  const selected = server && baseline ? effectiveSshRule(server, baseline) : undefined
  const routeOptions = sshRouteOptions(proxies, adapters)
  const routeName = (key: string) =>
    routeOptions.find((option) => sshRouteKey(option.route) === key)?.label ?? `${t.sshRouteMissing} · ${key}`
  const savedRouteDetails = selected?.rule
    ? sshRuleRoutes(selected.rule)
        .map((route) => routeName(sshRouteKey(route)))
        .join(' / ')
    : selected?.scope === 'legacy' && server
      ? [
          server.proxyProfileId
            ? routeName(`proxy:${server.proxyProfileId}`)
            : server.proxy && server.proxy.type !== 'none'
              ? t.proxySourceCustom
              : '',
          server.sshNetworkInterface
            ? (routeOptions.find(
                (option) => sshRouteKey(option.route) === `interface:${server.sshNetworkInterface?.adapterId}`
              )?.label ??
              server.sshNetworkInterface.displayName ??
              routeName(`interface:${server.sshNetworkInterface.adapterId}`))
            : ''
        ]
          .filter(Boolean)
          .join(' / ')
      : ''
  const resetDraft = () => {
    setDraft(undefined)
    setEmptyMode(null)
    setPriority(undefined)
  }
  const fetchSnapshot = async () => {
    const api = window.fileterm
    if (!api) throw new Error(t.proxyAssignmentApiUnavailable)
    const [current, inventory] = await Promise.all([
      api.getSshRoutePolicy(),
      api.listSshNetworkInterfaces().catch((): SshNetworkInterfaceInfo[] => [])
    ])
    return { current, inventory }
  }
  useEffect(() => {
    let cancelled = false
    const stopPolicy = window.fileterm?.onSshRoutePolicyChanged(() => setPolicyChanges((count) => count + 1))
    const stop = window.fileterm?.onSshRouteActivity((entry) => {
      setSnapshot((previous) => {
        if (!previous || previous.activity.some(({ id }) => id === entry.id)) return previous
        return { ...previous, activity: [entry, ...previous.activity].slice(0, 160) }
      })
    })
    void fetchSnapshot()
      .then(({ current, inventory }) => {
        if (cancelled) return
        setSnapshot((previous) => previous ?? current)
        setAdapters(inventory)
      })
      .catch((cause: unknown) => {
        if (!cancelled) pushFeedback(cause instanceof Error ? cause.message : String(cause), 'error')
      })
    return () => {
      cancelled = true
      stopPolicy?.()
      stop?.()
    }
  }, [])
  useEffect(() => {
    if (!policyChanges || hasChanges || busy) return
    let cancelled = false
    void window.fileterm?.getSshRoutePolicy().then(
      (next) => {
        if (cancelled) return
        setSnapshot(next)
        setPolicyChanges((count) => (count === policyChanges ? 0 : count))
      },
      () => {}
    )
    return () => {
      cancelled = true
    }
  }, [policyChanges, hasChanges, busy])

  const run = async (action: () => Promise<void>) => {
    if (inFlight.current) return
    inFlight.current = true
    setBusy(true)
    try {
      await action()
    } catch (cause) {
      pushFeedback(cause instanceof Error ? cause.message : String(cause), 'error')
    } finally {
      inFlight.current = false
      setBusy(false)
    }
  }

  const save = () =>
    run(async () => {
      const api = window.fileterm
      if (!baseline || !api || !scope) return
      let edit: SshRouteEdit
      if (scope === 'default') edit = { scope, rule }
      else if (scope === 'tag') edit = { scope, tag, rule, priority: Number(tagPriority) }
      else if (scope === 'server') edit = { scope, profileId: serverId, rule }
      else if (rule) edit = { scope, tag, rule }
      else return
      const policy = applySshRouteEdit(baseline, edit, profiles)
      setSnapshot(await api.saveSshRoutePolicy(baseline, policy))
      setDraft(undefined)
      setEmptyMode(null)
      setPriority(undefined)
      pushFeedback(t.sshRouteSaved, 'success')
    })
  const probeSelected = () =>
    void run(async () => {
      const next = await window.fileterm?.probeSshRoutePolicy(serverId)
      if (next) setSnapshot((current) => current && { ...current, activity: next.activity })
    })
  const recent = snapshot?.activity.filter((entry) => !serverId || entry.profileId === serverId) ?? []
  const lastAuthenticated = serverId ? recent.find((entry) => entry.stage === 'authenticated') : undefined
  const scopes = {
    default: t.sshRouteDefault,
    tag: t.sshRouteTag,
    server: t.sshRouteServer,
    batch: t.sshRouteBatch
  }
  const invalidPriority =
    scope === 'tag' &&
    Boolean(rule || emptyMode) &&
    (!tagPriority.trim() ||
      !Number.isInteger(Number(tagPriority)) ||
      Number(tagPriority) < -2147483648 ||
      Number(tagPriority) > 2147483647)
  const missingRoutes = emptyMode !== null || (scope === 'batch' && !rule)
  const showStatusTargetPicker = !profileId && scope !== 'server'
  const selectedScopeLabel = selected
    ? selected.scope === 'legacy'
      ? t.sshRouteLegacy
      : selected.scope === 'conflict'
        ? t.sshRouteConflict
        : selected.scope === 'default'
          ? selected.rule
            ? t.sshRouteDefault
            : t.sshRouteSystem
          : `${scopes[selected.scope]} · ${selected.label}`
    : ''
  const selectedScopeTone =
    selected?.scope === 'conflict' ? 'is-warning' : selected?.scope === 'legacy' ? 'is-muted' : 'is-info'

  return (
    <div className={profileId ? 'ssh-route-manager ssh-route-manager--inline' : 'ssh-route-manager'}>
      <div className="ssh-route-content" ref={scrollRef}>
        <section className="ssh-route-panel ssh-route-panel--rules">
          <header className="ssh-route-panel-header">
            <div className="ssh-route-panel-heading">
              <div className="ssh-route-panel-title-row">
                <AppIcon className="ssh-route-panel-icon" name="connections" size={16} />
                <h3>{profileId ? t.sshRouteConnectionExit : t.sshRouteRulesHeading}</h3>
              </div>
              <p className="ssh-route-hint">{profileId ? t.sshRouteConnectionSaveHint : t.sshRouteDraftHint}</p>
            </div>
            <div className="ssh-route-panel-header-actions">
              <Button
                size="panel"
                variant="secondary"
                icon={<AppIcon name="refresh" size={14} />}
                loading={busy}
                onClick={() =>
                  void run(async () => {
                    const { current, inventory } = await fetchSnapshot()
                    setSnapshot(current)
                    setAdapters(inventory)
                  })
                }
              >
                {t.refresh}
              </Button>
            </div>
          </header>

          <div className="ssh-route-panel-body">
            {profileId && selected ? (
              <div className="ssh-route-summary">
                <div className="ssh-route-summary-header">
                  <strong className="ssh-route-summary-title">{t.sshRouteSavedExit}</strong>
                  <span className={`ssh-route-chip ${selectedScopeTone}`}>{selectedScopeLabel}</span>
                </div>
                {savedRouteDetails ? <p className="ssh-route-hint">{savedRouteDetails}</p> : null}
              </div>
            ) : null}
            {!profileId ? (
              <label className="ssh-route-field">
                <span className="ssh-route-field-label">{t.sshRouteApplyTo}</span>
                <DropdownSelect
                  ariaLabel={t.sshRouteApplyTo}
                  placeholder={t.sshRouteApplyTo}
                  value={targetKey}
                  options={targetOptions}
                  disabled={busy}
                  onChange={(value) => {
                    const next = targetOptions.find((item) => item.value === value)
                    if (next?.scope === 'server') setPreviewServerId(next.id)
                    setTargetKey(value)
                    resetDraft()
                  }}
                />
              </label>
            ) : null}

            {scope === 'tag' && (rule || emptyMode) ? (
              <label className="ssh-route-field">
                <span className="ssh-route-field-label">{t.sshRoutePriority}</span>
                <Input
                  type="number"
                  step="1"
                  isError={invalidPriority}
                  aria-invalid={invalidPriority}
                  value={tagPriority}
                  disabled={busy}
                  onChange={(event) => setPriority(event.target.value)}
                />
              </label>
            ) : null}

            {target && baseline ? (
              <SshRouteEditor
                disabled={busy}
                emptyMode={emptyMode}
                inheritLabel={
                  scope === 'server'
                    ? (() => {
                        const fallback = server ? fallbackSshRuleSource(server, baseline) : { kind: 'direct' as const }
                        if (fallback.kind === 'manual') return t.sshRouteFollowManual
                        if (fallback.kind === 'conflict') return t.sshRouteFollowConflict
                        if (fallback.kind === 'tag') return t.sshRouteFollowTag.replace('{tag}', fallback.tag)
                        if (fallback.kind === 'global') return t.sshRouteFollowGlobal
                        return t.sshRouteFollowDirect
                      })()
                    : scope === 'tag'
                      ? t.sshRouteInheritTag
                      : scope === 'batch'
                        ? t.sshRouteInheritBatch
                        : t.sshRouteInherit
                }
                onChange={setDraft}
                onEmptyModeChange={setEmptyMode}
                options={routeOptions}
                value={rule}
              />
            ) : null}
            {scope === 'server' &&
            sshRuleRoutes(rule).some((candidate) => candidate.type === 'interface') &&
            !outboundAddressFamily(host ?? server?.host ?? '') ? (
              <p className="ssh-route-interface-warning" role="status">
                {t.sshInterfaceAddressRequired}
              </p>
            ) : null}
            {profileId && baseline && !baseRule && !rule && !emptyMode ? manualSettings : null}
          </div>

          <footer className="ssh-route-panel-footer">
            <div className="ssh-route-actions">
              {hasChanges ? (
                <Button size="panel" disabled={busy} onClick={resetDraft}>
                  {t.cancel}
                </Button>
              ) : null}
              <Button
                size="panel"
                variant="primary"
                icon={<AppIcon name="check" size={14} />}
                disabled={
                  !baseline ||
                  !target ||
                  !hasChanges ||
                  invalidPriority ||
                  (scope === 'batch' && !servers.some((item) => item.tags?.includes(tag))) ||
                  busy
                }
                onClick={() => {
                  if (missingRoutes) {
                    pushFeedback(t.sshRouteSelectRequired, 'warning')
                    return
                  }
                  if (scope === 'batch') setReview(true)
                  else void save()
                }}
              >
                {profileId ? t.sshRouteSaveConnectionRule : t.save}
              </Button>
            </div>
          </footer>
        </section>

        {!profileId ? (
          <section className="ssh-route-panel ssh-route-panel--status">
            <header className="ssh-route-panel-header">
              <div className="ssh-route-panel-heading">
                <div className="ssh-route-panel-title-row">
                  <AppIcon className="ssh-route-panel-icon" name="history" size={16} />
                  <h3>{t.sshRouteHistory}</h3>
                </div>
              </div>
              {!showStatusTargetPicker && selected ? (
                <div className="ssh-route-panel-header-actions">
                  <Button
                    size="panel"
                    variant="secondary"
                    icon={<AppIcon name="flash" size={14} />}
                    disabled={busy || !selected.rule}
                    onClick={probeSelected}
                  >
                    {t.sshRouteProbe}
                  </Button>
                </div>
              ) : null}
            </header>

            <div className="ssh-route-panel-body">
              {showStatusTargetPicker ? (
                <div className="ssh-route-status-toolbar">
                  <DropdownSelect
                    ariaLabel={t.sshRouteSelectTarget}
                    placeholder={t.sshRouteAllServers}
                    value={serverId}
                    disabled={busy}
                    options={[
                      { value: '', label: t.sshRouteAllServers },
                      ...servers.map((item) => ({ value: item.id, label: item.name }))
                    ]}
                    onChange={setPreviewServerId}
                  />
                  {selected ? (
                    <Button
                      size="md"
                      variant="secondary"
                      icon={<AppIcon name="flash" size={14} />}
                      disabled={busy || !selected.rule}
                      onClick={probeSelected}
                    >
                      {t.sshRouteProbe}
                    </Button>
                  ) : null}
                </div>
              ) : null}

              {selected ? (
                <div className="ssh-route-summary">
                  <div className="ssh-route-summary-header">
                    <strong className="ssh-route-summary-title">{t.sshRouteStatus}</strong>
                    <span className={`ssh-route-chip ${selectedScopeTone}`}>{selectedScopeLabel}</span>
                  </div>
                  {selected.scope === 'legacy' && savedRouteDetails ? (
                    <p className="ssh-route-hint">{savedRouteDetails}</p>
                  ) : null}
                  {lastAuthenticated ? (
                    <div className="ssh-route-auth-row">
                      <AppIcon className="ssh-route-auth-icon" name="shield-check" size={13} />
                      <span>
                        {t.sshRouteAuthenticated} · {routeName(sshRouteKey(lastAuthenticated.route))}
                      </span>
                    </div>
                  ) : null}
                  {sshRuleRoutes(selected.rule).length > 0 ? (
                    <ul className="ssh-route-summary-list">
                      {sshRuleRoutes(selected.rule).map((entry) => {
                        const key = sshRouteKey(entry)
                        const observation = recent.find(
                          (item) => item.stage !== 'authenticated' && sshRouteKey(item.route) === key
                        )
                        const unavailable = !routeOptions.some((option) => sshRouteKey(option.route) === key)
                        const stateClass =
                          unavailable || observation?.failure ? 'is-danger' : observation ? 'is-ok' : 'is-muted'
                        return (
                          <li className="ssh-route-summary-item" key={key}>
                            <span className="ssh-route-summary-route">
                              <span aria-hidden="true" className={`ssh-route-dot ${stateClass}`} />
                              <span>{routeName(key)}</span>
                            </span>
                            <span className={`ssh-route-summary-metric ${stateClass}`}>
                              {unavailable
                                ? t.sshRouteUnavailable
                                : observation
                                  ? (observation.failure ?? `${t.sshRouteReady} · ${observation.elapsedMs} ms`)
                                  : t.sshRouteUnknown}
                            </span>
                          </li>
                        )
                      })}
                    </ul>
                  ) : null}
                </div>
              ) : null}

              {recent.length ? (
                <div className="ssh-route-activity-wrap">
                  <ul className="ssh-route-activity" ref={activityScrollRef}>
                    {recent.slice(0, 24).map((entry, index) => {
                      const isAuth = entry.stage === 'authenticated'
                      const isFailed = Boolean(entry.failure)
                      const dotClass = isFailed ? 'is-danger' : isAuth ? 'is-info' : 'is-ok'
                      return (
                        <li className="ssh-route-activity-item" key={`${entry.observedAt}:${index}`}>
                          <div className="ssh-route-activity-head">
                            <span className="ssh-route-activity-route">
                              <span aria-hidden="true" className={`ssh-route-dot ${dotClass}`} />
                              <strong>{routeName(sshRouteKey(entry.route))}</strong>
                            </span>
                            <span className="ssh-route-activity-time">
                              {new Date(entry.observedAt).toLocaleString()}
                            </span>
                          </div>
                          <div className="ssh-route-activity-meta">
                            <span className={`ssh-route-activity-status ${dotClass}`}>
                              {isAuth ? t.sshRouteAuthenticated : (entry.failure ?? t.sshRouteReady)}
                            </span>
                            {!isAuth && !isFailed ? (
                              <span className="ssh-route-activity-latency">{entry.elapsedMs} ms</span>
                            ) : null}
                            {entry.stage === 'probe' ? (
                              <span className="ssh-route-chip is-muted">{t.sshRouteTestOnly}</span>
                            ) : null}
                            {entry.fallback ? (
                              <span className="ssh-route-chip is-warning">{t.sshRouteFallback}</span>
                            ) : null}
                          </div>
                        </li>
                      )
                    })}
                  </ul>
                  <VerticalScrollbar scrollRef={activityScrollRef} />
                </div>
              ) : (
                <div className="ssh-route-empty">
                  <AppIcon name="history" size={16} />
                  <span>{t.sshRouteUnknown}</span>
                </div>
              )}
            </div>
          </section>
        ) : null}

        {!profileId && snapshot ? (
          <SshRouteClients
            snapshot={snapshot}
            proxies={proxies}
            onSnapshotChange={(next, policyChanged) => {
              setSnapshot((current) =>
                !current || policyChanged ? next : { ...current, clients: next.clients, activity: next.activity }
              )
            }}
          />
        ) : null}
      </div>
      {!profileId ? <VerticalScrollbar scrollRef={scrollRef} /> : null}
      <FeedbackBubble eventId={feedback?.id} message={feedback?.message} tone={feedback?.tone} />
      {review ? (
        <ConfirmActionDialog
          title={t.sshRouteBatch}
          description={t.sshRouteBulkWarning}
          confirmLabel={t.save}
          onClose={() => setReview(false)}
          onConfirm={() => {
            setReview(false)
            void save()
          }}
        />
      ) : null}
    </div>
  )
}
