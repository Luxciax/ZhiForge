import {
  ArrowDown,
  ArrowUp,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  SlidersHorizontal,
  Trash2
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import type { ActionDefinition, AppConfig, GenerationOptions, ModelRef } from '../../domain/settings'
import {
  createCustomAction,
  deleteCustomAction,
  moveAction,
  setActionEnabled,
  updateActionDefinition
} from '../../services/action-service'
import { clearActionRoute, routeForAction, saveActionRoute } from '../../services/route-service'

interface ActionRoutesSettingsProps {
  config: AppConfig
  onConfigChange: (config: AppConfig) => void
}

interface RouteDraft {
  providerId: string
  model: string
  fallbacks: ModelRef[]
  options: GenerationOptions
}

interface DefinitionDraft {
  name: string
  prompt: string
  targetLanguage: string
  alternateLanguage: string
  showInToolbar: boolean
  outputMode: ActionDefinition['outputMode']
}

const FOLLOW_ACTIVE = '__active__'

function optionalNumber(value: string): number | undefined {
  if (!value.trim()) return undefined
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : undefined
}

function optionalInteger(value: string): number | undefined {
  const parsed = optionalNumber(value)
  return parsed === undefined ? undefined : Math.trunc(parsed)
}

export function ActionRoutesSettings({ config, onConfigChange }: ActionRoutesSettingsProps) {
  const actions = useMemo(
    () => [...config.actions].sort((a, b) => a.order - b.order),
    [config.actions]
  )
  const enabledProviders = useMemo(
    () => config.providers.filter((provider) => provider.enabled),
    [config.providers]
  )
  const [drafts, setDrafts] = useState<Record<string, RouteDraft>>({})
  const [definitionDrafts, setDefinitionDrafts] = useState<Record<string, DefinitionDraft>>({})
  const [expandedDefinitions, setExpandedDefinitions] = useState<Set<string>>(() => new Set())
  const [expandedOptions, setExpandedOptions] = useState<Set<string>>(() => new Set())
  const [savingAction, setSavingAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const nextRoutes: Record<string, RouteDraft> = {}
    const nextDefinitions: Record<string, DefinitionDraft> = {}
    for (const action of actions) {
      const route = routeForAction(config, action.id)
      nextRoutes[action.id] = route
        ? {
            providerId: route.primary.providerId,
            model: route.primary.model,
            fallbacks: (route.fallbacks || []).map((fallback) => ({ ...fallback })),
            options: { ...(route.options || {}) }
          }
        : { providerId: FOLLOW_ACTIVE, model: '', fallbacks: [], options: {} }
      nextDefinitions[action.id] = {
        name: action.name,
        prompt: action.prompt || '',
        targetLanguage: action.targetLanguage || '',
        alternateLanguage: action.alternateLanguage || '',
        showInToolbar: action.showInToolbar,
        outputMode: action.outputMode
      }
    }
    setDrafts(nextRoutes)
    setDefinitionDrafts(nextDefinitions)
  }, [actions, config])

  const createAction = async () => {
    setSavingAction('__new__')
    setError(null)
    try {
      const created = await createCustomAction(config)
      onConfigChange(created.config)
      setExpandedDefinitions((current) => new Set(current).add(created.actionId))
    } catch (createError) {
      setError(String(createError))
    } finally {
      setSavingAction(null)
    }
  }

  const updateProvider = async (actionId: string, providerId: string) => {
    setError(null)
    if (providerId === FOLLOW_ACTIVE) {
      setSavingAction(actionId)
      try {
        const next = await clearActionRoute(config, actionId)
        onConfigChange(next)
        setExpandedOptions((current) => {
          const nextExpanded = new Set(current)
          nextExpanded.delete(actionId)
          return nextExpanded
        })
      } catch (routeError) {
        setError(String(routeError))
      } finally {
        setSavingAction(null)
      }
      return
    }

    const provider = config.providers.find((item) => item.id === providerId)
    setDrafts((current) => ({
      ...current,
      [actionId]: {
        ...(current[actionId] || { fallbacks: [], options: {} }),
        providerId,
        model: provider?.defaultModel || ''
      }
    }))
  }

  const toggleAction = async (actionId: string, enabled: boolean) => {
    setSavingAction(actionId)
    setError(null)
    try {
      onConfigChange(await setActionEnabled(config, actionId, enabled))
    } catch (toggleError) {
      setError(String(toggleError))
    } finally {
      setSavingAction(null)
    }
  }

  const saveDefinition = async (actionId: string) => {
    const draft = definitionDrafts[actionId]
    if (!draft) return
    setSavingAction(actionId)
    setError(null)
    try {
      const next = await updateActionDefinition(config, actionId, {
        name: draft.name,
        prompt: draft.prompt,
        targetLanguage: draft.targetLanguage,
        alternateLanguage: draft.alternateLanguage,
        showInToolbar: draft.showInToolbar,
        outputMode: draft.outputMode
      })
      onConfigChange(next)
    } catch (definitionError) {
      setError(String(definitionError))
    } finally {
      setSavingAction(null)
    }
  }

  const deleteAction = async (action: ActionDefinition) => {
    if (action.builtin) return
    if (!window.confirm(`删除动作“${action.name}”？它的模型路由也会一并删除。`)) return
    setSavingAction(action.id)
    setError(null)
    try {
      const next = await deleteCustomAction(config, action.id)
      onConfigChange(next)
      setExpandedDefinitions((current) => {
        const copy = new Set(current)
        copy.delete(action.id)
        return copy
      })
    } catch (deleteError) {
      setError(String(deleteError))
    } finally {
      setSavingAction(null)
    }
  }

  const reorderAction = async (actionId: string, direction: -1 | 1) => {
    setSavingAction(actionId)
    setError(null)
    try {
      onConfigChange(await moveAction(config, actionId, direction))
    } catch (moveError) {
      setError(String(moveError))
    } finally {
      setSavingAction(null)
    }
  }

  const toggleDefinition = (actionId: string) => {
    setExpandedDefinitions((current) => {
      const next = new Set(current)
      if (next.has(actionId)) next.delete(actionId)
      else next.add(actionId)
      return next
    })
  }

  const updateDefinitionDraft = (actionId: string, patch: Partial<DefinitionDraft>) => {
    setDefinitionDrafts((current) => ({
      ...current,
      [actionId]: { ...current[actionId], ...patch }
    }))
  }

  const addFallback = (actionId: string) => {
    setDrafts((current) => {
      const draft = current[actionId]
      if (!draft || draft.providerId === FOLLOW_ACTIVE || enabledProviders.length === 0) return current
      const alternate = enabledProviders.find((provider) => provider.id !== draft.providerId)
      const provider = alternate || enabledProviders[0]
      const candidateModel = provider.defaultModel || ''
      const duplicatePrimary = provider.id === draft.providerId && candidateModel === draft.model
      return {
        ...current,
        [actionId]: {
          ...draft,
          fallbacks: [
            ...draft.fallbacks,
            { providerId: provider.id, model: duplicatePrimary ? '' : candidateModel }
          ]
        }
      }
    })
  }

  const updateFallback = (actionId: string, index: number, patch: Partial<ModelRef>) => {
    setDrafts((current) => {
      const draft = current[actionId]
      if (!draft) return current
      return {
        ...current,
        [actionId]: {
          ...draft,
          fallbacks: draft.fallbacks.map((fallback, fallbackIndex) =>
            fallbackIndex === index ? { ...fallback, ...patch } : fallback
          )
        }
      }
    })
  }

  const updateFallbackProvider = (actionId: string, index: number, providerId: string) => {
    const provider = config.providers.find((item) => item.id === providerId)
    updateFallback(actionId, index, { providerId, model: provider?.defaultModel || '' })
  }

  const removeFallback = (actionId: string, index: number) => {
    setDrafts((current) => {
      const draft = current[actionId]
      if (!draft) return current
      return {
        ...current,
        [actionId]: {
          ...draft,
          fallbacks: draft.fallbacks.filter((_, fallbackIndex) => fallbackIndex !== index)
        }
      }
    })
  }

  const updateOptions = (actionId: string, patch: Partial<GenerationOptions>) => {
    setDrafts((current) => {
      const draft = current[actionId]
      if (!draft) return current
      return {
        ...current,
        [actionId]: { ...draft, options: { ...draft.options, ...patch } }
      }
    })
  }

  const toggleOptions = (actionId: string) => {
    setExpandedOptions((current) => {
      const next = new Set(current)
      if (next.has(actionId)) next.delete(actionId)
      else next.add(actionId)
      return next
    })
  }

  const saveRoute = async (actionId: string) => {
    const draft = drafts[actionId]
    if (!draft || draft.providerId === FOLLOW_ACTIVE) return
    setSavingAction(actionId)
    setError(null)
    try {
      const next = await saveActionRoute(
        config,
        actionId,
        { providerId: draft.providerId, model: draft.model },
        draft.fallbacks,
        draft.options
      )
      onConfigChange(next)
    } catch (routeError) {
      setError(String(routeError))
    } finally {
      setSavingAction(null)
    }
  }

  return (
    <div className="action-routes-editor">
      <div className="action-routes-head">
        <span>动作按顺序显示在划词工具条；可单独指定模型、备用模型、Prompt 和生成参数</span>
        <button type="button" aria-label="添加自定义动作" title="添加自定义动作" disabled={savingAction === '__new__'} onClick={() => void createAction()}>
          <Plus size={13} strokeWidth={1.8} />
        </button>
      </div>

      <div className="action-route-list">
        {actions.map((action, actionIndex) => {
          const draft = drafts[action.id] || { providerId: FOLLOW_ACTIVE, model: '', fallbacks: [], options: {} }
          const definition = definitionDrafts[action.id] || {
            name: action.name,
            prompt: action.prompt || '',
            targetLanguage: action.targetLanguage || '',
            alternateLanguage: action.alternateLanguage || '',
            showInToolbar: action.showInToolbar,
            outputMode: action.outputMode
          }
          const followsActive = draft.providerId === FOLLOW_ACTIVE
          const fallbackIncomplete = draft.fallbacks.some((fallback) => !fallback.providerId || !fallback.model.trim())
          const optionsOpen = expandedOptions.has(action.id)
          const definitionOpen = expandedDefinitions.has(action.id)
          return (
            <div className={`action-route-item${action.enabled ? '' : ' disabled'}`} key={action.id}>
              <div className="action-route-row">
                <label className="action-route-name">
                  <input
                    type="checkbox"
                    checked={action.enabled}
                    disabled={savingAction === action.id}
                    onChange={(event) => void toggleAction(action.id, event.target.checked)}
                  />
                  <span title={action.name}>{action.name}</span>
                </label>
                <select
                  value={draft.providerId}
                  disabled={!action.enabled || savingAction === action.id}
                  onChange={(event) => void updateProvider(action.id, event.target.value)}>
                  <option value={FOLLOW_ACTIVE}>跟随当前渠道</option>
                  {enabledProviders.map((provider) => (
                    <option value={provider.id} key={provider.id}>{provider.name}</option>
                  ))}
                </select>
                <input
                  value={draft.model}
                  disabled={!action.enabled || followsActive || savingAction === action.id}
                  spellCheck={false}
                  placeholder={followsActive ? '使用当前渠道默认模型' : '模型'}
                  onChange={(event) => setDrafts((current) => ({
                    ...current,
                    [action.id]: { ...draft, model: event.target.value }
                  }))}
                />
                <div className="action-route-controls">
                  <button
                    type="button"
                    className={definitionOpen ? 'action-route-edit active' : 'action-route-edit'}
                    aria-label={`编辑${action.name}`}
                    title="动作与 Prompt"
                    onClick={() => toggleDefinition(action.id)}>
                    <Pencil size={12} strokeWidth={1.8} />
                  </button>
                  <button
                    type="button"
                    className="action-route-save"
                    aria-label={followsActive ? '跟随当前渠道' : `保存${action.name}路由`}
                    title={followsActive ? '跟随当前渠道' : '保存动作路由'}
                    disabled={!action.enabled || followsActive || savingAction === action.id || !draft.model.trim() || fallbackIncomplete}
                    onClick={() => void saveRoute(action.id)}>
                    {followsActive ? <RotateCcw size={13} strokeWidth={1.8} /> : <Save size={13} strokeWidth={1.8} />}
                  </button>
                  <button
                    type="button"
                    className="action-route-add"
                    aria-label={`为${action.name}添加备用模型`}
                    title="添加备用模型"
                    disabled={!action.enabled || followsActive || savingAction === action.id || enabledProviders.length === 0}
                    onClick={() => addFallback(action.id)}>
                    <Plus size={13} strokeWidth={1.8} />
                  </button>
                  <button
                    type="button"
                    className={optionsOpen ? 'action-route-options-toggle active' : 'action-route-options-toggle'}
                    aria-label={`${action.name}生成参数`}
                    title="生成参数"
                    disabled={!action.enabled || followsActive || savingAction === action.id}
                    onClick={() => toggleOptions(action.id)}>
                    <SlidersHorizontal size={13} strokeWidth={1.8} />
                  </button>
                </div>
              </div>

              {definitionOpen && (
                <div className="action-definition-card">
                  <div className="action-definition-grid">
                    <label className="action-definition-name">
                      <span>名称</span>
                      <input value={definition.name} maxLength={64} onChange={(event) => updateDefinitionDraft(action.id, { name: event.target.value })} />
                    </label>
                    <label>
                      <span>输出</span>
                      <select value={definition.outputMode} onChange={(event) => updateDefinitionDraft(action.id, { outputMode: event.target.value as ActionDefinition['outputMode'] })}>
                        <option value="markdown">Markdown</option>
                        <option value="plain-text">纯文本</option>
                      </select>
                    </label>
                    <label className="action-toolbar-toggle">
                      <input type="checkbox" checked={definition.showInToolbar} onChange={(event) => updateDefinitionDraft(action.id, { showInToolbar: event.target.checked })} />
                      <span>显示在划词工具条</span>
                    </label>
                  </div>
                  <div className="action-language-grid">
                    <label>
                      <span>目标语言</span>
                      <input
                        value={definition.targetLanguage}
                        maxLength={64}
                        placeholder={`继承全局 · ${config.ui.targetLanguage}`}
                        onChange={(event) => updateDefinitionDraft(action.id, { targetLanguage: event.target.value })}
                      />
                    </label>
                    <label>
                      <span>反向语言</span>
                      <input
                        value={definition.alternateLanguage}
                        maxLength={64}
                        placeholder={`继承全局 · ${config.ui.alternateLanguage}`}
                        onChange={(event) => updateDefinitionDraft(action.id, { alternateLanguage: event.target.value })}
                      />
                    </label>
                  </div>
                  <label className="action-prompt-field">
                    <span>Prompt {action.builtin && <small>留空使用内置模板</small>}</span>
                    <textarea
                      value={definition.prompt}
                      placeholder={action.builtin ? '留空使用内置 Prompt' : '告诉模型如何处理选中的文本'}
                      onChange={(event) => updateDefinitionDraft(action.id, { prompt: event.target.value })}
                    />
                  </label>
                  <div className="action-prompt-help">
                    可用 {'{target_language}'}、{'{alternate_language}'}、{'{action_name}'}。选中文本始终作为独立不可信输入，不会插入 Prompt 模板。
                  </div>
                  <div className="action-definition-footer">
                    <div>
                      <button type="button" aria-label="上移" title="上移" disabled={actionIndex === 0 || savingAction === action.id} onClick={() => void reorderAction(action.id, -1)}><ArrowUp size={12} strokeWidth={1.8} /></button>
                      <button type="button" aria-label="下移" title="下移" disabled={actionIndex === actions.length - 1 || savingAction === action.id} onClick={() => void reorderAction(action.id, 1)}><ArrowDown size={12} strokeWidth={1.8} /></button>
                      {!action.builtin && <button className="danger" type="button" aria-label="删除动作" title="删除动作" disabled={savingAction === action.id} onClick={() => void deleteAction(action)}><Trash2 size={12} strokeWidth={1.8} /></button>}
                    </div>
                    <button className="action-definition-save" type="button" disabled={savingAction === action.id} onClick={() => void saveDefinition(action.id)}><Save size={12} strokeWidth={1.8} /><span>保存动作</span></button>
                  </div>
                </div>
              )}

              {optionsOpen && !followsActive && (
                <div className="action-options-row">
                  <label><span>Temperature</span><input type="number" min="0" max="2" step="0.1" value={draft.options.temperature ?? ''} placeholder="继承" onChange={(event) => updateOptions(action.id, { temperature: optionalNumber(event.target.value) })} /></label>
                  <label><span>Top P</span><input type="number" min="0" max="1" step="0.05" value={draft.options.topP ?? ''} placeholder="继承" onChange={(event) => updateOptions(action.id, { topP: optionalNumber(event.target.value) })} /></label>
                  <label><span>Max Tokens</span><input type="number" min="1" step="1" value={draft.options.maxOutputTokens ?? ''} placeholder="继承" onChange={(event) => updateOptions(action.id, { maxOutputTokens: optionalInteger(event.target.value) })} /></label>
                  <label>
                    <span>Reasoning</span>
                    <select value={draft.options.reasoningMode ?? ''} onChange={(event) => updateOptions(action.id, { reasoningMode: event.target.value || undefined })}>
                      <option value="">继承</option><option value="auto">模型默认</option><option value="none">关闭 / 最低</option><option value="minimal">Minimal</option><option value="low">Low</option><option value="medium">Medium</option><option value="high">High</option><option value="xhigh">XHigh</option><option value="max">Max</option>
                    </select>
                  </label>
                </div>
              )}

              {draft.fallbacks.length > 0 && (
                <div className="action-fallback-list">
                  {draft.fallbacks.map((fallback, index) => (
                    <div className="action-fallback-row" key={`${action.id}-fallback-${index}`}>
                      <span className="action-fallback-label">备用 {index + 1}</span>
                      <select value={fallback.providerId} disabled={!action.enabled || savingAction === action.id} onChange={(event) => updateFallbackProvider(action.id, index, event.target.value)}>
                        {enabledProviders.map((provider) => <option value={provider.id} key={provider.id}>{provider.name}</option>)}
                      </select>
                      <input value={fallback.model} disabled={!action.enabled || savingAction === action.id} spellCheck={false} placeholder="备用模型" onChange={(event) => updateFallback(action.id, index, { model: event.target.value })} />
                      <button type="button" aria-label={`删除备用模型 ${index + 1}`} title="删除备用模型" disabled={savingAction === action.id} onClick={() => removeFallback(action.id, index)}><Trash2 size={12} strokeWidth={1.8} /></button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )
        })}
      </div>

      {error && <div className="settings-error">{error}</div>}
    </div>
  )
}
