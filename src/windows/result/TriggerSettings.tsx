import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Pause, Play, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { AppConfig, TriggerProfile } from '../../domain/settings'
import { replaceAppConfig } from '../../services/settings-service'

interface TriggerSettingsProps {
  config: AppConfig
  onConfigChange: (config: AppConfig) => void
}

export function TriggerSettings({ config, onConfigChange }: TriggerSettingsProps) {
  const [draft, setDraft] = useState<TriggerProfile>(config.trigger)
  const [paused, setPaused] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => setDraft(config.trigger), [config.trigger])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void invoke<boolean>('runtime_selection_paused')
      .then((value) => { if (!disposed) setPaused(value) })
      .catch(() => undefined)
    void listen<boolean>('runtime://pause-changed', (event) => {
      if (!disposed) setPaused(event.payload)
    }).then((cleanup) => {
      if (disposed) cleanup()
      else unlisten = cleanup
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  const togglePause = async () => {
    setError(null)
    try {
      const next = paused
        ? await invoke<boolean>('runtime_resume_selection')
        : await invoke<boolean>('runtime_pause_selection')
      setPaused(next)
    } catch (runtimeError) {
      setError(String(runtimeError))
    }
  }

  const save = async () => {
    setSaving(true)
    setError(null)
    try {
      const next = await replaceAppConfig({ ...config, trigger: draft })
      onConfigChange(next)
    } catch (saveError) {
      setError(String(saveError))
    } finally {
      setSaving(false)
    }
  }

  const toggle = (key: 'selectionEnabled' | 'dragEnabled' | 'doubleClickEnabled' | 'shiftClickEnabled') => {
    setDraft((current) => ({ ...current, [key]: !current[key] }))
  }

  return (
    <div className="trigger-settings-editor">
      <div className="runtime-control-row">
        <div>
          <strong>划词监听</strong>
          <small>{paused ? '当前已暂停' : '当前正在监听'}</small>
        </div>
        <button type="button" className={paused ? 'resume' : undefined} onClick={() => void togglePause()}>
          {paused ? <Play size={13} strokeWidth={1.8} /> : <Pause size={13} strokeWidth={1.8} />}
          <span>{paused ? '恢复' : '暂停'}</span>
        </button>
      </div>

      <div className="trigger-toggle-grid">
        <label>
          <input type="checkbox" checked={draft.selectionEnabled} onChange={() => toggle('selectionEnabled')} />
          <span>总开关</span>
        </label>
        <label>
          <input type="checkbox" checked={draft.dragEnabled} onChange={() => toggle('dragEnabled')} />
          <span>拖选</span>
        </label>
        <label>
          <input type="checkbox" checked={draft.doubleClickEnabled} onChange={() => toggle('doubleClickEnabled')} />
          <span>双击</span>
        </label>
        <label>
          <input type="checkbox" checked={draft.shiftClickEnabled} onChange={() => toggle('shiftClickEnabled')} />
          <span>Shift Click</span>
        </label>
      </div>

      <div className="trigger-fields">
        <label>
          <span>要求修饰键</span>
          <select value={draft.requiredModifier} onChange={(event) => setDraft({ ...draft, requiredModifier: event.target.value as TriggerProfile['requiredModifier'] })}>
            <option value="none">无</option>
            <option value="ctrl">Ctrl</option>
            <option value="alt">Alt</option>
            <option value="shift">Shift</option>
          </select>
        </label>
        <label>
          <span>最小拖选距离</span>
          <div className="number-field">
            <input type="number" min={1} max={64} value={draft.minDragDistance} onChange={(event) => setDraft({ ...draft, minDragDistance: Number(event.target.value) || 1 })} />
            <em>px</em>
          </div>
        </label>
        <label>
          <span>最长拖选时间</span>
          <div className="number-field">
            <input type="number" min={500} max={30000} step={500} value={draft.maxDragDurationMs} onChange={(event) => setDraft({ ...draft, maxDragDurationMs: Number(event.target.value) || 500 })} />
            <em>ms</em>
          </div>
        </label>
      </div>

      {error && <div className="settings-error">{error}</div>}
      <div className="settings-footer">
        <button type="button" className="settings-save" disabled={saving} onClick={() => void save()}>
          <Save size={13} strokeWidth={1.8} />
          <span>{saving ? '保存中' : '保存触发设置'}</span>
        </button>
      </div>
    </div>
  )
}
