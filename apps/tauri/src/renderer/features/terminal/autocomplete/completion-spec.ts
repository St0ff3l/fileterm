export type Completion = { value: string; description: string; label?: string }
export type SpecSuggestion = { name: string | string[]; description?: string; insertValue?: string; hidden?: boolean }
export type SpecArg = {
  name?: string | string[]
  description?: string
  suggestions?: Array<string | SpecSuggestion>
  template?: string | string[]
  isOptional?: boolean
  isVariadic?: boolean
  isCommand?: boolean
}
export type SpecOption = SpecSuggestion & {
  args?: SpecArg | SpecArg[]
  isPersistent?: boolean
  isRepeatable?: boolean | number
  exclusiveOn?: string[]
  dependsOn?: string[]
}
export type CommandSpec = SpecSuggestion & {
  subcommands?: CommandSpec[]
  options?: SpecOption[]
  args?: SpecArg | SpecArg[]
  loadSpec?: string | CommandSpec
}
export type SpecLoader = (name: string) => Promise<CommandSpec | null>
export const names = (value: string | string[] | undefined): string[] =>
  value === undefined ? [] : Array.isArray(value) ? value : [value]
export const args = (value: SpecArg | SpecArg[] | undefined): SpecArg[] =>
  value === undefined ? [] : Array.isArray(value) ? value : [value]
