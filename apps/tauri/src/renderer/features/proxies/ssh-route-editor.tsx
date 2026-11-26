import type { ProxyProfile, SshNetworkInterfaceInfo, SshRouteCandidate, SshRouteRule } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { PanelActionButton } from '../../components/common/panel-action-button/panel-action-button'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'
import { t } from '../../i18n'
import { sshRouteKey, sshRuleRoutes } from './ssh-route-rules'

export type SshRouteMode = SshRouteRule['mode']

export type SshRouteOption = {
  route: SshRouteCandidate
  label: string
  title?: string
  meta?: string
  tone?: 'info' | 'neutral' | 'offline' | 'danger'
  disabled?: boolean
}

export function sshRouteOptions(proxies: ProxyProfile[], adapters: SshNetworkInterfaceInfo[]): SshRouteOption[] {
  return [
    { route: { type: 'direct' }, label: t.sshRouteSystem, title: t.sshRouteSystem },
    ...proxies.map((proxy): SshRouteOption => {
      const protocol = proxy.type === 'http' ? 'HTTP CONNECT' : 'SOCKS5'
      return {
        route: { type: 'proxy', proxyId: proxy.id },
        label: `${proxy.name} · ${protocol}`,
        title: proxy.name,
        meta: protocol,
        tone: 'info'
      }
    }),
    ...[...adapters]
      .sort((left, right) => Number(right.online) - Number(left.online))
      .map((adapter): SshRouteOption => ({
        route: { type: 'interface', adapterId: adapter.id },
        label: `${adapter.name} · ${adapter.online ? t.sshRouteAdapter : t.sshRouteUnavailable}`,
        title: adapter.name,
        meta: adapter.online ? t.sshRouteAdapter : t.sshRouteUnavailable,
        tone: adapter.online ? 'neutral' : 'offline'
      }))
  ]
}

