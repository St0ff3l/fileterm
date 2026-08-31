import { memo } from 'react'
import '@xterm/xterm/css/xterm.css'
import { VerticalScrollbar } from '../features/common/vertical-scrollbar'
import { SerialToolbar } from '../features/serial/serial-toolbar'
import { TerminalContextMenu } from './terminal-context-menu'
import { TerminalFindBar } from './terminal-find-bar'
import { useTerminalView, type TerminalViewProps } from './use-terminal-view'

export const TerminalView = memo(function TerminalView(props: TerminalViewProps) {
  const {
    profileId,
    tabId,
    sessionType,
    connected = false,
    onActivate,
    onSplitPane,
    onClosePane,
    canClosePane = false
  } = props
  const {
    hostRef,
    terminalScrollController,
    findInputRef,
    hasSelection,
    contextMenu,
    findOpen,
    findQuery,
    findMiss,
    findMatchCount,
    activeFindIndex,
    findCaseSensitive,
    findRegex,
    shortcuts,
    setContextMenu,
    setFindQuery,
    setFindMiss,
    setActiveFindIndex,
    setFindCaseSensitive,
    setFindRegex,
    closeFind,
    searchTerminal,
    runCopy,
    runPaste,
    runFind,
    runSaveSessionLog,
    runClear,
    runSplitPane,
    runClosePane
  } = useTerminalView(props)

  return (
    <div
      className={`terminal-view ${sessionType === 'serial' ? 'terminal-view--serial' : ''}`}
      onFocusCapture={onActivate}
      onMouseDown={onActivate}
    >
      {sessionType === 'serial' ? <SerialToolbar connected={connected} profileId={profileId} tabId={tabId} /> : null}
      <div className="terminal-host">
        <div className="terminal-inner" ref={hostRef} />
      </div>
      <VerticalScrollbar scrollController={terminalScrollController} />
      {findOpen ? (
        <TerminalFindBar
          findInputRef={findInputRef}
          findQuery={findQuery}
          setFindQuery={setFindQuery}
          setFindMiss={setFindMiss}
          setActiveFindIndex={setActiveFindIndex}
          findMiss={findMiss}
          findMatchCount={findMatchCount}
          activeFindIndex={activeFindIndex}
          findCaseSensitive={findCaseSensitive}
          setFindCaseSensitive={setFindCaseSensitive}
          findRegex={findRegex}
          setFindRegex={setFindRegex}
          closeFind={closeFind}
          searchTerminal={searchTerminal}
        />
      ) : null}
      {contextMenu ? (
        <TerminalContextMenu
          position={contextMenu}
          hasSelection={hasSelection}
          shortcuts={shortcuts}
          hasSplitPane={Boolean(onSplitPane)}
          hasCloseablePane={Boolean(onClosePane && canClosePane)}
          setContextMenu={setContextMenu}
          runCopy={runCopy}
          runPaste={runPaste}
          runFind={runFind}
          runSaveSessionLog={runSaveSessionLog}
          runClear={runClear}
          runSplitPane={runSplitPane}
          runClosePane={runClosePane}
        />
      ) : null}
    </div>
  )
})
