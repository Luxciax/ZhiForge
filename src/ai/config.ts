import type { AiSettings, ProviderAdapter, ProviderConfig } from '../types'

const STORAGE_KEY = 'zhiforge.ai-settings.v1'

export const PROVIDER_BASES: Record<ProviderAdapter, string> = {
  'openai-compatible': 'https://api.openai.com/v1',
  'openai-responses': 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com',
  gemini: 'https://generativelanguage.googleapis.com/v1beta'
}

export const LANGUAGE_OPTIONS = [
  '简体中文',
  '繁體中文',
  'English',
  '日本語',
  '한국어',
  'Français',
  'Deutsch',
  'Español',
  'Русский'
]

export const DEFAULT_AI_SETTINGS: AiSettings = {
  adapter: 'openai-compatible',
  apiBase: PROVIDER_BASES['openai-compatible'],
  apiKey: '',
  model: '',
  headers: {},
  targetLanguage: '简体中文',
  alternateLanguage: 'English'
}

export function loadAiSettings(): AiSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return DEFAULT_AI_SETTINGS
    const parsed = JSON.parse(raw) as Partial<AiSettings>
    const adapter = isProviderAdapter(parsed.adapter) ? parsed.adapter : DEFAULT_AI_SETTINGS.adapter
    return {
      providerId: typeof parsed.providerId === 'string' ? parsed.providerId : undefined,
      credentialId: typeof parsed.credentialId === 'string' ? parsed.credentialId : undefined,
      adapter,
      apiBase: typeof parsed.apiBase === 'string' ? parsed.apiBase : PROVIDER_BASES[adapter],
      apiKey: typeof parsed.apiKey === 'string' ? parsed.apiKey : '',
      model: typeof parsed.model === 'string' ? parsed.model : '',
      headers: parsed.headers && typeof parsed.headers === 'object' ? parsed.headers as Record<string, string> : {},
      targetLanguage: typeof parsed.targetLanguage === 'string' ? parsed.targetLanguage : '简体中文',
      alternateLanguage: typeof parsed.alternateLanguage === 'string' ? parsed.alternateLanguage : 'English'
    }
  } catch {
    return DEFAULT_AI_SETTINGS
  }
}

export function saveAiSettings(settings: AiSettings) {
  const { apiKey: _secret, ...persisted } = settings
  localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted))
}

export function hasStoredAiSettings() {
  return localStorage.getItem(STORAGE_KEY) !== null
}

export function clearStoredAiSettings() {
  localStorage.removeItem(STORAGE_KEY)
}

export function isAiConfigured(settings: AiSettings) {
  return settings.apiBase.trim().length > 0 && settings.model.trim().length > 0
}

export function toProviderConfig(settings: AiSettings): ProviderConfig {
  return {
    adapter: settings.adapter,
    apiBase: settings.apiBase.trim(),
    apiKey: settings.apiKey.trim(),
    model: settings.model.trim(),
    headers: { ...settings.headers }
  }
}

export function switchAdapter(current: AiSettings, adapter: ProviderAdapter): AiSettings {
  const knownBases = Object.values(PROVIDER_BASES)
  const shouldReplaceBase = current.apiBase.trim() === '' || knownBases.includes(current.apiBase.trim())
  return {
    ...current,
    adapter,
    apiBase: shouldReplaceBase ? PROVIDER_BASES[adapter] : current.apiBase,
    apiKey: '',
    model: '',
    headers: {}
  }
}

function isProviderAdapter(value: unknown): value is ProviderAdapter {
  return value === 'openai-compatible' || value === 'openai-responses' || value === 'anthropic' || value === 'gemini'
}
