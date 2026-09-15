import { invoke } from '@tauri-apps/api/core'
import type { AiSettings } from '../types'
import type { AppConfig } from '../domain/settings'

export const MIGRATED_PROVIDER_ID = 'migrated-default-provider'

export async function getAppConfig(): Promise<AppConfig> {
  return invoke<AppConfig>('settings_get')
}

export async function replaceAppConfig(config: AppConfig): Promise<AppConfig> {
  return invoke<AppConfig>('settings_replace', { config })
}

export async function migrateLegacyAppConfig(settings: AiSettings): Promise<AppConfig> {
  return invoke<AppConfig>('settings_migrate_legacy', {
    legacy: {
      adapter: settings.adapter,
      apiBase: settings.apiBase,
      model: settings.model,
      targetLanguage: settings.targetLanguage,
      alternateLanguage: settings.alternateLanguage
    }
  })
}

export async function loadProviderApiKey(credentialId: string): Promise<string> {
  return invoke<string>('load_provider_api_key', { credentialId })
}

export async function saveProviderApiKey(credentialId: string, apiKey: string): Promise<void> {
  await invoke('save_provider_api_key', { credentialId, apiKey })
}

export async function migrateLegacyApiKey(credentialId: string): Promise<boolean> {
  return invoke<boolean>('migrate_legacy_api_key', { credentialId })
}
