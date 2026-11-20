import { useId, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'
import type { ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../../components/common/confirm-action-dialog/confirm-action-dialog'
import { DropdownSelect } from '../../components/common/dropdown-select/dropdown-select'
import { Input, Textarea } from '../../components/common/input/input'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { t } from '../../i18n'
import { useProxyLibrary } from '../../hooks/use-proxy-library'
import { StatusIndicator } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import {
  addTopologyNode,
  addTopologyNodes,
  decodeTopologyBoard,
  moveTopologyNode,
  removeTopologyNode,
  TOPOLOGY_LINK_NAME_LIMIT,
  TOPOLOGY_MEMO_LIMIT,
  TOPOLOGY_NODE_HEIGHT as NODE_HEIGHT,
  TOPOLOGY_NODE_WIDTH as NODE_WIDTH,
  TOPOLOGY_STORAGE_KEY,
  topologyLinkEndpoints,
  topologyLinkMatches,
  updateTopologyNodeMemo,
  type TopologyBoard,
  type TopologyLink,
  type TopologyNode
} from './server-topology-model'
import { configuredTopologyRoutes, isTopologyProxyNode, topologyProxyAddress } from './server-topology-routes'
import { statusForProfile, statusLabel } from './server-profile-status'
import './server-management-page-header.css'
import './server-topology-page.css'

const KEYBOARD_STEP = 12
const DRAG_THRESHOLD = 3

const linkKey = ([first, second]: TopologyLink) => JSON.stringify([first, second])

function NodeMemo({ memo, onSave }: { memo: string; onSave(value: string): void }) {
  const id = useId()
  const [draft, setDraft] = useState(memo)

  return (
    <form
      className="server-topology__memo"
      onSubmit={(event) => {
        event.preventDefault()
        if (draft !== memo) onSave(draft)
      }}
    >
      <label htmlFor={id}>{t.serverTopologyMemo}</label>
      <Textarea
        id={id}
        maxLength={TOPOLOGY_MEMO_LIMIT}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
            event.preventDefault()
            if (draft !== memo) onSave(draft)
          }
        }}
        placeholder={t.serverTopologyMemoPlaceholder}
        rows={5}
        value={draft}
      />
      <Button disabled={draft === memo} size="panel" type="submit">
        {t.save}
      </Button>
    </form>
  )
}

type NodeDrag = {
  pointerId: number
  clientX: number
  clientY: number
  scrollLeft: number
  scrollTop: number
  moved: boolean
  ends: {
    lines: NodeListOf<SVGLineElement>
    label: SVGTextElement | null
    first: TopologyNode
    second: TopologyNode
    directed: boolean
  }[]
}

function previewNode(button: HTMLButtonElement, node: TopologyNode, drag: NodeDrag, dx: number, dy: number) {
  button.style.transform = dx || dy ? `translate3d(${dx}px, ${dy}px, 0)` : ''
  const moved = { ...node, x: node.x + dx, y: node.y + dy }
  for (const { lines, label, first, second, directed } of drag.ends) {
    const endpoints = topologyLinkEndpoints(
      first.id === node.id ? moved : first,
      second.id === node.id ? moved : second,
      directed
    )
    for (const line of lines) {
      for (const [key, value] of Object.entries(endpoints)) line.setAttribute(key, String(value))
    }
    label?.setAttribute('x', String((endpoints.x1 + endpoints.x2) / 2))
    label?.setAttribute('y', String((endpoints.y1 + endpoints.y2) / 2))
  }
}

function loadBoard() {
  try {
    return decodeTopologyBoard(localStorage.getItem(TOPOLOGY_STORAGE_KEY))
  } catch {
    return null
  }
}

