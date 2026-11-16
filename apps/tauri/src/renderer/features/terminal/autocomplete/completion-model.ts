import type { RemoteFileItem } from '@fileterm/core'
import {
  args,
  names,
  type CommandSpec,
  type Completion,
  type SpecArg,
  type SpecLoader,
  type SpecOption
} from './completion-spec'
import { completionContext, completeToken, hasControlCharacters, type CompletionContext } from './completion-context'
export { completionContext, hasControlCharacters } from './completion-context'
export type { Completion } from './completion-spec'

export function uniqueCompletions(input: string, items: Completion[]): Completion[] {
  const seen = new Set<string>()
  const result: Completion[] = []
  for (const item of items) {
    if (item.value.length <= input.length || !item.value.startsWith(input) || seen.has(item.value)) continue
    seen.add(item.value)
    result.push(item)
    if (result.length === 80) break
  }
  return result
}

function suggestion(input: string, context: CompletionContext, value: string, description = ''): Completion[] {
  const completed = completeToken(input, context, value)
  return completed ? [{ value: completed, label: value, description }] : []
}

async function expand(spec: CommandSpec, load: SpecLoader): Promise<CommandSpec> {
  // Some upstream loadSpec definitions require executing CLI/version detectors; data-only loading does not run those.
  for (let depth = 0; spec.loadSpec && depth < 8; depth++) {
    const loaded = typeof spec.loadSpec === 'string' ? await load(spec.loadSpec) : spec.loadSpec
    if (!loaded || loaded === spec) break
    spec = {
      ...loaded,
      ...spec,
      subcommands: spec.subcommands ?? loaded.subcommands,
      options: spec.options ?? loaded.options,
      args: spec.args ?? loaded.args,
      loadSpec: loaded.loadSpec
    }
  }
  return spec
}

