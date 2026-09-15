import type { AiSettings, ProviderAdapter } from '../types'
import type { AppConfig, ProviderAdapterId, ProviderProfile } from '../domain/settings'
import { PROVIDER_BASES } from '../ai/config'
import {
  getAppConfig,
  loadProviderApiKey,
  MIGRATED_PROVIDER_ID,
  migrateLegacyApiKey,
  migrateLegacyAppConfig,
  saveProviderApiKey
} from './settings-service'

export function adapterIdToLegacy(adapterId: ProviderAdapterId): ProviderAdapter {
  return adapterId === 'openai-chat' ? 'openai-compatible' : adapterId
}

export function legacyAdapterToId(adapter: ProviderAdapter): ProviderAdapterId {
  return adapter === 'openai-compatible' ? 'openai-chat' : adapter
}

export function defaultBaseForAdapter(adapterId: ProviderAdapterId): string {
  return PROVIDER_BASES[adapterIdToLegacy(adapterId)]
}

export function createProviderProfile(adapterId: ProviderAdapterId = 'openai-chat'): ProviderProfile {
  const id = `provider-${crypto.randomUUID()}`
  return {
    id,
    name: '新渠道',
    adapterId,
    apiBase: defaultBaseForAdapter(adapterId),
    headers: {},
    credentialId: id,
    defaultModel: '',
    enabled: true,
    defaults: {},
    extra: {}
  }
}

export function providerToDisplaySettings(
  profile: ProviderProfile,
  config: AppConfig
): AiSettings {
  return {
    providerId: profile.id,
    credentialId: profile.credentialId || profile.id,
    adapter: adapterIdToLegacy(profile.adapterId),
    apiBase: profile.apiBase,
    apiKey: '',
    model: profile.defaultModel || '',
    headers: { ...(profile.headers || {}) },
    targetLanguage: config.ui.targetLanguage,
    alternateLanguage: config.ui.alternateLanguage
  }
}

export async function providerToAiSettings(
  profile: ProviderProfile,
  config: AppConfig,
  apiKey?: string
): Promise<AiSettings> {
  const display = providerToDisplaySettings(profile, config)
  const resolvedKey = apiKey ?? await loadProviderApiKey(display.credentialId || profile.id).catch(() => '')
  return { ...display, apiKey: resolvedKey }
}

export async function loadActiveAiSettings(): Promise<AiSettings | null> {
  const config = await getAppConfig()
  const id = config.ui.activeProviderId || config.providers[0]?.id
  const profile = config.providers.find((provider) => provider.id === id)
  return profile ? providerToDisplaySettings(profile, config) : null
}

export async function bootstrapV2Settings(
  legacySettings: AiSettings,
  hasLegacySettings: boolean
): Promise<AiSettings | null> {
  let config = await getAppConfig()
  if (config.providers.length === 0 && hasLegacySettings) {
    config = await migrateLegacyAppConfig(legacySettings)
  }

  const migratedProfile = config.providers.find((provider) => provider.id === MIGRATED_PROVIDER_ID)
  if (migratedProfile) {
    const credentialId = migratedProfile.credentialId || migratedProfile.id
    const existingKey = await loadProviderApiKey(credentialId).catch(() => '')
    if (!existingKey) {
      if (legacySettings.apiKey) {
        await saveProviderApiKey(credentialId, legacySettings.apiKey)
      } else {
        await migrateLegacyApiKey(credentialId).catch(() => false)
      }
    }
  }

  const id = config.ui.activeProviderId || config.providers[0]?.id
  const profile = config.providers.find((provider) => provider.id === id)
  return profile ? providerToDisplaySettings(profile, config) : null
}

export function removeProviderReferences(config: AppConfig, providerId: string): AppConfig {
  const routes = config.routes
    .filter((route) => route.primary.providerId !== providerId)
    .map((route) => ({
      ...route,
      fallbacks: route.fallbacks?.filter((fallback) => fallback.providerId !== providerId)
    }))
  const routeIds = new Set(routes.map((route) => route.id))
  const providers = config.providers.filter((provider) => provider.id !== providerId)
  const activeProviderId = config.ui.activeProviderId === providerId
    ? providers[0]?.id
    : config.ui.activeProviderId

  return {
    ...config,
    providers,
    routes,
    actions: config.actions.map((action) =>
      action.routeId && !routeIds.has(action.routeId) ? { ...action, routeId: undefined } : action
    ),
    ui: { ...config.ui, activeProviderId }
  }
}
