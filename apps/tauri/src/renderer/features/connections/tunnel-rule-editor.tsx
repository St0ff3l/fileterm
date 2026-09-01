import type { SshForwardRule } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { SelectionControl } from '../common/selection-control'

export function TunnelRuleEditor({
  rule,
  index,
  onChange,
  onRemove
}: {
  rule: SshForwardRule
  index: number
  onChange(patch: Partial<SshForwardRule>): void
  onRemove(): void
}) {
  const isDynamic = rule.kind === 'dynamic'
  return (
    <article className="tunnel-rule-card">
      <header>
        <div>
          <span className="tunnel-rule-index">{String(index + 1).padStart(2, '0')}</span>
          <strong>
            {rule.kind === 'local' ? t.localForward : rule.kind === 'remote' ? t.remoteForward : t.dynamicSocks5}
          </strong>
        </div>
        <button
          type="button"
          className="tunnel-remove-button"
          aria-label={t.deleteTunnel}
          title={t.deleteTunnel}
          onClick={onRemove}
        >
          <span className="material-symbols-outlined">delete</span>
        </button>
      </header>
      <div className="tunnel-rule-grid">
        <label>
          {t.tunnelType}
          <DropdownSelect
            value={rule.kind}
            options={[
              { value: 'local', label: t.localForwardShort },
              { value: 'remote', label: t.remoteForwardShort },
              { value: 'dynamic', label: t.dynamicForwardShort }
            ]}
            onChange={(value) =>
              onChange({
                kind: value as SshForwardRule['kind'],
                ...(value === 'dynamic' ? { targetHost: undefined, targetPort: undefined } : {})
              })
            }
          />
        </label>
        <label>
          {t.tunnelBindHost}
          <input value={rule.bindHost} onChange={(event) => onChange({ bindHost: event.target.value })} />
        </label>
        <label>
          {t.tunnelBindPort}
          <input
            inputMode="numeric"
            value={rule.bindPort || ''}
            onChange={(event) => onChange({ bindPort: Number(event.target.value) })}
          />
        </label>
        {!isDynamic ? (
          <>
            <label>
              {t.tunnelTargetHost}
              <input value={rule.targetHost ?? ''} onChange={(event) => onChange({ targetHost: event.target.value })} />
            </label>
            <label>
              {t.tunnelTargetPort}
              <input
                inputMode="numeric"
                value={rule.targetPort || ''}
                onChange={(event) => onChange({ targetPort: Number(event.target.value) })}
              />
            </label>
          </>
        ) : (
          <div className="tunnel-socks-note">
            <span className="material-symbols-outlined">vpn_key</span>
            {t.tunnelClientTargetHint}
          </div>
        )}
      </div>
      <label className="tunnel-autostart ssh-checkbox">
        <SelectionControl
          type="checkbox"
          checked={rule.autoStart}
          onChange={(event) => onChange({ autoStart: event.target.checked })}
        />
        {t.autoStartAfterConnect}
      </label>
    </article>
  )
}
