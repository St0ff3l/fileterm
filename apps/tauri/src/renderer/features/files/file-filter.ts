export type FileFilterMode = 'text' | 'glob' | 'regex'

export interface FileFilterConfig {
  query: string
  mode: FileFilterMode
  caseSensitive?: boolean
}

/**
 * Converts a wildcard glob pattern (supporting * and ?) to a standard RegExp.
 */
export function globToRegex(pattern: string, caseSensitive = false): RegExp {
  const escaped = pattern
    .trim()
    .replace(/[-[\]{}()+.,^$|#\s]/g, '\\$&') // escape regex special chars except * and ?
    .replace(/\*/g, '.*') // * matches zero or more characters
    .replace(/\?/g, '.') // ? matches any single character

  return new RegExp(`^${escaped}$`, caseSensitive ? '' : 'i')
}

/**
 * Evaluates whether a filename matches the specified filter configuration.
 * Always returns true for '..' (parent directory).
 */
export function matchesFileFilter(filename: string, filter: FileFilterConfig): boolean {
  if (filename === '..') {
    return true
  }

  const query = filter.query.trim()
  if (!query) {
    return true
  }

  const flags = filter.caseSensitive ? '' : 'i'

  try {
    switch (filter.mode) {
      case 'glob': {
        const regex = globToRegex(query, filter.caseSensitive)
        return regex.test(filename)
      }
      case 'regex': {
        const regex = new RegExp(query, flags)
        return regex.test(filename)
      }
      case 'text':
      default: {
        return filter.caseSensitive ? filename.includes(query) : filename.toLowerCase().includes(query.toLowerCase())
      }
    }
  } catch {
    // Fallback gracefully to substring match if regex is invalid (e.g. unclosed bracket)
    return filename.toLowerCase().includes(query.toLowerCase())
  }
}
