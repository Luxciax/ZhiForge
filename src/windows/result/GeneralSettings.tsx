import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Check, Copy, FileText, Power } from 'lucide-react'
import { useEffect, useState } from 'react'

export function GeneralSettings() {
  const [autostart, setAutostart] = useState(false)
  const [loading, setLoading] = useState(true)
  const [logPath, setLogPath] = useState('')
  const [copiedLogPath, setCopiedLogPath] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void invoke<string>('diagnostics_log_path')
      .then((path) => {
        if (!disposed) setLogPath(path)
      })
      .catch(() => undefined)

    void invoke<boolean>('autostart_is_enabled')
      .then((enabled) => {
        if (!disposed) setAutostart(enabled)
      })
      .catch((readError) => {
        if (!disposed) setError(String(readError))
      })
      .finally(() => {
        if (!disposed) setLoading(false)
      })

    void listen<boolean>('runtime://autostart-changed', (event) => {
      if (!disposed) setAutostart(event.payload)
    }).then((cleanup) => {
      if (disposed) cleanup()
      else unlisten = cleanup
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  const copyLogPath = async () => {
    if (!logPath) return
    try {
      await invoke('write_to_clipboard', { text: logPath })
      setCopiedLogPath(true)
      window.setTimeout(() => setCopiedLogPath(false), 1200)
    } catch (copyError) {
      setError(String(copyError))
    }
  }

  const toggleAutostart = async () => {
    setLoading(true)
    setError(null)
    try {
      const enabled = await invoke<boolean>('autostart_set_enabled', { enabled: !autostart })
      setAutostart(enabled)
    } catch (toggleError) {
      setError(String(toggleError))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="general-settings-editor">
      <button type="button" className="general-setting-row" disabled={loading} onClick={() => void toggleAutostart()}>
        <span className="general-setting-icon"><Power size={14} strokeWidth={1.8} /></span>
        <span className="general-setting-copy">
          <strong>开机启动</strong>
          <small>开机后静默在后台运行，不自动弹出主页。</small>
        </span>
        <span className={`compact-switch${autostart ? ' on' : ''}`} aria-hidden="true"><i /></span>
      </button>
      <button type="button" className="general-setting-row" disabled={!logPath} onClick={() => void copyLogPath()}>
        <span className="general-setting-icon"><FileText size={14} strokeWidth={1.8} /></span>
        <span className="general-setting-copy">
          <strong>诊断日志</strong>
          <small>{logPath || '正在读取日志路径…'}</small>
        </span>
        <span className="general-setting-icon general-copy-status">
          {copiedLogPath ? <Check size={13} strokeWidth={1.8} /> : <Copy size={13} strokeWidth={1.8} />}
        </span>
      </button>
      <div className="general-note">日志只记录应用、取词方式、渠道/模型和错误类别，不记录选中文字、Prompt、API Key 或请求体。关闭主页不会退出应用。</div>
      {error && <div className="settings-error">{error}</div>}
    </div>
  )
}
