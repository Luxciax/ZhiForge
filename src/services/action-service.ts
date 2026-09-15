import type { ActionDefinition, AppConfig } from '../domain/settings'
import { replaceAppConfig } from './settings-service'

export type ActionDefinitionPatch = Partial<Pick<
  ActionDefinition,
  'name' | 'enabled' | 'showInToolbar' | 'prompt' | 'targetLanguage' | 'alternateLanguage' | 'outputMode'
>>

export async function setActionEnabled(
  config: AppConfig,
  actionId: string,
  enabled: boolean
): Promise<AppConfig> {
  return updateActionDefinition(config, actionId, { enabled })
}

export async function updateActionDefinition(
  config: AppConfig,
  actionId: string,
  patch: ActionDefinitionPatch
): Promise<AppConfig> {
  const current = config.actions.find((action) => action.id === actionId)
  if (!current) throw new Error(`Unknown action: ${actionId}`)

  const nextAction: ActionDefinition = {
    ...current,
    ...patch,
    name: (patch.name ?? current.name).trim(),
    prompt: patch.prompt === undefined ? current.prompt : patch.prompt.trim() || undefined,
    targetLanguage: patch.targetLanguage === undefined ? current.targetLanguage : patch.targetLanguage.trim() || undefined,
    alternateLanguage: patch.alternateLanguage === undefined ? current.alternateLanguage : patch.alternateLanguage.trim() || undefined
  }
  validateActionDraft(nextAction)

  return replaceAppConfig({
    ...config,
    actions: config.actions.map((action) => action.id === actionId ? nextAction : action)
  })
}

export async function createCustomAction(config: AppConfig): Promise<{ config: AppConfig; actionId: string }> {
  const actionId = `custom-${crypto.randomUUID()}`
  const order = config.actions.reduce((max, action) => Math.max(max, action.order), -1) + 1
  const action: ActionDefinition = {
    id: actionId,
    name: '新动作',
    icon: 'Sparkles',
    enabled: true,
    builtin: false,
    showInToolbar: true,
    promptTemplateId: `custom.${actionId}`,
    prompt: '根据选中的文本完成当前动作，并使用 {target_language} 输出结果。',
    targetLanguage: undefined,
    alternateLanguage: undefined,
    outputMode: 'markdown',
    order
  }
  const next = await replaceAppConfig({ ...config, actions: [...config.actions, action] })
  return { config: next, actionId }
}

export async function deleteCustomAction(config: AppConfig, actionId: string): Promise<AppConfig> {
  const current = config.actions.find((action) => action.id === actionId)
  if (!current) throw new Error(`Unknown action: ${actionId}`)
  if (current.builtin) throw new Error('内置动作不能删除')

  const routes = config.routes.filter((route) => route.actionId !== actionId)
  const actions = normalizeOrders(config.actions.filter((action) => action.id !== actionId))
  return replaceAppConfig({ ...config, actions, routes })
}

export async function moveAction(config: AppConfig, actionId: string, direction: -1 | 1): Promise<AppConfig> {
  const ordered = [...config.actions].sort((a, b) => a.order - b.order)
  const index = ordered.findIndex((action) => action.id === actionId)
  if (index < 0) throw new Error(`Unknown action: ${actionId}`)
  const target = index + direction
  if (target < 0 || target >= ordered.length) return config
  const [item] = ordered.splice(index, 1)
  ordered.splice(target, 0, item)
  return replaceAppConfig({ ...config, actions: normalizeOrders(ordered) })
}

function normalizeOrders(actions: ActionDefinition[]): ActionDefinition[] {
  return actions.map((action, order) => ({ ...action, order }))
}

function validateActionDraft(action: ActionDefinition) {
  if (!action.name.trim()) throw new Error('动作名称不能为空')
  if (!action.builtin && !action.prompt?.trim()) throw new Error('自定义动作需要 Prompt')
  if ((action.prompt?.length || 0) > 65_536) throw new Error('Prompt 过长')
  if ((action.targetLanguage?.length || 0) > 64 || (action.alternateLanguage?.length || 0) > 64) {
    throw new Error('动作语言名称过长')
  }
}
