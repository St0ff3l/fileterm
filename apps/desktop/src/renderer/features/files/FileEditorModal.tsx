import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from 'react'
import Editor, { loader, type Monaco, type OnMount } from '@monaco-editor/react'
import OpenCC from 'opencc-js'
import * as monacoEditor from 'monaco-editor'
import type { FileContentSnapshot } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { EDITOR_ENCODINGS, findEncodingOption, sortEditorLanguages, type EditorLanguageOption } from './file-editor-config'

const toTraditional = OpenCC.Converter({ from: 'cn', to: 'tw' })
const toSimplified = OpenCC.Converter({ from: 'tw', to: 'cn' })

// Monaco loader defaults to a CDN, which gets blocked by Electron CSP and leaves the editor
// stuck on its built-in loading state. Use the bundled npm package instead.
loader.config({ monaco: monacoEditor })

type EditorInstance = Parameters<OnMount>[0]
type EditorMenu = 'file' | 'edit' | 'search' | 'preferences' | 'encoding' | 'language'
const MONACO_DARK_THEME = 'fileterm-default-dark'

function readCssVariable(name: string, fallbackName?: string) {
  const styles = window.getComputedStyle(document.documentElement)
  const value = styles.getPropertyValue(name).trim()
  if (value) {
    return value
  }
  return fallbackName ? styles.getPropertyValue(fallbackName).trim() : ''
}

function defineFileTermMonacoTheme(monaco: Monaco) {
  monaco.editor.defineTheme(MONACO_DARK_THEME, {
    base: 'vs-dark',
    inherit: true,
    rules: [],
    colors: {
      'editor.background': readCssVariable('--monaco-editor-bg', '--terminal-bg'),
      'editor.foreground': readCssVariable('--monaco-editor-foreground', '--terminal-text'),
      'editorLineNumber.foreground': readCssVariable('--monaco-line-number', '--text-soft'),
      'editorLineNumber.activeForeground': readCssVariable('--monaco-line-number-active', '--text-muted'),
      'editorCursor.foreground': readCssVariable('--monaco-cursor', '--accent-primary'),
      'editor.selectionBackground': readCssVariable('--monaco-selection', '--selection-bg'),
      'editor.inactiveSelectionBackground': readCssVariable('--monaco-inactive-selection', '--selection-bg'),
      'editor.lineHighlightBackground': readCssVariable('--monaco-line-highlight', '--surface-inset'),
      'editorIndentGuide.background1': readCssVariable('--monaco-indent-guide', '--border-light'),
      'editorIndentGuide.activeBackground1': readCssVariable('--monaco-indent-guide-active', '--border-dark')
    }
  })
}

