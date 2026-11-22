import { memo } from 'react'
import '@xterm/xterm/css/xterm.css'
import '../xterm-adapters/xterm-compat.css'
import { VerticalScrollbar } from '../features/common/vertical-scrollbar'
import { SerialToolbar } from '../features/serial/serial-toolbar'
import { TerminalCompletionPopup } from '../features/terminal/autocomplete/terminal-completion-popup'
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
    completionPopup,
    acceptCompletion,
    selectCompletion,
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
    setActiveFindIndex,
    setFindCaseSensitive,
    setFindRegex,
    closeFind,
    searchTerminal,
    runCopy,
    runPaste,
    runPasteSelection,
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
      {completionPopup ? (
        <TerminalCompletionPopup popup={completionPopup} onAccept={acceptCompletion} onSelect={selectCompletion} />
      ) : null}
      {findOpen ? (
        <TerminalFindBar
          findInputRef={findInputRef}
          findQuery={findQuery}
          setFindQuery={setFindQuery}
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
          selectedText={contextMenu.selectedText ?? ''}
          hasSelection={hasSelection}
          shortcuts={shortcuts}
          hasSplitPane={Boolean(onSplitPane)}
          hasCloseablePane={Boolean(onClosePane && canClosePane)}
          setContextMenu={setContextMenu}
          runCopy={runCopy}
          runPaste={runPaste}
          runPasteSelection={runPasteSelection}
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
