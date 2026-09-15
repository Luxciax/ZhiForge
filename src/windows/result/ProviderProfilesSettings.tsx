import { invoke } from '@tauri-apps/api/core'
import { Braces, Check, ChevronDown, Eye, EyeOff, Plus, RefreshCw, Save, SlidersHorizontal, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { LANGUAGE_OPTIONS, toProviderConfig } from '../../ai/config'
import type { AppConfig, ProviderAdapterId, ProviderProfile } from '../../domain/settings'
import { getModelCapabilities } from '../../services/capability-service'
import {
  createProviderProfile,
  defaultBaseForAdapter,
  providerToAiSettings,
  providerToDisplaySettings,
  removeProviderReferences
} from '../../services/provider-service'
import {
  loadProviderApiKey,
  replaceAppConfig,
  saveProviderApiKey
} from '../../services/settings-service'
import type { AiSettings, ModelCapabilities } from '../../types'

interface ProviderProfilesSettingsProps {
  config: AppConfig
  value: AiSettings
  onConfigChange: (config: AppConfig) => void
  onApply: (settings: AiSettings) => Promise<void>
}

const ADAPTER_LABELS: Record<ProviderAdapterId, string> = {
  'openai-chat': 'OpenAI Compatible',
  'openai-responses': 'OpenAI Responses',
  anthropic: 'Anthropic',
  gemini: 'Gemini'
}

function optionalNumber(value: string): number | undefined {
  if (!value.trim()) return undefined
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : undefined
}

function optionalInteger(value: string): number | undefined {
  const parsed = optionalNumber(value)
  return parsed === undefined ? undefined : Math.trunc(parsed)
}

function headersToText(headers: Record<string, string>): string {
  return Object.entries(headers || {}).map(([name, value]) => `${name}: ${value}`).join('\n')
}

function parseHeaders(text: string): Record<string, string> {
  const headers: Record<string, string> = {}
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim()
    if (!line) continue
    const separator = line.indexOf(':')
    if (separator <= 0) throw new Error(`Header 格式错误：${line}`)
    const name = line.slice(0, separator).trim()
    const value = line.slice(separator + 1).trim()
    if (!name || !value) throw new Error(`Header 格式错误：${line}`)
    headers[name] = value
  }
  return headers
}