export async function commandCompletions(
  input: string,
  history: string[],
  historyDescription: string,
  commands: Completion[],
  load: SpecLoader
): Promise<{ items: Completion[]; context: CompletionContext | null; pathMode: 'files' | 'folders' | null }> {
  const context = completionContext(input)
  const empty = { items: [] as Completion[], context, pathMode: null as 'files' | 'folders' | null }
  if (!context) return empty
  const { tokens, token, commandIndex } = context
  const historyItems = history
    .filter((value) => value.startsWith(input) && !hasControlCharacters(value))
    .map((value) => ({ value, description: historyDescription }))
  if (commandIndex === tokens.length - 1) {
    if (
      token &&
      (commands.some((command) => command.value === token) || token.includes('/')) &&
      (await load(token.split('/').at(-1)!))
    ) {
      const preview = await commandCompletions(input + ' ', history, historyDescription, commands, load)
      return { ...preview, context, pathMode: null }
    }
    const commandItems = commands.flatMap((command) => suggestion(input, context, command.value, command.description))
    return { ...empty, items: uniqueCompletions(input, [...commandItems, ...historyItems]) }
  }
  let spec = await load(tokens[commandIndex].split('/').at(-1)!)
  if (!spec) return { ...empty, items: historyItems, pathMode: token.includes('/') ? ('files' as const) : null }
  spec = await expand(spec, load)
  let inherited: SpecOption[] = []
  const used = new Set<string>()
  let position = 0
  let pending: SpecArg[] = []
  let endOptions = false
  let positionalStarted = false
  const options = () => [...inherited, ...(spec!.options ?? [])]
  for (const consumed of tokens.slice(commandIndex + 1, -1)) {
    if (pending.length && (!pending[0].isOptional || !consumed.startsWith('-'))) {
      if (!pending[0].isVariadic) pending.shift()
      continue
    }
    pending = []
    if (!endOptions && consumed === '--') {
      endOptions = true
      continue
    }
    if (!endOptions && consumed.startsWith('-')) {
      const name = consumed.split('=')[0]
      let matched = options().find((option) => names(option.name).includes(name))
      if (!matched && /^-[^-]{2,}$/.test(name)) {
        for (const short of name.slice(1)) {
          const option = options().find((option) => names(option.name).includes('-' + short))
          if (option) {
            names(option.name).forEach((alias) => used.add(alias))
            matched = option
          }
        }
      }
      if (matched) {
        names(matched.name).forEach((alias) => used.add(alias))
        if (!consumed.includes('=')) pending = [...args(matched.args)]
      }
      continue
    }
    const child = !positionalStarted && spec.subcommands?.find((command) => names(command.name).includes(consumed))
    if (child) {
      inherited = [...inherited, ...(spec.options?.filter((option) => option.isPersistent) ?? [])]
      spec = await expand(child, load)
      position = 0
      used.clear()
    } else {
      positionalStarted = true
      const positional = args(spec.args)
      if (!positional[position]?.isVariadic) position++
    }
  }
  if (
    token &&
    !pending.length &&
    !positionalStarted &&
    spec.subcommands?.some((child) => names(child.name).includes(token))
  ) {
    const preview = await commandCompletions(input + ' ', history, historyDescription, commands, load)
    return { ...preview, context, pathMode: null }
  }
  let argument = pending[0] ?? args(spec.args)[position]
  let argumentTokenPrefix = ''
  const equal = token.indexOf('=')
  if (!endOptions && token.startsWith('--') && equal !== -1) {
    argument = args(options().find((option) => names(option.name).includes(token.slice(0, equal)))?.args)[0]
    argumentTokenPrefix = token.slice(0, equal + 1)
  }
  const result: Completion[] = []
  if (!pending.length && !argumentTokenPrefix) {
    if (!positionalStarted)
      for (const child of spec.subcommands ?? [])
        if (!child.hidden)
          for (const name of names(child.name)) result.push(...suggestion(input, context, name, child.description))
    if (!endOptions && (!token || token.startsWith('-'))) {
      for (const option of options()) {
        if (
          option.hidden ||
          (!option.isRepeatable && names(option.name).some((name) => used.has(name))) ||
          option.exclusiveOn?.some((name) => used.has(name)) ||
          option.dependsOn?.some((name) => !used.has(name))
        )
          continue
        for (const name of names(option.name)) result.push(...suggestion(input, context, name, option.description))
      }
    }
  }
  for (const item of argument?.suggestions ?? []) {
    const values = typeof item === 'string' ? [item] : item.hidden ? [] : names(item.insertValue ?? item.name)
    for (const name of values)
      result.push(
        ...suggestion(
          input,
          context,
          argumentTokenPrefix + name,
          typeof item === 'string' ? argument?.description : item.description
        )
      )
  }
  if (argument?.isCommand)
    for (const command of commands) result.push(...suggestion(input, context, command.value, command.description))
  const templates = names(argument?.template)
  const argumentName = names(argument?.name).join(' ')
  const command = tokens[commandIndex].split('/').at(-1)
  const pathMode =
    command === 'cd' || /directory|folder/iu.test(argumentName)
      ? 'folders'
      : templates.includes('filepaths') || /file|pathspec|path/iu.test(argumentName)
        ? 'files'
        : templates.includes('folders')
          ? 'folders'
          : token.includes('/')
            ? 'files'
            : null
  return {
    items: uniqueCompletions(input, [...result, ...historyItems]),
    context,
    pathMode: pathMode as 'files' | 'folders' | null
  }
}

export function pathRequest(input: string, cwd: string) {
  const context = completionContext(input)
  if (!context || context.tokens.length < 2 || !cwd.startsWith('/')) return null
  const equal = context.token.startsWith('--') ? context.token.indexOf('=') : -1
  const optionPrefix = equal === -1 ? '' : context.token.slice(0, equal + 1)
  const token = optionPrefix ? context.token.slice(equal + 1) : context.token
  if (token.startsWith('-') || token.startsWith('~')) return null
  const slash = token.lastIndexOf('/')
  const directory = token.slice(0, slash + 1)
  const absolute = directory.startsWith('/') ? directory : `${cwd}/${directory}`
  const parts: string[] = []
  for (const part of absolute.split('/')) {
    if (part === '..') parts.pop()
    else if (part && part !== '.') parts.push(part)
  }
  return { path: `/${parts.join('/')}`, directory, basename: token.slice(slash + 1), context, optionPrefix }
}

export function pathCompletions(
  input: string,
  cwd: string,
  files: RemoteFileItem[],
  description: string,
  directoriesOnly = false
): Completion[] {
  const request = pathRequest(input, cwd)
  if (!request) return []
  return uniqueCompletions(
    input,
    files
      .filter(
        (file) =>
          (!directoriesOnly || file.type === 'folder') &&
          file.name.startsWith(request.basename) &&
          (request.basename.startsWith('.') || !file.name.startsWith('.')) &&
          !hasControlCharacters(file.name) &&
          !file.name.includes('/')
      )
      .flatMap((file) =>
        suggestion(
          input,
          request.context,
          request.optionPrefix + request.directory + file.name + (file.type === 'folder' ? '/' : ''),
          description
        )
      )
  )
}
