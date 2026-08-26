import type { RemoteFileCapabilities } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return t.remoteCapabilityUnavailable
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit += 1
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`
}

function capabilityLabel(value: boolean) {
  return value ? t.remoteCapabilityAvailable : t.remoteCapabilityUnavailable
}

export function RemoteCapabilityPanel({ capabilities }: { capabilities?: RemoteFileCapabilities }) {
  if (!capabilities) {
    return null
  }

  const protocol = `${capabilities.protocol.toUpperCase()}${capabilities.protocolVersion ? ` ${capabilities.protocolVersion}` : ''}`
  const diskSpace = capabilities.diskSpace
    ? `${formatBytes(capabilities.diskSpace.availableBytes)} / ${formatBytes(capabilities.diskSpace.totalBytes)}`
    : t.remoteCapabilityUnavailable
  const checksums = capabilities.checksumAlgorithms.length
    ? capabilities.checksumAlgorithms.join(', ')
    : t.remoteCapabilityNone
  const extensions = capabilities.extensions.length ? capabilities.extensions.join(', ') : t.remoteCapabilityNone

  return (
    <details className="remote-capability-panel">
      <summary aria-label={t.remoteCapabilities} title={t.remoteCapabilities}>
        <AppIcon name="server" size={13} />
        <span>{protocol}</span>
        <AppIcon className="remote-capability-panel__chevron" name="chevron-down" size={11} />
      </summary>
      <dl className="remote-capability-panel__body">
        <div>
          <dt>{t.remoteCapabilityProtocol}</dt>
          <dd>{protocol}</dd>
        </div>
        <div>
          <dt>{t.remoteCapabilityDiskSpace}</dt>
          <dd>{diskSpace}</dd>
        </div>
        <div>
          <dt>{t.remoteCapabilityChecksums}</dt>
          <dd>{checksums}</dd>
        </div>
        <div className="remote-capability-panel__extensions">
          <dt>{t.remoteCapabilityExtensions}</dt>
          <dd>{extensions}</dd>
        </div>
        <div>
          <dt>{t.remoteCapabilityServerCopy}</dt>
          <dd>{capabilityLabel(capabilities.serverCopy)}</dd>
        </div>
        <div>
          <dt>{t.remoteCapabilitySymlink}</dt>
          <dd>{capabilityLabel(capabilities.symlink)}</dd>
        </div>
        <div>
          <dt>{t.remoteCapabilityHardlink}</dt>
          <dd>{capabilityLabel(capabilities.hardlink)}</dd>
        </div>
      </dl>
    </details>
  )
}