export function SshRouteEditor({
  value,
  emptyMode = null,
  inheritLabel = t.sshRouteInherit,
  options,
  disabled,
  onChange,
  onEmptyModeChange
}: {
  value: SshRouteRule | null
  emptyMode?: SshRouteMode | null
  inheritLabel?: string
  options: SshRouteOption[]
  disabled: boolean
  onChange(value: SshRouteRule | null): void
  onEmptyModeChange?(mode: SshRouteMode | null): void
}) {
  const routes = sshRuleRoutes(value)
  const mode = value?.mode ?? emptyMode ?? 'inherit'
  const available = options.filter((option) => !option.disabled)
  const next = available.find((option) => !routes.some((route) => sshRouteKey(route) === sshRouteKey(option.route)))
  const replace = (nextMode: string, [first, ...rest]: SshRouteCandidate[]) => {
    if (nextMode === 'inherit') {
      onEmptyModeChange?.(null)
      onChange(null)
      return
    }
    const activeMode = nextMode as SshRouteMode
    if (!first) {
      if (onEmptyModeChange) {
        onEmptyModeChange(activeMode)
        onChange(null)
      } else {
        onChange(null)
      }
      return
    }
    onEmptyModeChange?.(null)
    if (activeMode === 'fixed') return onChange({ mode: 'fixed', route: first })
    if (activeMode === 'preferred') return onChange({ mode: 'preferred', primary: first, fallback: rest })
    onChange({ mode: 'auto', routes: [first, ...rest] })
  }
  const move = (from: number, to: number) => {
    if (to < 0 || to >= routes.length) return
    const reordered = [...routes]
    const [moved] = reordered.splice(from, 1)
    if (!moved) return
    reordered.splice(to, 0, moved)
    replace(mode, reordered)
  }
  const fixed = mode === 'fixed'
  const slots: (SshRouteCandidate | null)[] = mode === 'inherit' ? [] : routes.length > 0 ? routes : [null]

  return (
    <div className="ssh-route-editor">
      <label className="ssh-route-field">
        <span className="ssh-route-field-label">{t.sshRouteMode}</span>
        <DropdownSelect
          ariaLabel={t.sshRouteMode}
          disabled={disabled}
          value={mode}
          options={[
            { value: 'inherit', label: inheritLabel },
            { value: 'auto', label: t.sshRouteAuto },
            { value: 'preferred', label: t.sshRoutePreferred },
            { value: 'fixed', label: t.sshRouteFixed }
          ]}
          onChange={(newMode) => replace(newMode, routes.length || onEmptyModeChange ? routes : [{ type: 'direct' }])}
        />
      </label>

      {mode !== 'inherit' ? (
        <div className="ssh-route-candidates" role="group" aria-label={t.sshRouteChoices}>
          <div className="ssh-route-choices-header">
            <span className="ssh-route-field-label">{t.sshRouteChoices}</span>
            {!fixed ? <span className="ssh-route-choices-count">{routes.length}/4</span> : null}
          </div>
          <p className="ssh-route-hint ssh-route-choices-hint">
            {fixed ? t.sshRouteFixedHint : mode === 'preferred' ? t.sshRoutePreferredHint : t.sshRouteAutoHint}
          </p>
          <div className="ssh-route-slot-list">
            {slots.map((route, index) => {
              const key = route ? sshRouteKey(route) : ''
              const isPrimary = mode === 'preferred' && index === 0
              const slotLabel = isPrimary ? t.sshRoutePreferred : `${t.route} ${index + 1}`
              const choices = options.map((option) => ({
                value: sshRouteKey(option.route),
                label: option.label,
                disabled:
                  option.disabled ||
                  routes.some((other, i) => i !== index && sshRouteKey(other) === sshRouteKey(option.route))
              }))
              if (key && !choices.some((choice) => choice.value === key)) {
                choices.push({ value: key, label: `${t.sshRouteMissing} · ${key}`, disabled: true })
              }
              return (
                <div className="ssh-route-candidate ssh-route-slot" key={key || `empty-${index}`}>
                  <span
                    aria-hidden="true"
                    className={`ssh-route-candidate-index${isPrimary ? ' is-primary' : ''}`}
                    title={slotLabel}
                  >
                    {index + 1}
                  </span>
                  <div className="ssh-route-slot-select">
                    <DropdownSelect
                      ariaLabel={slotLabel}
                      placeholder={t.sshRouteChoices}
                      disabled={disabled}
                      value={key}
                      options={choices}
                      onChange={(newKey) => {
                        const option = available.find((item) => sshRouteKey(item.route) === newKey)
                        if (!option) return
                        if (!route) {
                          replace(mode, [option.route])
                          return
                        }
                        replace(
                          mode,
                          routes.map((item, i) => (index === i ? option.route : item))
                        )
                      }}
                    />
                  </div>
                  {isPrimary ? (
                    <span className="ssh-route-chip is-info ssh-route-slot-badge">{t.sshRoutePrimary}</span>
                  ) : null}
                  {route ? (
                    <div className="ssh-route-slot-actions">
                      {!fixed && routes.length > 1 ? (
                        <>
                          <Button
                            className="ssh-route-slot-btn"
                            disabled={disabled || index === 0}
                            icon={<AppIcon name="arrow-up" size={14} />}
                            size="md"
                            variant="secondary"
                            aria-label={`${slotLabel} ↑`}
                            onClick={() => move(index, index - 1)}
                          />
                          <Button
                            className="ssh-route-slot-btn"
                            disabled={disabled || index === routes.length - 1}
                            icon={<AppIcon name="arrow-down" size={14} />}
                            size="md"
                            variant="secondary"
                            aria-label={`${slotLabel} ↓`}
                            onClick={() => move(index, index + 1)}
                          />
                        </>
                      ) : null}
                      <Button
                        className="ssh-route-slot-btn ssh-route-slot-btn--remove"
                        disabled={disabled}
                        icon={<AppIcon name="trash" size={14} />}
                        size="md"
                        variant="secondary"
                        aria-label={t.sshRouteRemove}
                        title={t.sshRouteRemove}
                        onClick={() =>
                          replace(
                            mode,
                            routes.filter((_, i) => i !== index)
                          )
                        }
                      />
                    </div>
                  ) : null}
                </div>
              )
            })}
            {!fixed && routes.length > 0 && routes.length < 4 && next ? (
              <div className="ssh-route-candidates-footer">
                <PanelActionButton
                  disabled={disabled}
                  icon={<AppIcon name="plus" size={14} />}
                  onClick={() => replace(mode, [...routes, next.route])}
                >
                  {t.sshRouteAdd}
                </PanelActionButton>
              </div>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  )
}