export function ServerTopologyPage({
  profiles,
  sessions,
  workspaceTabs,
  onOpenProfile
}: {
  profiles: ConnectionProfile[]
  sessions: Record<string, SessionSnapshot>
  workspaceTabs: WorkspaceTab[]
  onOpenProfile(id: string): void
}) {
  const [board, setBoard] = useState(loadBoard)
  const [saveFailed, setSaveFailed] = useState(false)
  const [selectedId, setSelectedId] = useState('')
  const [nodeCandidateId, setNodeCandidateId] = useState('')
  const [selectedLinkKey, setSelectedLinkKey] = useState('')
  const [linkName, setLinkName] = useState('')
  const [linkTo, setLinkTo] = useState('')
  const [placing, setPlacing] = useState(false)
  const [showRoutes, setShowRoutes] = useState(true)
  const [pendingRemoval, setPendingRemoval] = useState<'node' | 'link' | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const detailsScrollRef = useRef<HTMLDivElement>(null)
  const linksRef = useRef<SVGSVGElement>(null)
  const dragRef = useRef<NodeDrag | null>(null)
  const arrowId = useId()
  const { proxies, error: proxyError } = useProxyLibrary()
  const configured = useMemo(() => configuredTopologyRoutes(profiles, proxies), [profiles, proxies])
  const routeLinks = showRoutes ? configured.links : []
  const routeNodeIds = new Set(routeLinks.flatMap(({ from, to }) => [from, to]))
  const canvas = useMemo(() => {
    if (!board) return null
    const links = showRoutes ? configured.links : []
    const ids = new Set(links.flatMap(({ from, to }) => [from, to]))
    return addTopologyNodes(
      {
        nodes: board.nodes.filter(({ id, routeOnly }) => ids.has(id) || (!routeOnly && !isTopologyProxyNode(id))),
        links: board.links
      },
      ids
    )
  }, [board, configured, showRoutes])

  const save = (next: TopologyBoard) => {
    try {
      localStorage.setItem(TOPOLOGY_STORAGE_KEY, JSON.stringify(next))
      setBoard(next)
      setSaveFailed(false)
      return true
    } catch {
      setSaveFailed(true)
      return false
    }
  }

  if (!board || !canvas) {
    return (
      <section className="server-topology">
        <h1>{t.serverTopology}</h1>
        <p role="alert">{t.serverTopologyLoadError}</p>
      </section>
    )
  }

  const profilesById = new Map(profiles.map((profile) => [profile.id, profile]))
  const nodesById = new Map(canvas.nodes.map((node) => [node.id, node]))
  const visibleRouteLinks = routeLinks.filter(({ from, to }) => nodesById.has(from) && nodesById.has(to))
  const manualLinks = board.links.filter(([first, second]) => nodesById.has(first) && nodesById.has(second))
  const available = profiles.filter((profile) => !nodesById.has(profile.id))
  const nodeCandidate = available.find((profile) => profile.id === nodeCandidateId)
  const selected = nodesById.get(selectedId)
  const selectedProfile = profilesById.get(selectedId)
  const selectedProxy = configured.proxyNodes.get(selectedId)
  const statuses = new Map(canvas.nodes.map(({ id }) => [id, statusForProfile(id, workspaceTabs, sessions)]))
  const selectedConnection = statuses.get(selectedId)
  const selectedSystem = selectedConnection && sessions[selectedConnection.tab.id]?.systemMetrics?.identity.osName
  const relatedIds = (id: string) => {
    const manual = manualLinks
      .filter(([first, second]) => first === id || second === id)
      .map(([a, b]) => (a === id ? b : a))
    const routes = visibleRouteLinks
      .filter(({ from, to }) => from === id || to === id)
      .map(({ from, to }) => (from === id ? to : from))
    return [...new Set([...manual, ...routes])]
  }
  const selectedRelations = relatedIds(selectedId)
  const protocol = (profile: ConnectionProfile) =>
    profile.type === 'ftp' && profile.secure ? 'FTPS' : profile.type.toUpperCase()
  const selectedTarget = nodesById.has(linkTo) && linkTo !== selectedId ? linkTo : ''
  const linked = board.links.find((link) => topologyLinkMatches(link, selectedId, selectedTarget))
  const selectedLink = board.links.find((link) => linkKey(link) === selectedLinkKey)
  const linkToRemove = selectedLink ?? linked
  const nodeName = (id: string) =>
    profilesById.get(id)?.name ?? configured.proxyNodes.get(id)?.name ?? t.serverTopologyMissingProfile
  const linkEndpoints = ([first, second]: TopologyLink) => `${nodeName(first)} ↔ ${nodeName(second)}`
  const minWidth = Math.max(320, ...canvas.nodes.map((node) => node.x + NODE_WIDTH + 40))
  const minHeight = Math.max(200, ...canvas.nodes.map((node) => node.y + NODE_HEIGHT + 40))

  const selectNode = (id: string) => {
    if (id !== selectedId || selectedLinkKey) {
      setSelectedId(id)
      setSelectedLinkKey('')
      setLinkTo('')
      setPlacing(false)
      if (detailsScrollRef.current) detailsScrollRef.current.scrollTop = 0
    }
  }

  const selectLink = (link: TopologyLink) => {
    const key = linkKey(link)
    if (key === selectedLinkKey) return
    selectNode('')
    setSelectedLinkKey(key)
    setLinkName(link[2] ?? '')
    if (detailsScrollRef.current) detailsScrollRef.current.scrollTop = 0
  }

  const saveLinkName = () => {
    if (!selectedLink) return
    const [first, second] = selectedLink
    const name = linkName.trim()
    const replacement: TopologyLink = name ? [first, second, name] : [first, second]
    const next = { ...board, links: board.links.map((link) => (link === selectedLink ? replacement : link)) }
    if (save(next)) setLinkName(name)
  }

  const focusRelatedNode = (id: string) => {
    selectNode(id)
    const target = Array.from(scrollRef.current?.querySelectorAll<HTMLButtonElement>('[data-node-id]') ?? []).find(
      (button) => button.dataset.nodeId === id
    )
    target?.focus()
  }

  const storeLayout = () => {
    const storedIds = new Set(board.nodes.map(({ id }) => id))
    const added = canvas.nodes.filter(({ id }) => !storedIds.has(id))
    return added.length
      ? { ...board, nodes: [...board.nodes, ...added.map((node) => ({ ...node, routeOnly: true as const }))] }
      : board
  }

  const pinNodes = (stored: TopologyBoard, ids: string[]) => ({
    ...stored,
    nodes: stored.nodes.map((node) => (ids.includes(node.id) ? { ...node, routeOnly: undefined } : node))
  })

  const addNode = () => {
    if (!nodeCandidate) return
    const profileId = nodeCandidate.id
    const stored = storeLayout()
    const next = stored.nodes.some(({ id }) => id === profileId)
      ? pinNodes(stored, [profileId])
      : addTopologyNode(stored, profileId)
    if (next !== stored && save(next)) {
      setNodeCandidateId('')
      selectNode(profileId)
    }
  }

  const moveNode = (node: TopologyNode, x: number, y: number) => {
    const stored = storeLayout()
    const next = moveTopologyNode(stored, node.id, Math.max(0, x), Math.max(0, y))
    if (next !== stored && save(next)) setPlacing(false)
  }

  const handleNodeKeys = (event: KeyboardEvent<HTMLButtonElement>, node: TopologyNode) => {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
    const offset = {
      ArrowLeft: [-KEYBOARD_STEP, 0],
      ArrowRight: [KEYBOARD_STEP, 0],
      ArrowUp: [0, -KEYBOARD_STEP],
      ArrowDown: [0, KEYBOARD_STEP]
    }[event.key]
    if (!offset) return
    event.preventDefault()
    selectNode(node.id)
    moveNode(node, node.x + offset[0], node.y + offset[1])
  }

  const startDrag = (event: PointerEvent<HTMLButtonElement>, node: TopologyNode) => {
    if (event.button !== 0 || !event.isPrimary || dragRef.current) return
    const ends = Array.from(linksRef.current?.querySelectorAll<SVGGElement>('g[data-first]') ?? []).flatMap<
      NodeDrag['ends'][number]
    >((group) => {
      const first = nodesById.get(group.dataset.first ?? '')
      const second = nodesById.get(group.dataset.second ?? '')
      if (!first || !second || (first.id !== node.id && second.id !== node.id)) return []
      return [
        {
          lines: group.querySelectorAll('line'),
          label: group.querySelector('text'),
          first,
          second,
          directed: group.dataset.configured !== undefined
        }
      ]
    })
    event.currentTarget.setPointerCapture(event.pointerId)
    dragRef.current = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      scrollLeft: scrollRef.current?.scrollLeft ?? 0,
      scrollTop: scrollRef.current?.scrollTop ?? 0,
      moved: false,
      ends
    }
  }

  const dragPosition = (event: PointerEvent<HTMLButtonElement>, drag: NodeDrag, node: TopologyNode) => {
    const dx = event.clientX - drag.clientX + (scrollRef.current?.scrollLeft ?? 0) - drag.scrollLeft
    const dy = event.clientY - drag.clientY + (scrollRef.current?.scrollTop ?? 0) - drag.scrollTop
    drag.moved ||= Math.hypot(dx, dy) >= DRAG_THRESHOLD
    return { dx: Math.max(-node.x, dx), dy: Math.max(-node.y, dy) }
  }

  const updateDrag = (event: PointerEvent<HTMLButtonElement>, node: TopologyNode) => {
    const drag = dragRef.current
    if (drag?.pointerId !== event.pointerId) return
    const { dx, dy } = dragPosition(event, drag, node)
    if (drag.moved) previewNode(event.currentTarget, node, drag, dx, dy)
  }

  const endDrag = (event: PointerEvent<HTMLButtonElement>, node: TopologyNode, commit: boolean) => {
    const drag = dragRef.current
    if (drag?.pointerId !== event.pointerId) return
    dragRef.current = null
    previewNode(event.currentTarget, node, drag, 0, 0)
    if (!commit) return
    const { dx, dy } = dragPosition(event, drag, node)
    if (drag.moved) {
      selectNode(node.id)
      moveNode(node, node.x + dx, node.y + dy)
    }
  }

  const confirmRemoval = () => {
    const removingNode = pendingRemoval === 'node'
    const next = removingNode
      ? removeTopologyNode(board, selectedId)
      : { ...board, links: board.links.filter((link) => link !== linkToRemove) }
    if (save(next)) {
      if (removingNode || selectedLink) selectNode('')
      setPendingRemoval(null)
    }
  }

  return (
    <section
      className="server-topology"
      onKeyDown={(event) => {
        if (event.key !== 'Escape' || (!selected && !selectedLink) || pendingRemoval || event.defaultPrevented) return
        if (!(event.target instanceof Node) || !event.currentTarget.contains(event.target)) return
        event.preventDefault()
        if (placing) setPlacing(false)
        else selectNode('')
      }}
    >
      <header className="server-management-page-header">
        <h1>
          <MaterialIcon name="edit_note" />
          <span>{t.serverTopology}</span>
        </h1>
        <div className="server-topology__actions">
          <Button
            aria-pressed={showRoutes}
            onClick={() => {
              selectNode('')
              setShowRoutes(!showRoutes)
            }}
            size="panel"
            title={t.serverTopologyConfiguredRoutesHint}
          >
            {t.serverTopologyConfiguredRoutes}
          </Button>
          <div className="server-topology__add-node">
            <DropdownSelect
              ariaLabel={t.serverTopologySelectConnection}
              className="server-topology__select"
              disabled={!available.length}
              menuClassName="server-topology__add-node-menu"
              onChange={setNodeCandidateId}
              options={[
                { value: '', label: t.serverTopologySelectConnection, disabled: true },
                ...available.map((profile) => ({ value: profile.id, label: profile.name }))
              ]}
              value={nodeCandidate?.id ?? ''}
            />
            <Button
              aria-label={t.serverTopologyAddNode}
              disabled={!nodeCandidate}
              icon={<MaterialIcon name="add" />}
              onClick={addNode}
              size="panel"
              title={t.serverTopologyAddNode}
            />
          </div>
        </div>
      </header>
      <div className="server-topology__body">
        <div className="server-topology__viewport">
          <div className="server-topology__scroll" ref={scrollRef}>
            <div
              className="server-topology__canvas"
              data-placing={placing}
              style={{ minWidth, minHeight }}
              onClick={(event) => {
                if (event.target !== event.currentTarget) return
                if (!placing || !selected) {
                  selectNode('')
                  return
                }
                const bounds = event.currentTarget.getBoundingClientRect()
                moveNode(
                  selected,
                  event.clientX - bounds.left - NODE_WIDTH / 2,
                  event.clientY - bounds.top - NODE_HEIGHT / 2
                )
              }}
            >
              <svg className="server-topology__links" ref={linksRef}>
                <defs>
                  <marker
                    id={arrowId}
                    markerHeight="8"
                    markerWidth="8"
                    markerUnits="userSpaceOnUse"
                    orient="auto"
                    refX="8"
                    refY="4"
                  >
                    <path className="server-topology__route-arrow" d="M0 0 L8 4 L0 8 Z" />
                  </marker>
                </defs>
                {visibleRouteLinks.map(({ from, to, kind }) => (
                  <g
                    aria-hidden="true"
                    className="server-topology__route"
                    data-configured={kind}
                    data-first={from}
                    data-second={to}
                    key={JSON.stringify([from, to, kind])}
                  >
                    <line
                      className="server-topology__route-line"
                      markerEnd={`url(#${arrowId})`}
                      {...topologyLinkEndpoints(nodesById.get(from)!, nodesById.get(to)!, true)}
                    />
                  </g>
                ))}
                {manualLinks.map((link) => {
                  const [first, second, name] = link
                  const a = nodesById.get(first)!
                  const b = nodesById.get(second)!
                  const key = linkKey(link)
                  const isSelected = selectedLinkKey === key
                  const endpoints = topologyLinkEndpoints(a, b, false)
                  return (
                    <g
                      aria-label={name ? `${name} · ${linkEndpoints(link)}` : linkEndpoints(link)}
                      aria-pressed={isSelected}
                      className={
                        isSelected || first === selectedId || second === selectedId ? 'is-selected' : undefined
                      }
                      data-first={first}
                      data-second={second}
                      key={key}
                      onClick={() => selectLink(link)}
                      onKeyDown={(event) => {
                        if (event.key !== 'Enter' && event.key !== ' ') return
                        event.preventDefault()
                        selectLink(link)
                      }}
                      role="button"
                      tabIndex={0}
                    >
                      <line aria-hidden="true" className="server-topology__link-hit" {...endpoints} />
                      <line aria-hidden="true" className="server-topology__link-line" {...endpoints} />
                      {name ? (
                        <text
                          aria-hidden="true"
                          dominantBaseline="central"
                          textAnchor="middle"
                          x={(endpoints.x1 + endpoints.x2) / 2}
                          y={(endpoints.y1 + endpoints.y2) / 2}
                        >
                          {name}
                        </text>
                      ) : null}
                    </g>
                  )
                })}
              </svg>
              {canvas.nodes.map((node) => {
                const profile = profilesById.get(node.id)
                const proxy = configured.proxyNodes.get(node.id)
                const details = profile
                  ? connectionProfileSummary(profile)
                  : proxy
                    ? topologyProxyAddress(proxy)
                    : t.serverTopologyMissingProfile
                const related = relatedIds(node.id).map(nodeName)
                const status = statuses.get(node.id)?.status ?? 'idle'
                return (
                  <button
                    aria-description={
                      related.length ? t.serverTopologyLinkedTo.replace('{nodes}', related.join(', ')) : undefined
                    }
                    aria-pressed={node.id === selectedId}
                    className="server-topology__node"
                    data-node-id={node.id}
                    key={node.id}
                    onClick={() => selectNode(node.id)}
                    onKeyDown={(event) => handleNodeKeys(event, node)}
                    onPointerDown={(event) => startDrag(event, node)}
                    onPointerMove={(event) => updateDrag(event, node)}
                    onLostPointerCapture={(event) => endDrag(event, node, false)}
                    onPointerUp={(event) => endDrag(event, node, true)}
                    style={{ left: node.x, top: node.y, width: NODE_WIDTH, height: NODE_HEIGHT }}
                    title={`${nodeName(node.id)} · ${details}`}
                    type="button"
                  >
                    <span className="server-topology__node-meta">
                      <MaterialIcon name={profile?.type === 'serial' ? 'usb' : 'dns'} />
                      <span>
                        {profile ? protocol(profile) : (proxy?.type.toUpperCase() ?? t.serverTopologyMissingProfile)}
                      </span>
                      {profile ? (
                        <StatusIndicator
                          role="img"
                          aria-label={statusLabel(status)}
                          title={statusLabel(status)}
                          status={status}
                        />
                      ) : null}
                    </span>
                    <strong>{nodeName(node.id)}</strong>
                    {profile || proxy ? <small>{details}</small> : null}
                  </button>
                )
              })}
            </div>
          </div>
          <VerticalScrollbar ariaLabel={t.serverTopology} scrollRef={scrollRef} />
        </div>
        <aside
          className="server-topology__details"
          aria-label={selectedLink ? t.serverTopologyLinkDetails : t.serverDashboardDetails}
        >
          <div className="server-topology__details-scroll" ref={detailsScrollRef}>
            <h2>{selectedLink ? t.serverTopologyLinkDetails : t.serverDashboardDetails}</h2>
            {selectedLink ? (
              <>
                <div className="server-topology__identity">
                  <strong>{linkEndpoints(selectedLink)}</strong>
                </div>
                <form
                  className="server-topology__link-editor"
                  onSubmit={(event) => {
                    event.preventDefault()
                    saveLinkName()
                  }}
                >
                  <label htmlFor="server-topology-link-name">{t.serverTopologyLinkName}</label>
                  <Input
                    id="server-topology-link-name"
                    maxLength={TOPOLOGY_LINK_NAME_LIMIT}
                    onChange={(event) => setLinkName(event.target.value)}
                    value={linkName}
                  />
                  <div className="server-topology__actions">
                    <Button disabled={linkName.trim() === (selectedLink[2] ?? '')} size="panel" type="submit">
                      {t.save}
                    </Button>
                    <Button onClick={() => setPendingRemoval('link')} size="panel" variant="ghost">
                      {t.serverTopologyRemoveLink}
                    </Button>
                  </div>
                </form>
              </>
            ) : selected ? (
              <>
                <div className="server-topology__identity">
                  <strong>{nodeName(selectedId)}</strong>
                  {selectedProfile ? (
                    <span className="server-topology__connection-status">
                      <StatusIndicator aria-hidden="true" status={selectedConnection?.status ?? 'idle'} />
                      {statusLabel(selectedConnection?.status ?? 'idle')}
                    </span>
                  ) : selectedProxy ? (
                    <span className="server-topology__connection-status">
                      {selectedProxy.type.toUpperCase()} · {topologyProxyAddress(selectedProxy)}
                    </span>
                  ) : null}
                </div>
                {selectedProfile ? (
                  <dl className="server-topology__facts">
                    <div>
                      <dt>{t.connectionType}</dt>
                      <dd>{protocol(selectedProfile)}</dd>
                    </div>
                    {selectedProfile.type === 'serial' ? (
                      <>
                        <div>
                          <dt>{t.devicePath}</dt>
                          <dd>{selectedProfile.devicePath}</dd>
                        </div>
                        <div>
                          <dt>{t.baudRate}</dt>
                          <dd>{selectedProfile.baudRate}</dd>
                        </div>
                      </>
                    ) : (
                      <>
                        <div>
                          <dt>{t.host}</dt>
                          <dd>{selectedProfile.host}</dd>
                        </div>
                        <div>
                          <dt>{t.port}</dt>
                          <dd>{selectedProfile.port}</dd>
                        </div>
                        {selectedProfile.username ? (
                          <div>
                            <dt>{t.username}</dt>
                            <dd>{selectedProfile.username}</dd>
                          </div>
                        ) : null}
                      </>
                    )}
                    {selectedSystem ? (
                      <div>
                        <dt>{t.serverInspectorSystem}</dt>
                        <dd>{selectedSystem}</dd>
                      </div>
                    ) : null}
                    {'note' in selectedProfile && selectedProfile.note?.trim() ? (
                      <div className="server-topology__note">
                        <dt>{t.note}</dt>
                        <dd>{selectedProfile.note}</dd>
                      </div>
                    ) : null}
                  </dl>
                ) : null}
                <div className="server-topology__node-actions">
                  {!selectedProxy && (
                    <Button
                      size="panel"
                      icon={<MaterialIcon name="terminal" />}
                      disabled={!selectedProfile}
                      onClick={() => onOpenProfile(selectedId)}
                      title={t.openConnection}
                    >
                      <span>{t.openConnection}</span>
                    </Button>
                  )}
                  <Button
                    size="panel"
                    icon={<MaterialIcon name="drag_indicator" />}
                    title={t.serverTopologyMoveHint}
                    aria-pressed={placing}
                    onClick={() => setPlacing(!placing)}
                  >
                    <span>{t.serverTopologyMove}</span>
                  </Button>
                </div>
                {placing ? (
                  <p className="server-topology__hint" role="status">
                    {t.serverTopologyPlaceHint}
                  </p>
                ) : null}
                <section className="server-topology__relations">
                  <h3>{t.serverTopologyRelations}</h3>
                  {selectedRelations.length ? (
                    <ul>
                      {selectedRelations.map((id) => (
                        <li key={id}>
                          <Button size="sm" variant="ghost" onClick={() => focusRelatedNode(id)}>
                            {nodeName(id)}
                          </Button>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="server-topology__hint">{t.serverTopologyNoRelations}</p>
                  )}
                  {!selectedProxy && (
                    <div className="server-topology__actions server-topology__link-actions">
                      <DropdownSelect
                        ariaLabel={t.serverTopologyPickNode}
                        className="server-topology__select"
                        disabled={canvas.nodes.filter(({ id }) => !isTopologyProxyNode(id)).length < 2}
                        onChange={setLinkTo}
                        options={[
                          { value: '', label: t.serverTopologyPickNode, disabled: true },
                          ...canvas.nodes
                            .filter((node) => node.id !== selectedId && !isTopologyProxyNode(node.id))
                            .map((node) => ({ value: node.id, label: nodeName(node.id) }))
                        ]}
                        value={selectedTarget}
                      />
                      <Button
                        disabled={!selectedTarget}
                        onClick={() => {
                          if (linked) setPendingRemoval('link')
                          else {
                            const stored = pinNodes(storeLayout(), [selectedId, selectedTarget])
                            save({ ...stored, links: [...stored.links, [selectedId, selectedTarget]] })
                          }
                        }}
                        size="panel"
                      >
                        {linked ? t.serverTopologyRemoveLink : t.serverTopologyAddLink}
                      </Button>
                    </div>
                  )}
                </section>
                <NodeMemo
                  key={selected.id}
                  memo={selected.memo ?? ''}
                  onSave={(memo) => save(updateTopologyNodeMemo(storeLayout(), selected.id, memo))}
                />
                {!routeNodeIds.has(selectedId) && (
                  <Button size="panel" variant="ghost" onClick={() => setPendingRemoval('node')}>
                    {t.serverTopologyRemoveNode}
                  </Button>
                )}
              </>
            ) : (
              <p className="server-topology__hint">
                {canvas.nodes.length ? t.serverTopologySelectNode : t.serverTopologyEmpty}
              </p>
            )}
            {saveFailed ? (
              <p className="server-topology__error" role="alert">
                {t.serverTopologySaveError}
              </p>
            ) : null}
            {showRoutes && proxyError && (
              <p className="server-topology__error" role="alert">
                {t.serverTopologyProxyLoadError}
              </p>
            )}
          </div>
          <VerticalScrollbar ariaLabel={t.serverDashboardDetails} scrollRef={detailsScrollRef} />
        </aside>
      </div>
      {pendingRemoval ? (
        <ConfirmActionDialog
          confirmLabel={pendingRemoval === 'node' ? t.serverTopologyRemoveNode : t.serverTopologyRemoveLink}
          description={
            pendingRemoval === 'node'
              ? t.serverTopologyRemoveNodeDescription.replace('{node}', nodeName(selectedId))
              : linkToRemove && linkEndpoints(linkToRemove)
          }
          errorMessage={saveFailed ? t.serverTopologySaveError : null}
          onClose={() => setPendingRemoval(null)}
          onConfirm={confirmRemoval}
          title={pendingRemoval === 'node' ? t.serverTopologyRemoveNode : t.serverTopologyRemoveLink}
        />
      ) : null}
    </section>
  )
}
