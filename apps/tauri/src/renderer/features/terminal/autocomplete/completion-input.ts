import { hasControlCharacters } from './completion-context'

export type InputScreen = {
  row: number
  before: string
  after: string
  normal: boolean
  wrapped: boolean
  atBottom: boolean
}

function isInteractivePrompt(text: string) {
  return (
    /(?:password|passphrase|verification code|one[- ]time|passcode|pin|token|密码|口令|验证码)[^\r\n]*[:：]\s*$/iu.test(
      text
    ) || /^(?:>{2,}|(?:mysql|sqlite|psql|redis|node|python|sftp|ftp)>)/iu.test(text)
  )
}

function promptBoundary(text: string): number | null {
  // A bare > can be PS2 or a REPL; require OSC 7 to treat it as a shell prompt.
  if (isInteractivePrompt(text)) return null
  const simple = /^\s*[$#%❯❮➜→»›](?:\s+|$)/u.exec(text)
  if (simple) return simple[0].length
  const hosted =
    /^(?:(?:\([^\r\n)]*\)\s*)?(?:\[[^\r\n\]]+\]|[^\s@]+@[^\s$#%❯]+|(?:bash|zsh|ash|sh)(?:-\d[\d.]*)?))[^\r\n]*?[$#%❯]\s*/u.exec(
      text
    )
  return hosted?.[0].length ?? null
}

export class CompletionInput {
  private row = -1
  private prompt = ''
  private marked = false
  private suspendedRow: number | null = null
  input = ''

  markPrompt() {
    this.reset()
    this.marked = true
    this.suspendedRow = null
  }

  reset() {
    this.suspendedRow = this.row === -1 ? this.suspendedRow : this.row
    this.row = -1
    this.prompt = ''
    this.marked = false
    this.input = ''
  }

  private usable(screen: InputScreen) {
    return screen.normal && !screen.wrapped && screen.atBottom && !screen.after.trim()
  }

  synchronize(screen: InputScreen) {
    if (this.marked || !this.usable(screen) || this.row === screen.row || this.suspendedRow === screen.row) return
    const boundary = promptBoundary(screen.before)
    if (boundary === null) return
    this.row = screen.row
    this.prompt = screen.before.slice(0, boundary)
    this.input = screen.before.slice(boundary)
    this.suspendedRow = null
  }

  observe(data: string, screen: InputScreen) {
    if (!this.usable(screen) || (hasControlCharacters(data) && data !== '\x7f')) {
      this.reset()
      this.suspendedRow = screen.row
      return
    }
    if (this.marked && this.row === -1 && screen.before) {
      if (isInteractivePrompt(screen.before)) {
        this.reset()
        this.suspendedRow = screen.row
        return
      }
      const boundary = screen.before.length
      this.row = screen.row
      this.prompt = screen.before.slice(0, boundary)
      this.input = screen.before.slice(boundary)
      this.marked = false
    } else this.synchronize(screen)
    if (
      screen.row !== this.row ||
      !screen.before.startsWith(this.prompt) ||
      !(this.prompt + this.input).startsWith(screen.before)
    ) {
      this.reset()
      this.suspendedRow = screen.row
      return
    }
    if (data === '\x7f') this.input = Array.from(this.input).slice(0, -1).join('')
    else this.input += data
  }

  matches(screen: InputScreen) {
    return (
      this.row === screen.row &&
      this.usable(screen) &&
      screen.before === this.prompt + this.input &&
      Boolean(this.input)
    )
  }
}
