import type { ActionRoute, AppConfig, GenerationOptions, ModelRef } from '../domain/settings'
import { replaceAppConfig } from './settings-service'

export function routeForAction(config: AppConfig, actionId: string): ActionRoute | undefined {
  const action = config.actions.find((item) => item.id === actionId)
  return action?.routeId
    ? config.routes.find((route) => route.id === action.routeId)
    : config.routes.find((route) => route.actionId === actionId)
}
export async function saveActionRoute(
  config: AppConfig,
  actionId: string,
  primary: ModelRef,
  fallbacks?: ModelRef[],
  options?: GenerationOptions
): Promise<AppConfig> {
  const action = config.actions.find((item) => item.id === actionId)
  if (!action) throw new Error(`Unknown action: ${actionId}`)

  const provider = config.providers.find((item) => item.id === primary.providerId)
  if (!provider) throw new Error('选择的渠道不存在')
  const primaryModel = primary.model.trim()
  if (!primaryModel) throw new Error('动作模型不能为空')

  const existing = routeForAction(config, actionId)
  const requestedFallbacks = fallbacks ?? existing?.fallbacks ?? []
  const seen = new Set([`${primary.providerId}\u0000${primaryModel}`])
  const normalizedFallbacks: ModelRef[] = []
  for (const fallback of requestedFallbacks) {
    if (!config.providers.some((item) => item.id === fallback.providerId)) {
      throw new Error('备用渠道不存在')
    }
    const model = fallback.model.trim()
    if (!model) throw new Error('备用模型不能为空')
    const key = `${fallback.providerId}\u0000${model}`
    if (seen.has(key)) continue
    seen.add(key)
    normalizedFallbacks.push({ providerId: fallback.providerId, model })
  }

  const routeId = existing?.id || `builtin.${actionId}.route`
  const nextRoute: ActionRoute = {
    id: routeId,
    actionId,
    primary: { providerId: primary.providerId, model: primaryModel },
    fallbacks: normalizedFallbacks,
    options: options ?? existing?.options ?? {}
  }

  const routes = existing
    ? config.routes.map((route) => route.id === existing.id ? nextRoute : route)
    : [...config.routes, nextRoute]

  const next: AppConfig = {
    ...config,
    routes,
    actions: config.actions.map((item) =>
      item.id === actionId ? { ...item, routeId } : item
    )
  }
  return replaceAppConfig(next)
}

export async function clearActionRoute(config: AppConfig, actionId: string): Promise<AppConfig> {
  const existing = routeForAction(config, actionId)
  if (!existing) return config

  const next: AppConfig = {
    ...config,
    routes: config.routes.filter((route) => route.id !== existing.id),
    actions: config.actions.map((item) =>
      item.id === actionId ? { ...item, routeId: undefined } : item
    )
  }
  return replaceAppConfig(next)
}
