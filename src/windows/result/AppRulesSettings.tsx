import { Plus, Save, Trash2 } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { AppConfig, AppRule } from '../../domain/settings'
import { replaceAppConfig } from '../../services/settings-service'

interface AppRulesSettingsProps {
  config: AppConfig
  onConfigChange: (config: AppConfig) => void
}

export function AppRulesSettings({ config, onConfigChange }: AppRulesSettingsProps) {
  const [rules, setRules] = useState<AppRule[]>(config.appRules)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => setRules(config.appRules), [config.appRules])

  const addRule = () => {
    setRules((current) => [
      ...current,
      {
        id: `app-rule-${crypto.randomUUID()}`,
        processPattern: '',
        behavior: 'disable',
        clipboardFallback: 'inherit'
      }
    ])
  }

  const updateRule = (id: string, patch: Partial<AppRule>) => {
    setRules((current) => current.map((rule) => rule.id === id ? { ...rule, ...patch } : rule))
  }

  const save = async () => {
    const normalized = rules
      .map((rule) => ({ ...rule, processPattern: rule.processPattern.trim().toLowerCase() }))
      .filter((rule) => rule.processPattern.length > 0)
    setSaving(true)
    setError(null)
    try {
      const next = await replaceAppConfig({ ...config, appRules: normalized })
      setRules(next.appRules)
      onConfigChange(next)
    } catch (saveError) {
      setError(String(saveError))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="app-rules-editor">
      <div className="app-rules-head">
        <span>应用规则</span>
        <button type="button" aria-label="添加应用规则" title="添加应用规则" onClick={addRule}>
          <Plus size={13} strokeWidth={1.8} />
        </button>
      </div>

      <div className="app-rule-list">
        {rules.length === 0 && <div className="app-rules-empty">未添加自定义规则，使用内置兼容规则。</div>}
        {rules.map((rule) => (
          <div className="app-rule-row" key={rule.id}>
            <input
              value={rule.processPattern}
              spellCheck={false}
              placeholder="例如 excel.exe 或 *game*"
              onChange={(event) => updateRule(rule.id, { processPattern: event.target.value })}
            />
            <select value={rule.behavior} title="划词行为" onChange={(event) => updateRule(rule.id, { behavior: event.target.value as AppRule['behavior'] })}>
              <option value="enable">启用</option>
              <option value="disable">禁用</option>
              <option value="inherit">默认</option>
            </select>
            <select value={rule.clipboardFallback} title="剪贴板回退" onChange={(event) => updateRule(rule.id, { clipboardFallback: event.target.value as AppRule['clipboardFallback'] })}>
              <option value="inherit">剪贴板·自动</option>
              <option value="allow">剪贴板·允许</option>
              <option value="deny">剪贴板·禁止</option>
            </select>
            <button type="button" aria-label="删除规则" title="删除规则" onClick={() => setRules((current) => current.filter((item) => item.id !== rule.id))}>
              <Trash2 size={13} strokeWidth={1.8} />
            </button>
          </div>
        ))}
      </div>

      {error && <div className="settings-error">{error}</div>}
      <div className="settings-footer">
        <button type="button" className="settings-save" disabled={saving} onClick={() => void save()}>
          <Save size={13} strokeWidth={1.8} />
          <span>{saving ? '保存中' : '保存应用规则'}</span>
        </button>
      </div>
    </div>
  )
}
