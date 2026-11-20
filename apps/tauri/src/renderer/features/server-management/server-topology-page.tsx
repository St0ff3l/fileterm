import { useId, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'
import type { ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../../components/common/confirm-action-dialog/confirm-action-dialog'
import { DropdownSelect } from '../../components/common/dropdown-select/dropdown-select'
import { Input, Textarea } from '../../components/common/input/input'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { t } from '../../i18n'
import { StatusIndicator } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import {
  addTopologyNode,
  decodeTopologyBoard,
  moveTopologyNode,
  removeTopologyNode,
  TOPOLOGY_LINK_NAME_LIMIT,
  TOPOLOGY_MEMO_LIMIT,
  TOPOLOGY_NODE_HEIGHT as NODE_HEIGHT,
  TOPOLOGY_NODE_WIDTH as NODE_WIDTH,
  TOPOLOGY_STORAGE_KEY,
  topologyLinkMatches,
  updateTopologyNodeMemo,
  type TopologyBoard,
  type TopologyLink,
  type TopologyNode
} from './server-topology-model'
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
        placeholder={t.serverTopologyMemoPlaceholder}
        rows={5}
        value={draft}
      />
      <Button disabled={draft === memo} size="panel" type="submit" variant="primary">
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
  ends: { lines: NodeListOf<SVGLineElement>; label: SVGTextElement | null; x: number; y: number; end: 1 | 2 }[]
}

function previewNode(button: HTMLButtonElement, node: TopologyNode, drag: NodeDrag, dx: number, dy: number) {
  button.style.transform = dx || dy ? `translate3d(${dx}px, ${dy}px, 0)` : ''
  for (const { lines, label, x, y, end } of drag.ends) {
    for (const line of lines) {
      line.setAttribute(`x${end}`, String(node.x + dx + NODE_WIDTH / 2))
      line.setAttribute(`y${end}`, String(node.y + dy + NODE_HEIGHT / 2))
    }
    label?.setAttribute('x', String(x + dx / 2))
    label?.setAttribute('y', String(y + dy / 2))
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
  const [selectedLinkKey, setSelectedLinkKey] = useState('')
  const [linkName, setLinkName] = useState('')
  const [linkTo, setLinkTo] = useState('')
  const [placing, setPlacing] = useState(false)
  const [pendingRemoval, setPendingRemoval] = useState<'node' | 'link' | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const detailsScrollRef = useRef<HTMLDivElement>(null)
  const linksRef = useRef<SVGSVGElement>(null)
  const dragRef = useRef<NodeDrag | null>(null)

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

  if (!board) {
    return (
      <section className="server-topology">
        <h1>{t.serverTopology}</h1>
        <p role="alert">{t.serverTopologyLoadError}</p>
      </section>
    )
  }

  const profilesById = new Map(profiles.map((profile) => [profile.id, profile]))
  const nodesById = new Map(board.nodes.map((node) => [node.id, node]))
  const available = profiles.filter((profile) => !nodesById.has(profile.id))
  const selected = nodesById.get(selectedId)
  const selectedProfile = profilesById.get(selectedId)
  const statuses = new Map(board.nodes.map(({ id }) => [id, statusForProfile(id, workspaceTabs, sessions)]))
  const selectedConnection = statuses.get(selectedId)
  const selectedSystem = selectedConnection && sessions[selectedConnection.tab.id]?.systemMetrics?.identity.osName
  const relatedIds = (id: string) =>
    board.links.filter(([first, second]) => first === id || second === id).map(([a, b]) => (a === id ? b : a))
  const selectedRelations = relatedIds(selectedId)
  const protocol = (profile: ConnectionProfile) =>
    profile.type === 'ftp' && profile.secure ? 'FTPS' : profile.type.toUpperCase()
  const selectedTarget = nodesById.has(linkTo) && linkTo !== selectedId ? linkTo : ''
  const linked = board.links.find((link) => topologyLinkMatches(link, selectedId, selectedTarget))
  const selectedLink = board.links.find((link) => linkKey(link) === selectedLinkKey)
  const linkToRemove = selectedLink ?? linked
  const nodeName = (id: string) => profilesById.get(id)?.name ?? t.serverTopologyMissingProfile
  const linkEndpoints = ([first, second]: TopologyLink) => `${nodeName(first)} ↔ ${nodeName(second)}`
  const minWidth = Math.max(320, ...board.nodes.map((node) => node.x + NODE_WIDTH + 40))
  const minHeight = Math.max(200, ...board.nodes.map((node) => node.y + NODE_HEIGHT + 40))

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

  const addNode = (profileId: string) => {
    if (!available.some((profile) => profile.id === profileId)) return
    const next = addTopologyNode(board, profileId)
    if (next !== board && save(next)) selectNode(profileId)
  }

  const moveNode = (node: TopologyNode, x: number, y: number) => {
    const next = moveTopologyNode(board, node.id, Math.max(0, x), Math.max(0, y))
    if (next !== board && save(next)) setPlacing(false)
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
      const end = group.dataset.first === node.id ? 1 : group.dataset.second === node.id ? 2 : null
      if (!end) return []
      const label = group.querySelector('text')
      return [
        {
          lines: group.querySelectorAll('line'),
          label,
          x: Number(label?.getAttribute('x')),
          y: Number(label?.getAttribute('y')),
          end
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
        <DropdownSelect
          ariaLabel={t.serverTopologyAddNode}
          className="server-topology__select"
          disabled={!available.length}
          onChange={addNode}
          options={[
            { value: '', label: t.serverTopologyAddNode, disabled: true },
            ...available.map((profile) => ({ value: profile.id, label: profile.name }))
          ]}
          value=""
        />
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
                {board.links.map((link) => {
                  const [first, second, name] = link
                  const a = nodesById.get(first)!
                  const b = nodesById.get(second)!
                  const key = linkKey(link)
                  const isSelected = selectedLinkKey === key
                  const endpoints = {
                    x1: a.x + NODE_WIDTH / 2,
                    x2: b.x + NODE_WIDTH / 2,
                    y1: a.y + NODE_HEIGHT / 2,
                    y2: b.y + NODE_HEIGHT / 2
                  }
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
              {board.nodes.map((node) => {
                const profile = profilesById.get(node.id)
                const details = profile ? connectionProfileSummary(profile) : t.serverTopologyMissingProfile
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
                      <span>{profile ? protocol(profile) : t.serverTopologyMissingProfile}</span>
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
                    {profile ? <small>{details}</small> : null}
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
                    <Button
                      disabled={linkName.trim() === (selectedLink[2] ?? '')}
                      size="panel"
                      type="submit"
                      variant="primary"
                    >
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
                <div className="server-topology__actions">
                  <Button
                    size="panel"
                    variant="primary"
                    disabled={!selectedProfile}
                    onClick={() => onOpenProfile(selectedId)}
                  >
                    {t.openConnection}
                  </Button>
                  <Button
                    size="panel"
                    title={t.serverTopologyMoveHint}
                    aria-pressed={placing}
                    onClick={() => setPlacing(!placing)}
                  >
                    {t.serverTopologyMove}
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
                  <div className="server-topology__actions server-topology__link-actions">
                    <DropdownSelect
                      ariaLabel={t.serverTopologyPickNode}
                      className="server-topology__select"
                      disabled={board.nodes.length < 2}
                      onChange={setLinkTo}
                      options={[
                        { value: '', label: t.serverTopologyPickNode, disabled: true },
                        ...board.nodes
                          .filter((node) => node.id !== selectedId)
                          .map((node) => ({ value: node.id, label: nodeName(node.id) }))
                      ]}
                      value={selectedTarget}
                    />
                    <Button
                      disabled={!selectedTarget}
                      onClick={() => {
                        if (linked) setPendingRemoval('link')
                        else save({ ...board, links: [...board.links, [selectedId, selectedTarget]] })
                      }}
                    >
                      {linked ? t.serverTopologyRemoveLink : t.serverTopologyAddLink}
                    </Button>
                  </div>
                </section>
                <NodeMemo
                  key={selected.id}
                  memo={selected.memo ?? ''}
                  onSave={(memo) => save(updateTopologyNodeMemo(board, selected.id, memo))}
                />
                <Button size="panel" variant="ghost" onClick={() => setPendingRemoval('node')}>
                  {t.serverTopologyRemoveNode}
                </Button>
              </>
            ) : (
              <p className="server-topology__hint">
                {board.nodes.length ? t.serverTopologySelectNode : t.serverTopologyEmpty}
              </p>
            )}
            {saveFailed ? (
              <p className="server-topology__error" role="alert">
                {t.serverTopologySaveError}
              </p>
            ) : null}
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
