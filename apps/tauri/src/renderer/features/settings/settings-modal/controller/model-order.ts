export type AiModelDropPosition = 'before' | 'after'

export function reorderAiProviderModels(
  models: string[],
  source: string,
  target: string,
  position: AiModelDropPosition
) {
  if (source === target) return models
  const sourceIndex = models.indexOf(source)
  const targetIndex = models.indexOf(target)
  if (sourceIndex < 0 || targetIndex < 0) return models

  const next = [...models]
  next.splice(sourceIndex, 1)
  const insertIndex = next.indexOf(target) + Number(position === 'after')
  next.splice(insertIndex, 0, source)
  return next.every((model, index) => model === models[index]) ? models : next
}