export function FileEditorModal({
  errorMessage,
  file,
  isBusy,
  isSaving,
  onClose,
  onReloadWithEncoding,
  onSave,
  standalone = false,
  themeMode
}: {
  errorMessage: string | null
  file: FileContentSnapshot
  isBusy: boolean
  isSaving: boolean
  onClose(): void
  onReloadWithEncoding(encoding: string): void
  onSave(content: string, encoding: string): void
  standalone?: boolean
  themeMode: string
}) {
  const [content, setContent] = useState(file.content)
  const [encoding, setEncoding] = useState(file.encoding ?? 'utf-8')
  const [language, setLanguage] = useState('plaintext')
  const [wordWrap, setWordWrap] = useState(true)
  const [showMinimap, setShowMinimap] = useState(false)
  const [showLineNumbers, setShowLineNumbers] = useState(true)
  const [cursorLine, setCursorLine] = useState(1)
  const [cursorColumn, setCursorColumn] = useState(1)
  const [openMenu, setOpenMenu] = useState<EditorMenu | null>(null)
  const [languages, setLanguages] = useState<EditorLanguageOption[]>([{ id: 'plaintext', label: 'Plain Text' }])

  const editorRef = useRef<EditorInstance | null>(null)
  const monacoRef = useRef<Monaco | null>(null)
  const encodingRef = useRef(encoding)
  const shellRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    setContent(file.content)
    setEncoding(file.encoding ?? 'utf-8')
  }, [file.content, file.encoding, file.path])

  useEffect(() => {
    encodingRef.current = encoding
  }, [encoding])

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null
      if (!target) {
        return
      }

      const clickedInsideMenu = target.closest('[data-file-editor-menu-scope="true"]')
      if (!clickedInsideMenu) {
        setOpenMenu(null)
      }
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && openMenu) {
        setOpenMenu(null)
      }
    }

    window.addEventListener('pointerdown', onPointerDown, true)
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [openMenu])

  const isDirty = content !== file.content || encoding !== (file.encoding ?? 'utf-8')
  const lineCount = useMemo(() => (content.match(/\n/g)?.length ?? 0) + 1, [content])
  const characterCount = content.length
  const currentEncoding = findEncodingOption(encoding)
  const currentLanguage = languages.find((option) => option.id === language)?.label ?? language
  const handleMount: OnMount = (editor, monaco) => {
    editorRef.current = editor
    monacoRef.current = monaco
    setLanguages(sortEditorLanguages(monaco.languages.getLanguages()))

    defineFileTermMonacoTheme(monaco)
    monaco.editor.setTheme(themeMode === 'default-dark' ? MONACO_DARK_THEME : 'vs')
    setLanguage(editor.getModel()?.getLanguageId() ?? 'plaintext')

    const position = editor.getPosition()
    if (position) {
      setCursorLine(position.lineNumber)
      setCursorColumn(position.column)
    }

    editor.onDidChangeCursorPosition((event) => {
      setCursorLine(event.position.lineNumber)
      setCursorColumn(event.position.column)
    })

    editor.onDidChangeModelLanguage(() => {
      setLanguage(editor.getModel()?.getLanguageId() ?? 'plaintext')
    })

    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      onSave(editor.getValue(), encodingRef.current)
    })

    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyF, () => {
      void runEditorAction('actions.find')
    })
  }

  useEffect(() => {
    if (!monacoRef.current) {
      return
    }
    defineFileTermMonacoTheme(monacoRef.current)
    monacoRef.current.editor.setTheme(themeMode === 'default-dark' ? MONACO_DARK_THEME : 'vs')
  }, [themeMode])

  useEffect(() => {
    const editor = editorRef.current
    if (!editor) {
      return
    }
    editor.updateOptions({
      fixedOverflowWidgets: true,
      lineNumbers: showLineNumbers ? 'on' : 'off',
      minimap: { enabled: showMinimap },
      wordWrap: wordWrap ? 'on' : 'off'
    })
  }, [showLineNumbers, showMinimap, wordWrap])

  const runEditorAction = async (actionId: string) => {
    const editor = editorRef.current
    if (!editor) {
      return
    }
    editor.focus()
    await editor.getAction(actionId)?.run()
    setOpenMenu(null)
  }

  const updateLanguage = (nextLanguage: string) => {
    const editor = editorRef.current
    const model = editor?.getModel()
    if (model && editor) {
      monacoRef.current?.editor.setModelLanguage(model, nextLanguage)
    }
    setLanguage(nextLanguage)
    setOpenMenu(null)
  }

  const convertContent = (converter: (text: string) => string) => {
    const editor = editorRef.current
    if (!editor) {
      return
    }

    const model = editor.getModel()
    const selection = editor.getSelection()
    const selectedText = selection ? model?.getValueInRange(selection) ?? '' : ''

    if (selection && !selection.isEmpty() && selectedText) {
      editor.executeEdits('opencc-convert', [{ range: selection, text: converter(selectedText) }])
    } else {
      editor.setValue(converter(editor.getValue()))
    }

    setContent(editor.getValue())
    editor.focus()
    setOpenMenu(null)
  }

  const handleShellClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement
    if (!target.closest('[data-file-editor-menu-scope="true"]')) {
      setOpenMenu(null)
    }
  }

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape' && openMenu) {
      event.stopPropagation()
      setOpenMenu(null)
    }
  }

  const frameClassName = `modal-card file-editor-modal ${themeMode === 'default-dark' ? 'file-editor-modal--dark' : ''} ${standalone ? 'standalone' : ''}`
  const contentNode = (
    <div className={frameClassName} onClick={handleShellClick} onKeyDown={handleKeyDown} ref={shellRef}>
      <div className="modal-header">
        <div className="file-editor-title">
          <span>{file.source === 'remote' ? t.editRemoteFile : t.editLocalFile}</span>
          <strong>{file.name}</strong>
          {isDirty ? <b>{t.fileEditorUnsaved}</b> : null}
        </div>
        <div className="file-editor-header-actions">
          <button
            aria-busy={isSaving}
            className={`file-editor-save-button ${isDirty ? 'is-dirty' : ''} ${isSaving ? 'is-saving' : ''}`}
            disabled={!isDirty || isBusy || isSaving}
            onClick={() => onSave(content, encoding)}
            type="button"
          >
            {isSaving ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{isSaving ? t.saving : t.save}</span>
          </button>
          <button aria-label={t.closeTab} className="icon-button file-editor-close-button" onClick={onClose} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>
      </div>

      <div className="file-editor-workspace">
        <section className="file-editor-main">
          <div className="file-editor-toolbar">
            <div className="file-editor-menubar">
              <EditorMenuButton current={openMenu} label={t.fileEditorFile} menu="file" onToggle={setOpenMenu}>
                <MenuAction disabled={!isDirty || isBusy || isSaving} label={isSaving ? t.saving : t.save} onClick={() => onSave(content, encoding)} />
                <MenuAction label={t.fileEditorReloadEncoding} onClick={() => setOpenMenu('encoding')} />
              </EditorMenuButton>
              <EditorMenuButton current={openMenu} label={t.edit} menu="edit" onToggle={setOpenMenu}>
                <MenuAction label={t.fileEditorUndo} onClick={() => void runEditorAction('undo')} />
                <MenuAction label={t.fileEditorRedo} onClick={() => void runEditorAction('redo')} />
                <MenuSeparator />
                <MenuAction label={t.fileEditorSelectAll} onClick={() => void runEditorAction('editor.action.selectAll')} />
                <MenuSeparator />
                <MenuAction label={t.fileEditorToTraditional} onClick={() => convertContent(toTraditional)} />
                <MenuAction label={t.fileEditorToSimplified} onClick={() => convertContent(toSimplified)} />
              </EditorMenuButton>
              <EditorMenuButton current={openMenu} label={t.fileEditorSearch} menu="search" onToggle={setOpenMenu}>
                <MenuAction label={t.fileEditorFind} onClick={() => void runEditorAction('actions.find')} />
                <MenuAction label={t.fileEditorReplace} onClick={() => void runEditorAction('editor.action.startFindReplaceAction')} />
                <MenuAction label={t.fileEditorGoToLine} onClick={() => void runEditorAction('editor.action.gotoLine')} />
              </EditorMenuButton>
              <EditorMenuButton current={openMenu} label={t.fileEditorPreferences} menu="preferences" onToggle={setOpenMenu}>
                <MenuToggle label={t.fileEditorWordWrap} checked={wordWrap} onClick={() => setWordWrap((value) => !value)} />
                <MenuToggle label={t.fileEditorShowLineNumbers} checked={showLineNumbers} onClick={() => setShowLineNumbers((value) => !value)} />
                <MenuToggle label={t.fileEditorShowMinimap} checked={showMinimap} onClick={() => setShowMinimap((value) => !value)} />
              </EditorMenuButton>
            </div>

            <div className="file-editor-path" title={file.path}>{file.path}</div>
          </div>

          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

          <div className="file-editor-body">
            <div className="file-editor-surface">
              <Editor
                height="100%"
                onChange={(value) => setContent(value ?? '')}
                onMount={handleMount}
                options={{
                  automaticLayout: true,
                  find: {
                    addExtraSpaceOnTop: true,
                    seedSearchStringFromSelection: 'always'
                  },
                  fixedOverflowWidgets: true,
                  fontFamily: '"SF Mono", Menlo, Consolas, monospace',
                  fontLigatures: true,
                  fontSize: 13,
                  lineHeight: 20,
                  minimap: { enabled: showMinimap },
                  padding: { top: 14, bottom: 8 },
                  renderLineHighlight: 'line',
                  roundedSelection: true,
                  scrollBeyondLastLine: false,
                  smoothScrolling: true,
                  wordWrap: wordWrap ? 'on' : 'off',
                  lineNumbers: showLineNumbers ? 'on' : 'off'
                }}
                path={file.path}
                theme={themeMode === 'default-dark' ? MONACO_DARK_THEME : 'vs'}
                value={content}
              />
            </div>
          </div>

          <div className="file-editor-statusbar">
            <span>{t.fileEditorStatusReady}</span>
            <span>{t.fileEditorLines}: {lineCount}</span>
            <span>{t.fileEditorCharacters}: {characterCount}</span>
            <span>{t.fileEditorCursor}: {cursorLine}:{cursorColumn}</span>
            <div className="file-editor-status-actions">
              <StatusMenu
                current={openMenu}
                label={currentEncoding.label}
                menu="encoding"
                onToggle={setOpenMenu}
              >
                {EDITOR_ENCODINGS.map((option) => (
                  <button
                    className={option.value === encoding ? 'is-active' : ''}
                    key={option.value}
                    onClick={() => {
                      setEncoding(option.value)
                      onReloadWithEncoding(option.value)
                      setOpenMenu(null)
                    }}
                    type="button"
                  >
                    {option.label}
                  </button>
                ))}
              </StatusMenu>
              <StatusMenu
                current={openMenu}
                label={currentLanguage}
                menu="language"
                onToggle={setOpenMenu}
              >
                {languages.map((option) => (
                  <button
                    className={option.id === language ? 'is-active' : ''}
                    key={option.id}
                    onClick={() => updateLanguage(option.id)}
                    type="button"
                  >
                    {option.label}
                  </button>
                ))}
              </StatusMenu>
            </div>
          </div>
        </section>
      </div>
    </div>
  )

  if (standalone) {
    return <div className="standalone-shell file-editor-window">{contentNode}</div>
  }

  return <div className="modal-backdrop">{contentNode}</div>
}

