import { useEffect, useId, useState, type FormEvent } from 'react'
import type { TopologyTcpProbeResult } from '@fileterm/core'
import { Button } from '../../../components/common/button/button'
import { Input } from '../../../components/common/input/input'
import { StatusIndicator } from '../../common/status-indicator'
import { t } from '../../../i18n'
import { TopologySettingsDisclosure } from './topology-settings-disclosure'
import {
  TOPOLOGY_SERVICE_HOST_LIMIT,
  TOPOLOGY_SERVICE_BOARD_LIMIT,
  TOPOLOGY_SERVICE_LIMIT,
  TOPOLOGY_SERVICE_NAME_LIMIT,
  TOPOLOGY_SERVICE_RESULT_MAX_AGE_MS,
  topologyServiceProbeId,
  type TopologyServiceCheck
} from '../server-topology-model'

type ProbeStatus = TopologyTcpProbeResult

function formatCheckTime(timestamp: number) {
  return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(timestamp)
}

export function ServiceStatusSettings({
  nodeId,
  defaultHost,
  services,
  totalServices,
  results,
  onChange
}: {
  nodeId: string
  defaultHost: string
  services: TopologyServiceCheck[]
  totalServices: number
  results: Record<string, ProbeStatus>
  onChange(services: TopologyServiceCheck[]): void
}) {
  const id = useId()
  const [name, setName] = useState('')
  const [host, setHost] = useState(defaultHost)
  const [port, setPort] = useState('')
  const [error, setError] = useState('')

  useEffect(() => {
    setHost(defaultHost)
    setName('')
    setPort('')
    setError('')
  }, [nodeId, defaultHost])

  const addService = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const numericPort = Number(port)
    const normalizedName = name.trim()
    const normalizedHost = host.trim()
    if (
      !normalizedName ||
      normalizedName.length > TOPOLOGY_SERVICE_NAME_LIMIT ||
      !normalizedHost ||
      normalizedHost.length > TOPOLOGY_SERVICE_HOST_LIMIT ||
      /\s/.test(normalizedHost) ||
      !Number.isInteger(numericPort) ||
      numericPort < 1 ||
      numericPort > 65535
    ) {
      setError(t.serverTopologyServiceInvalid)
      return
    }
    if (services.length >= TOPOLOGY_SERVICE_LIMIT) {
      setError(t.serverTopologyServiceLimit)
      return
    }
    if (totalServices >= TOPOLOGY_SERVICE_BOARD_LIMIT) {
      setError(t.serverTopologyServiceTotalLimit)
      return
    }
    setError('')
    onChange([...services, { id: crypto.randomUUID(), name: normalizedName, host: normalizedHost, port: numericPort }])
    setName('')
    setPort('')
  }

  const removeService = (serviceId: string) => onChange(services.filter(({ id: currentId }) => currentId !== serviceId))

  return (
    <TopologySettingsDisclosure
      className="server-topology__services"
      storageKey="fileterm:server-topology:service-settings-expanded"
      title={t.serverTopologyServices}
    >
      {services.length ? (
        <ul>
          {services.map((service) => {
            const result = results[topologyServiceProbeId(nodeId, service.id)]
            const fresh = result && Date.now() - result.checkedAt <= TOPOLOGY_SERVICE_RESULT_MAX_AGE_MS
            const indicatorStatus =
              fresh && result.status === 'reachable'
                ? 'connected'
                : fresh && result.status === 'unreachable'
                  ? 'disconnected'
                  : 'idle'
            const label =
              fresh && result.status === 'reachable'
                ? t.serverTopologyServiceReachable
                : fresh && result.status === 'unreachable'
                  ? t.serverTopologyServiceUnreachable
                  : t.serverTopologyServiceUnknown
            return (
              <li key={service.id}>
                <div className="server-topology__service-identity">
                  <strong>{service.name}</strong>
                  <span>
                    {service.host}:{service.port}
                  </span>
                </div>
                <span className="server-topology__service-state">
                  <StatusIndicator aria-hidden="true" status={indicatorStatus} />
                  {fresh && result.latencyMs !== null ? `${label} · ${result.latencyMs} ms` : label}
                </span>
                {result && result.checkedAt > 0 ? (
                  <small>{t.serverTopologyServiceChecked.replace('{time}', formatCheckTime(result.checkedAt))}</small>
                ) : null}
                <Button
                  aria-label={`${t.delete}: ${service.name}`}
                  onClick={() => removeService(service.id)}
                  size="sm"
                  variant="ghost"
                >
                  {t.delete}
                </Button>
              </li>
            )
          })}
        </ul>
      ) : null}
      <p className="server-topology__hint">{t.serverTopologyServiceProbeHint}</p>
      <form className="server-topology__service-form" onSubmit={addService}>
        <label htmlFor={`${id}-name`}>{t.serverTopologyServiceName}</label>
        <Input
          id={`${id}-name`}
          maxLength={TOPOLOGY_SERVICE_NAME_LIMIT}
          onChange={(event) => setName(event.target.value)}
          value={name}
        />
        <label htmlFor={`${id}-host`}>{t.serverTopologyServiceHost}</label>
        <Input
          id={`${id}-host`}
          maxLength={TOPOLOGY_SERVICE_HOST_LIMIT}
          onChange={(event) => setHost(event.target.value)}
          value={host}
        />
        <label htmlFor={`${id}-port`}>{t.serverTopologyServicePort}</label>
        <Input id={`${id}-port`} inputMode="numeric" onChange={(event) => setPort(event.target.value)} value={port} />
        {error ? (
          <p className="server-topology__error" role="alert">
            {error}
          </p>
        ) : null}
        <Button disabled={!name.trim() || !host.trim() || !port} size="panel" type="submit">
          {t.serverTopologyServiceAdd}
        </Button>
      </form>
    </TopologySettingsDisclosure>
  )
}
