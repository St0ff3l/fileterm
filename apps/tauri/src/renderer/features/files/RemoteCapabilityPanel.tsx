import type { RemoteFileCapabilities } from '@fileterm/core'
import { t } from '../../i18n'

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

  const diskSpace = capabilities.diskSpace
    ? `${formatBytes(capabilities.diskSpace.availableBytes)} / ${formatBytes(capabilities.diskSpace.totalBytes)}`
    : t.remoteCapabilityUnavailable
  const checksums = capabilities.checksumAlgorithms.length
    ? capabilities.checksumAlgorithms.join(', ')
    : t.remoteCapabilityNone
  const extensions = capabilities.extensions.length ? capabilities.extensions.join(', ') : t.remoteCapabilityNone

  return (
    <details className="remote-capability-panel">
      <summary>{t.remoteCapabilities}</summary>
      <div className="remote-capability-panel__body">
        <span>
          {t.remoteCapabilityProtocol}: {capabilities.protocol.toUpperCase()}
          {capabilities.protocolVersion ? ` ${capabilities.protocolVersion}` : ''}
        </span>
        <span>
          {t.remoteCapabilityDiskSpace}: {diskSpace}
        </span>
        <span>
          {t.remoteCapabilityChecksums}: {checksums}
        </span>
        <span>
          {t.remoteCapabilityExtensions}: {extensions}
        </span>
        <span>
          {t.remoteCapabilityServerCopy}: {capabilityLabel(capabilities.serverCopy)}
        </span>
        <span>
          {t.remoteCapabilitySymlink}: {capabilityLabel(capabilities.symlink)}
        </span>
        <span>
          {t.remoteCapabilityHardlink}: {capabilityLabel(capabilities.hardlink)}
        </span>
      </div>
    </details>
  )
}