export function ProviderProfilesSettings({
  config,
  value,
  onConfigChange,
  onApply
}: ProviderProfilesSettingsProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draft, setDraft] = useState<ProviderProfile | null>(null)
  const [apiKey, setApiKey] = useState('')
  const [targetLanguage, setTargetLanguage] = useState(value.targetLanguage)
  const [alternateLanguage, setAlternateLanguage] = useState(value.alternateLanguage)
  const [showKey, setShowKey] = useState(false)
  const [models, setModels] = useState<string[]>([])
  const [modelMenuOpen, setModelMenuOpen] = useState(false)
  const [modelError, setModelError] = useState<string | null>(null)
  const [loadingModels, setLoadingModels] = useState(false)
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)
  const [defaultsOpen, setDefaultsOpen] = useState(false)
  const [headersOpen, setHeadersOpen] = useState(false)
  const [headersText, setHeadersText] = useState('')
  const [capabilities, setCapabilities] = useState<ModelCapabilities | null>(null)


  const selectProvider = useCallback(async (profile: ProviderProfile) => {
    setSelectedId(profile.id)
    setDraft({
      ...profile,
      headers: { ...(profile.headers || {}) },
      defaults: { ...profile.defaults },
      extra: { ...(profile.extra || {}) }
    })
    setHeadersText(headersToText(profile.headers || {}))
    setModels([])
    setModelError(null)
    setSaved(false)
    const credentialId = profile.credentialId || profile.id
    const key = await loadProviderApiKey(credentialId).catch(() => '')
    setApiKey(key)
  }, [])

  useEffect(() => {
    setTargetLanguage(config.ui.targetLanguage)
    setAlternateLanguage(config.ui.alternateLanguage)
    const activeId = config.ui.activeProviderId || config.providers[0]?.id
    const active = config.providers.find((profile) => profile.id === activeId)
    if (active) {
      void selectProvider(active)
    } else {
      const profile = createProviderProfile()
      setSelectedId(profile.id)
      setDraft(profile)
      setHeadersText('')
      setApiKey('')
    }
  }, [config, selectProvider])

  useEffect(() => {
    if (!draft) {
      setCapabilities(null)
      return
    }
    let disposed = false
    void getModelCapabilities(draft.adapterId, draft.defaultModel || '')
      .then((next) => {
        if (!disposed) setCapabilities(next)
      })
      .catch(() => {
        if (!disposed) setCapabilities(null)
      })
    return () => {
      disposed = true
    }
  }, [draft?.adapterId, draft?.defaultModel])

  const addProvider = () => {
    const profile = createProviderProfile()
    setSelectedId(profile.id)
    setDraft(profile)
    setHeadersText('')
    setApiKey('')
    setModels([])
    setModelError(null)
    setSaved(false)
  }

  const updateAdapter = (adapterId: ProviderAdapterId) => {
    setDraft((current) => current ? {
      ...current,
      adapterId,
      apiBase: defaultBaseForAdapter(adapterId),
      headers: {},
      defaultModel: ''
    } : current)
    setApiKey('')
    setHeadersText('')
    setModels([])
    setModelError(null)
    setSaved(false)
  }

  const loadModels = async () => {
    if (!draft) return
    if (!draft.apiBase.trim()) {
      setModelError('先填写 API 地址')
      return
    }
    setLoadingModels(true)
    setModelError(null)
    try {
      const runtimeDraft = { ...draft, headers: parseHeaders(headersText) }
      const runtime = await providerToAiSettings(runtimeDraft, {
        ...config,
        ui: { ...config.ui, targetLanguage, alternateLanguage }
      }, apiKey)
      const next = await invoke<string[]>('list_models', { provider: toProviderConfig(runtime) })
      setModels(next)
      setModelMenuOpen(next.length > 0)
      if (!draft.defaultModel && next.length > 0) {
        setDraft((current) => current ? { ...current, defaultModel: next[0] } : current)
      }
      if (next.length === 0) setModelError('没有返回可用模型')
    } catch (error) {
      setModelError(String(error))
    } finally {
      setLoadingModels(false)
    }
  }

  const saveDraft = async () => {
    if (!draft) return
    if (!draft.name.trim()) {
      setModelError('渠道名称不能为空')
      return
    }
    if (!draft.apiBase.trim()) {
      setModelError('API 地址不能为空')
      return
    }
    if (!draft.defaultModel?.trim()) {
      setModelError('模型不能为空')
      return
    }

    setSaving(true)
    setModelError(null)
    setSaved(false)
    try {
      const normalized: ProviderProfile = {
        ...draft,
        name: draft.name.trim(),
        apiBase: draft.apiBase.trim(),
        headers: parseHeaders(headersText),
        defaultModel: draft.defaultModel.trim(),
        credentialId: draft.credentialId || draft.id
      }
      await saveProviderApiKey(normalized.credentialId || normalized.id, apiKey)

      const exists = config.providers.some((profile) => profile.id === normalized.id)
      const providers = exists
        ? config.providers.map((profile) => profile.id === normalized.id ? normalized : profile)
        : [...config.providers, normalized]
      const nextConfig: AppConfig = {
        ...config,
        providers,
        ui: {
          ...config.ui,
          targetLanguage,
          alternateLanguage,
          activeProviderId: normalized.id
        }
      }
      const persisted = await replaceAppConfig(nextConfig)
      onConfigChange(persisted)
      setDraft(normalized)
      setSelectedId(normalized.id)
      await onApply(providerToDisplaySettings(normalized, persisted))
      setSaved(true)
      window.setTimeout(() => setSaved(false), 1400)
    } catch (error) {
      setModelError(String(error))
    } finally {
      setSaving(false)
    }
  }

  const deleteSelected = async () => {
    if (!draft) return
    const persistedProfile = config.providers.find((profile) => profile.id === draft.id)
    if (!persistedProfile) {
      const next = config.providers.find((profile) => profile.id === config.ui.activeProviderId) || config.providers[0]
      if (next) await selectProvider(next)
      else addProvider()
      return
    }

    setSaving(true)
    setModelError(null)
    try {
      const nextConfig = removeProviderReferences(config, draft.id)
      const persisted = await replaceAppConfig(nextConfig)
      await saveProviderApiKey(persistedProfile.credentialId || persistedProfile.id, '').catch(() => undefined)
      onConfigChange(persisted)

      const nextId = persisted.ui.activeProviderId || persisted.providers[0]?.id
      const next = persisted.providers.find((profile) => profile.id === nextId)
      if (next) {
        await selectProvider(next)
        await onApply(providerToDisplaySettings(next, persisted))
      } else {
        addProvider()
        await onApply({
          adapter: 'openai-compatible',
          apiBase: defaultBaseForAdapter('openai-chat'),
          apiKey: '',
          model: '',
          headers: {},
          targetLanguage: persisted.ui.targetLanguage,
          alternateLanguage: persisted.ui.alternateLanguage
        })
      }
    } catch (error) {
      setModelError(String(error))
    } finally {
      setSaving(false)
    }
  }

  const modelQuery = draft?.defaultModel?.trim().toLowerCase() || ''
  const selectedModelIsFetched = Boolean(modelQuery && models.some((model) => model.toLowerCase() === modelQuery))
  const visibleModels = modelQuery && !selectedModelIsFetched
    ? models.filter((model) => model.toLowerCase().includes(modelQuery))
    : models

  return (
    <div className="provider-settings-layout">
      <aside className="provider-list">
        <div className="provider-list-head">
          <span>渠道</span>
          <button type="button" aria-label="添加渠道" title="添加渠道" onClick={addProvider}>
            <Plus size={14} strokeWidth={1.8} />
          </button>
        </div>
        <div className="provider-list-items">
          {config.providers.map((profile) => (
            <button
              type="button"
              key={profile.id}
              className={`provider-list-item${selectedId === profile.id ? ' selected' : ''}`}
              onClick={() => void selectProvider(profile)}>
              <span className={`provider-dot${config.ui.activeProviderId === profile.id ? ' active' : ''}`} />
              <span className="provider-list-copy">
                <strong>{profile.name}</strong>
                <small>{profile.defaultModel || '未选择模型'}</small>
              </span>
            </button>
          ))}
          {config.providers.length === 0 && <div className="provider-list-empty">还没有渠道</div>}
        </div>
      </aside>

      <div className="provider-editor">
        {draft ? (
          <>
            <div className="settings-grid provider-grid">
              <label className="wide">
                <span>渠道名称</span>
                <input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} />
              </label>

              <label>
                <span>协议</span>
                <select value={draft.adapterId} onChange={(event) => updateAdapter(event.target.value as ProviderAdapterId)}>
                  {Object.entries(ADAPTER_LABELS).map(([id, label]) => <option key={id} value={id}>{label}</option>)}
                </select>
              </label>

              <label className="wide">
                <span>API 地址</span>
                <input value={draft.apiBase} spellCheck={false} onChange={(event) => setDraft({ ...draft, apiBase: event.target.value })} />
              </label>

              <label className="wide">
                <span>API Key</span>
                <div className="input-with-action">
                  <input
                    type={showKey ? 'text' : 'password'}
                    value={apiKey}
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="本地服务可留空"
                    onChange={(event) => setApiKey(event.target.value)}
                  />
                  <button type="button" aria-label={showKey ? '隐藏 Key' : '显示 Key'} title={showKey ? '隐藏 Key' : '显示 Key'} onClick={() => setShowKey((current) => !current)}>
                    {showKey ? <EyeOff size={14} strokeWidth={1.8} /> : <Eye size={14} strokeWidth={1.8} />}
                  </button>
                </div>
                {!apiKey.trim() && (
                  <small className="provider-key-warning">当前渠道没有保存 API Key。Groq / OpenAI / Anthropic 等需要鉴权的服务会直接调用失败。</small>
                )}
              </label>

              <label className="wide">
                <span>默认模型</span>
                <div className="model-picker">
                  <div className="input-with-action">
                    <input
                      value={draft.defaultModel || ''}
                      spellCheck={false}
                      placeholder="输入模型名或拉取"
                      onFocus={() => {
                        if (models.length > 0) setModelMenuOpen(true)
                      }}
                      onChange={(event) => {
                        setDraft({ ...draft, defaultModel: event.target.value })
                        if (models.length > 0) setModelMenuOpen(true)
                      }}
                    />
                    <button
                      type="button"
                      aria-label="展开模型列表"
                      title={models.length > 0 ? `查看全部 ${models.length} 个模型` : '先拉取模型'}
                      aria-expanded={modelMenuOpen}
                      disabled={models.length === 0}
                      onClick={() => setModelMenuOpen((current) => !current)}>
                      <ChevronDown size={14} strokeWidth={1.8} className={modelMenuOpen ? 'expanded' : undefined} />
                    </button>
                    <button type="button" aria-label="拉取模型" title="拉取模型" disabled={loadingModels} onClick={() => void loadModels()}>
                      <RefreshCw size={14} strokeWidth={1.8} className={loadingModels ? 'spin' : undefined} />
                    </button>
                  </div>
                  {modelMenuOpen && models.length > 0 && (
                    <div className="model-picker-menu" role="listbox" aria-label="可用模型">
                      <div className="model-picker-menu-head">
                        <span>可用模型</span>
                        <small>{visibleModels.length === models.length ? `${models.length} 个` : `${visibleModels.length} / ${models.length}`}</small>
                      </div>
                      <div className="model-picker-options">
                        {visibleModels.map((model) => (
                          <button
                            type="button"
                            role="option"
                            aria-selected={model === draft.defaultModel}
                            className={model === draft.defaultModel ? 'selected' : undefined}
                            key={model}
                            onClick={() => {
                              setDraft({ ...draft, defaultModel: model })
                              setModelMenuOpen(false)
                            }}>
                            {model}
                          </button>
                        ))}
                        {visibleModels.length === 0 && <div className="model-picker-empty">没有匹配的模型</div>}
                      </div>
                    </div>
                  )}
                </div>
              </label>

              <label>
                <span>优先语言</span>
                <select value={targetLanguage} onChange={(event) => setTargetLanguage(event.target.value)}>
                  {LANGUAGE_OPTIONS.map((language) => <option value={language} key={language}>{language}</option>)}
                </select>
              </label>

              <label>
                <span>反向语言</span>
                <select value={alternateLanguage} onChange={(event) => setAlternateLanguage(event.target.value)}>
                  {LANGUAGE_OPTIONS.map((language) => <option value={language} key={language}>{language}</option>)}
                </select>
              </label>
            </div>

            {capabilities && (
              <div className="provider-capability-row">
                <span className={capabilities.temperature ? 'on' : 'off'}>Temperature</span>
                <span className={capabilities.topP ? 'on' : 'off'}>Top P</span>
                <span className={capabilities.maxOutputTokens ? 'on' : 'off'}>Max Tokens</span>
                <span className={capabilities.reasoning ? 'on' : 'off'}>Reasoning</span>
              </div>
            )}

            <button
              type="button"
              className={defaultsOpen ? 'provider-defaults-toggle active' : 'provider-defaults-toggle'}
              onClick={() => setDefaultsOpen((current) => !current)}>
              <SlidersHorizontal size={13} strokeWidth={1.8} />
              <span>默认生成参数</span>
              <small>动作未覆盖时继承</small>
            </button>

            {defaultsOpen && (
              <div className="provider-defaults-grid">
                <label>
                  <span>Temperature</span>
                  <input
                    type="number"
                    min="0"
                    max="2"
                    step="0.1"
                    value={draft.defaults.temperature ?? ''}
                    disabled={capabilities?.temperature === false}
                    placeholder={capabilities?.temperature === false ? '不支持' : '模型默认'}
                    onChange={(event) => setDraft({
                      ...draft,
                      defaults: { ...draft.defaults, temperature: optionalNumber(event.target.value) }
                    })}
                  />
                </label>
                <label>
                  <span>Top P</span>
                  <input
                    type="number"
                    min="0"
                    max="1"
                    step="0.05"
                    value={draft.defaults.topP ?? ''}
                    disabled={capabilities?.topP === false}
                    placeholder={capabilities?.topP === false ? '不支持' : '模型默认'}
                    onChange={(event) => setDraft({
                      ...draft,
                      defaults: { ...draft.defaults, topP: optionalNumber(event.target.value) }
                    })}
                  />
                </label>
                <label>
                  <span>Max Tokens</span>
                  <input
                    type="number"
                    min="1"
                    step="1"
                    value={draft.defaults.maxOutputTokens ?? ''}
                    disabled={capabilities?.maxOutputTokens === false}
                    placeholder={capabilities?.maxOutputTokens === false ? '不支持' : '模型默认'}
                    onChange={(event) => setDraft({
                      ...draft,
                      defaults: { ...draft.defaults, maxOutputTokens: optionalInteger(event.target.value) }
                    })}
                  />
                </label>
                <label>
                  <span>Reasoning</span>
                  <select
                    value={draft.defaults.reasoningMode ?? ''}
                    disabled={capabilities?.reasoning === false}
                    onChange={(event) => setDraft({
                      ...draft,
                      defaults: { ...draft.defaults, reasoningMode: event.target.value || undefined }
                    })}>
                    <option value="">模型默认</option>
                    <option value="auto">Auto</option>
                    <option value="none">关闭 / 最低</option>
                    <option value="minimal">Minimal</option>
                    <option value="low">Low</option>
                    <option value="medium">Medium</option>
                    <option value="high">High</option>
                    <option value="xhigh">XHigh</option>
                    <option value="max">Max</option>
                  </select>
                </label>
              </div>
            )}

            <button
              type="button"
              className={headersOpen ? 'provider-defaults-toggle active' : 'provider-defaults-toggle'}
              onClick={() => setHeadersOpen((current) => !current)}>
              <Braces size={13} strokeWidth={1.8} />
              <span>自定义 Headers</span>
              <small>高级</small>
            </button>

            {headersOpen && (
              <div className="provider-headers-editor">
                <textarea
                  value={headersText}
                  spellCheck={false}
                  placeholder={'HTTP-Referer: https://example.com\nX-Title: ZhiForge'}
                  onChange={(event) => setHeadersText(event.target.value)}
                />
                <small>每行一个 Name: Value。Authorization / API Key 等认证头由程序管理，不允许在这里覆盖。</small>
              </div>
            )}

            {modelError && <div className="settings-error">{modelError}</div>}

            <div className="settings-footer provider-settings-footer">
              <button type="button" className="provider-delete" disabled={saving} onClick={() => void deleteSelected()}>
                <Trash2 size={13} strokeWidth={1.8} />
                <span>删除</span>
              </button>
              <button type="button" className="settings-save" disabled={saving} onClick={() => void saveDraft()}>
                {saved ? <Check size={14} strokeWidth={1.8} /> : <Save size={14} strokeWidth={1.8} />}
                <span>{saving ? '保存中' : saved ? '已保存' : '保存并应用'}</span>
              </button>
            </div>
          </>
        ) : (
          <div className="provider-editor-empty">读取设置…</div>
        )}
      </div>
    </div>
  )
}
