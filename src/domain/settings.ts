export type ProviderAdapterId = 'openai-chat' | 'openai-responses' | 'anthropic' | 'gemini'

export interface GenerationOptions {
  temperature?: number
  maxOutputTokens?: number
  topP?: number
  reasoningMode?: string
}

export interface ProviderProfile {
  id: string
  name: string
  adapterId: ProviderAdapterId
  apiBase: string
  headers: Record<string, string>
  credentialId?: string
  defaultModel?: string
  enabled: boolean
  defaults: GenerationOptions
  extra?: Record<string, unknown>
}

export interface ActionDefinition {
  id: string
  name: string
  icon: string
  enabled: boolean
  builtin: boolean
  showInToolbar: boolean
  promptTemplateId: string
  prompt?: string
  targetLanguage?: string
  alternateLanguage?: string
  outputMode: 'markdown' | 'plain-text'
  routeId?: string
  order: number
}

export interface ModelRef {
  providerId: string
  model: string
}

export interface ActionRoute {
  id: string
  actionId: string
  primary: ModelRef
  fallbacks?: ModelRef[]
  options: GenerationOptions
}

export interface TriggerProfile {
  selectionEnabled: boolean
  dragEnabled: boolean
  doubleClickEnabled: boolean
  shiftClickEnabled: boolean
  requiredModifier: 'none' | 'ctrl' | 'alt' | 'shift'
  minDragDistance: number
  maxDragDurationMs: number
}

export interface AppRule {
  id: string
  processPattern: string
  behavior: 'inherit' | 'enable' | 'disable'
  clipboardFallback: 'inherit' | 'allow' | 'deny'
}

export interface UiPreferences {
  targetLanguage: string
  alternateLanguage: string
  activeProviderId?: string
}

export interface AppConfig {
  schemaVersion: 2
  providers: ProviderProfile[]
  actions: ActionDefinition[]
  routes: ActionRoute[]
  trigger: TriggerProfile
  appRules: AppRule[]
  ui: UiPreferences
}
