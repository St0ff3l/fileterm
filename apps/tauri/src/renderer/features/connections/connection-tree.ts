import type { ConnectionFolder, ConnectionProfile } from '@fileterm/core'

export type ConnectionTreeNode =
  (ConnectionFolder & { children: ConnectionTreeNode[] }) | (ConnectionProfile & { children?: never })

export function buildConnectionTree(profiles: ConnectionProfile[], folders: ConnectionFolder[]) {
  const items: ConnectionTreeNode[] = [
    ...profiles.map((profile, index) => ({
      ...profile,
      order: typeof profile.order === 'number' ? profile.order : index * 1000
    })),
    ...folders.map((folder, index) => ({
      ...folder,
      order: typeof folder.order === 'number' ? folder.order : (profiles.length + index) * 1000,
      children: []
    }))
  ]

  const roots: ConnectionTreeNode[] = []
  const map = new Map<string, ConnectionTreeNode>()

  items.forEach((item) => {
    map.set(item.id, item)
  })

  items.forEach((item) => {
    const parent = item.parentId ? map.get(item.parentId) : undefined
    if (parent?.type === 'folder') {
      parent.children.push(item)
    } else {
      roots.push(item)
    }
  })

  const sortNodes = (nodes: ConnectionTreeNode[]) => {
    nodes.sort((left, right) => (left.order ?? 0) - (right.order ?? 0))
    nodes.forEach((node) => {
      if (node.type === 'folder') sortNodes(node.children)
    })
  }
  sortNodes(roots)

  return { roots, map }
}

export function flattenConnectionProfiles(nodes: ConnectionTreeNode[]) {
  const profiles: ConnectionProfile[] = []

  const visit = (items: ConnectionTreeNode[]) => {
    items.forEach((item) => {
      if (item.type === 'folder') {
        visit(item.children)
      } else {
        profiles.push(item)
      }
    })
  }

  visit(nodes)
  return profiles
}
