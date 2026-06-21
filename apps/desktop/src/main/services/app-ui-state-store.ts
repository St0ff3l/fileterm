import { mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

type StoredUiState = {
  version: 1
  values: Record<string, string>
}

const EMPTY_UI_STATE: StoredUiState = {
  version: 1,
  values: {}
}

export class AppUiStateStore {
  private readonly filePath: string
  private ready: Promise<void>

  constructor(baseDir: string) {
    this.filePath = path.join(baseDir, 'ui-state.json')
    this.ready = this.ensureFile()
  }

  async getItem(key: string): Promise<string | null> {
    const state = await this.readState()
    return state.values[key] ?? null
  }

  async setItem(key: string, value: string): Promise<void> {
    const state = await this.readState()
    await this.writeState({
      ...state,
      values: {
        ...state.values,
        [key]: value
      }
    })
  }

  async removeItem(key: string): Promise<void> {
    const state = await this.readState()
    if (!(key in state.values)) {
      return
    }

    const nextValues = { ...state.values }
    delete nextValues[key]
    await this.writeState({
      ...state,
      values: nextValues
    })
  }

  private async ensureFile() {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    try {
      await readFile(this.filePath, 'utf8')
    } catch {
      await this.writeStateFile(EMPTY_UI_STATE)
    }
  }

  private async readState(): Promise<StoredUiState> {
    await this.ready
    try {
      const raw = await readFile(this.filePath, 'utf8')
      const parsed = JSON.parse(raw) as Partial<StoredUiState>
      return {
        version: 1,
        values: typeof parsed.values === 'object' && parsed.values
          ? Object.fromEntries(
              Object.entries(parsed.values).filter(
                (entry): entry is [string, string] => typeof entry[0] === 'string' && typeof entry[1] === 'string'
              )
            )
          : {}
      }
    } catch {
      return EMPTY_UI_STATE
    }
  }

  private async writeState(state: StoredUiState): Promise<void> {
    await this.ready
    await this.writeStateFile(state)
  }

  private async writeStateFile(state: StoredUiState): Promise<void> {
    await writeFile(this.filePath, JSON.stringify(state, null, 2), 'utf8')
  }
}