function EditorMenuButton({
  children,
  current,
  label,
  menu,
  onToggle
}: {
  children: ReactNode
  current: EditorMenu | null
  label: string
  menu: 'file' | 'edit' | 'search' | 'preferences'
  onToggle(menu: EditorMenu | null): void
}) {
  const open = current === menu

  return (
    <div className="file-editor-menu-anchor" data-file-editor-menu-scope="true">
      <button className={`file-editor-menubar-button ${open ? 'is-open' : ''}`} onClick={() => onToggle(open ? null : menu)} type="button">
        {label}
      </button>
      {open ? <div className="file-editor-menu">{children}</div> : null}
    </div>
  )
}

function StatusMenu({
  children,
  current,
  label,
  menu,
  onToggle
}: {
  children: ReactNode
  current: EditorMenu | null
  label: string
  menu: 'encoding' | 'language'
  onToggle(menu: EditorMenu | null): void
}) {
  const open = current === menu

  return (
    <div className="file-editor-status-menu" data-file-editor-menu-scope="true">
      <button className={`file-editor-status-button ${open ? 'is-open' : ''}`} onClick={() => onToggle(open ? null : menu)} type="button">
        {label}
      </button>
      {open ? <div className="file-editor-menu file-editor-menu--wide file-editor-menu--upward file-editor-menu--align-end">{children}</div> : null}
    </div>
  )
}

function MenuAction({ disabled = false, label, onClick }: { disabled?: boolean; label: string; onClick(): void }) {
  return <button disabled={disabled} onClick={onClick} type="button">{label}</button>
}

function MenuToggle({ checked, label, onClick }: { checked: boolean; label: string; onClick(): void }) {
  return (
    <button onClick={onClick} type="button">
      <span>{checked ? '✓ ' : ''}</span>
      {label}
    </button>
  )
}

function MenuSeparator() {
  return <div className="file-editor-menu-separator" />
}
