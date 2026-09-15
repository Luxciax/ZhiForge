import { Check, ExternalLink, KeyRound, LoaderCircle, Trash2 } from 'lucide-react'
import { useEffect, useState } from 'react'
import { openSourceUrl } from '../../services/knowledge-service'
import { isZhihuAccessSecretConfigured, saveZhihuAccessSecret } from '../../services/zhihu-service'

export function ZhihuSettings() {
  const [configured, setConfigured] = useState(false)
  const [secret, setSecret] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  const refresh = async () => {
    setLoading(true)
    try {
      setConfigured(await isZhihuAccessSecretConfigured())
      setError(null)
    } catch (readError) {
      setError(String(readError))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void refresh()
  }, [])

  const save = async () => {
    const value = secret.trim()
    if (!value) return
    setSaving(true)
    setError(null)
    setSaved(false)
    try {
      await saveZhihuAccessSecret(value)
      setSecret('')
      setConfigured(true)
      setSaved(true)
      window.setTimeout(() => setSaved(false), 1400)
    } catch (saveError) {
      setError(String(saveError))
    } finally {
      setSaving(false)
    }
  }

  const clear = async () => {
    setSaving(true)
    setError(null)
    try {
      await saveZhihuAccessSecret('')
      setConfigured(false)
      setSecret('')
    } catch (clearError) {
      setError(String(clearError))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="zhihu-settings-editor">
      <section className="zhihu-settings-card">
        <div className="zhihu-settings-title">
          <span className="general-setting-icon"><KeyRound size={16} strokeWidth={1.8} /></span>
          <div>
            <strong>知乎开放平台</strong>
            <span>{loading ? '正在检查凭证…' : configured ? 'Access Secret 已安全保存' : '尚未配置 Access Secret'}</span>
          </div>
          {configured && <span className="zhihu-connected"><Check size={13} strokeWidth={2} />已连接</span>}
        </div>

        <label className="zhihu-secret-field">
          <span>Access Secret</span>
          <input
            type="password"
            autoComplete="off"
            value={secret}
            placeholder={configured ? '输入新 Secret 可覆盖当前凭证' : '输入知乎开放平台 Access Secret'}
            onChange={(event) => setSecret(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void save()
            }}
          />
        </label>

        <div className="zhihu-settings-actions">
          <button type="button" className="settings-save" disabled={saving || !secret.trim()} onClick={() => void save()}>
            {saving ? <LoaderCircle size={14} strokeWidth={1.8} className="spin" /> : saved ? <Check size={14} strokeWidth={1.8} /> : <KeyRound size={14} strokeWidth={1.8} />}
            <span>{saved ? '已保存' : '保存凭证'}</span>
          </button>
          {configured && (
            <button type="button" className="zhihu-clear-secret" disabled={saving} onClick={() => void clear()}>
              <Trash2 size={13} strokeWidth={1.8} />
              <span>清除</span>
            </button>
          )}
          <button type="button" className="zhihu-open-platform" onClick={() => void openSourceUrl('https://developer.zhihu.com/profile')}>
            <ExternalLink size={13} strokeWidth={1.8} />
            <span>开放平台</span>
          </button>
        </div>
      </section>
      <p className="zhihu-settings-note">凭证只保存在 Windows Credential Manager，不写入 config.json，也不会进入诊断日志。搜索请求直接发送到知乎开放平台。</p>
      {error && <div className="settings-error">{error}</div>}
    </div>
  )
}
