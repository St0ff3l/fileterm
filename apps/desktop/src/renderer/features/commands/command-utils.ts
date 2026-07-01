import type { CommandFolder, CommandTemplate } from '@fileterm/core'

export function sortByOrder<T extends { order?: number; name: string }>(items: T[]) {
  return [...items].sort((left, right) => {
    const orderDelta = (left.order ?? 0) - (right.order ?? 0)
    return orderDelta !== 0 ? orderDelta : left.name.localeCompare(right.name)
  })
}

export function extractCommandParams(command: string) {
  const matches = command.matchAll(/\[p#(\d+)\]/g)
  const seen = new Set<number>()

  for (const match of matches) {
    seen.add(Number(match[1]))
  }

  return [...seen].sort((left, right) => left - right)
}

export function groupCommands(
  folders: CommandFolder[],
  templates: CommandTemplate[]
) {
  const sortedFolders = sortByOrder(folders)
  const sortedTemplates = sortByOrder(templates)

  return sortedFolders.map((folder) => ({
    folder,
    templates: sortedTemplates.filter((template) => template.parentId === folder.id)
  }))
}
