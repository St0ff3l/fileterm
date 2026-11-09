import { SelectionControl } from '../../features/common/selection-control'

export function TransferSidebarSettingControl({
  checked,
  disabled = false,
  hint,
  label,
  onChange
}: {
  checked: boolean
  disabled?: boolean
  hint: string
  label: string
  onChange(value: boolean): void
}) {
  return (
    <div className="advanced-toggle-row">
      <label className="ssh-checkbox advanced-toggle-label">
        <SelectionControl
          checked={checked}
          disabled={disabled}
          type="checkbox"
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="advanced-toggle-name">{label}</span>
      </label>
      <p className="advanced-toggle-hint">{hint}</p>
    </div>
  )
}
