import fs from 'node:fs'
import path from 'node:path'

let logDirectoryPath: string | null = null
let logFilePath: string | null = null

export function initAppLogger(userDataPath: string) {
  logDirectoryPath = path.join(userDataPath, 'logs')
  logFilePath = path.join(logDirectoryPath, 'termdock.log')
  fs.mkdirSync(logDirectoryPath, { recursive: true })
}

export function getAppLogDirectory() {
  if (!logDirectoryPath) {
    throw new Error('App logger is not initialized')
  }
  return logDirectoryPath
}

export function appLog(...args: unknown[]) {
  writeLogLine('INFO', args)
  console.log(...args)
}

export function appWarn(...args: unknown[]) {
  writeLogLine('WARN', args)
  console.warn(...args)
}

export function appError(...args: unknown[]) {
  writeLogLine('ERROR', args)
  console.error(...args)
}

function writeLogLine(level: 'INFO' | 'WARN' | 'ERROR', args: unknown[]) {
  if (!logFilePath) {
    return
  }

  const line = `[${new Date().toISOString()}] [${level}] ${args.map(stringifyLogPart).join(' ')}\n`
  try {
    fs.appendFileSync(logFilePath, line, 'utf8')
  } catch {
    // Avoid recursive logger failures from breaking the app.
  }
}

function stringifyLogPart(value: unknown) {
  if (typeof value === 'string') {
    return value
  }
  if (value instanceof Error) {
    return value.stack || value.message
  }
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}
