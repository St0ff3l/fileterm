import { Profiler, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { TerminalContextMenu } from '../../../src/renderer/components/terminal-context-menu'
import '../../../src/renderer/styles/index.css'

const fixture = window as unknown as {
  commits: number
  closeRevision: number | null
  update(): void
  open(x: number, y: number): void
}
fixture.commits = 0
fixture.closeRevision = null
function Fixture() {
  const [revision, setRevision] = useState(0)
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null)
  fixture.update = () => flushSync(() => setRevision((current) => current + 1))
  fixture.open = (x, y) => setPosition({ x, y })
  return (
    <>
      <textarea aria-label="terminal" defaultValue="terminal" />
      {position ? (
        <Profiler id="menu" onRender={() => fixture.commits++}>
          <TerminalContextMenu
            position={{ ...position }}
            hasSelection={false}
            shortcuts={{ copy: '', paste: '', find: '', vertical: '', horizontal: '', closePane: '' }}
            hasSplitPane={true}
            hasCloseablePane={true}
            setContextMenu={(next) => {
              fixture.closeRevision = revision
              setPosition(next)
            }}
            runCopy={() => {}}
            runPaste={async () => {}}
            runFind={() => {}}
            runSaveSessionLog={async () => {}}
            runClear={() => {}}
            runSplitPane={() => {}}
            runClosePane={() => {}}
          />
        </Profiler>
      ) : null}
    </>
  )
}
createRoot(document.getElementById('root')!).render(<Fixture />)
