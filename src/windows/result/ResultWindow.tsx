import { invoke } from '@tauri-apps/api/core'
import { LogicalSize, PhysicalPosition } from '@tauri-apps/api/dpi'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  BookOpenText,
  Check,
  ChevronDown,
  Copy,
  LoaderCircle,
  Maximize2,
  Minus,
  PanelTopClose,
  PanelTopOpen,
  Pin,
  RotateCw,
  Settings2,
  Square,
  X
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { clearStoredAiSettings, hasStoredAiSettings, isAiConfigured, loadAiSettings, saveAiSettings } from '../../ai/config'
import { sanitizeModelOutput } from '../../ai/output'
import { bootstrapV2Settings } from '../../services/provider-service'import type {
  ActionRequest,
  AiChunkEvent,
  AiDoneEvent,
  AiErrorEvent,
  AiErrorKind,
  AiSettings,
  AiStartedEvent,
  RoutedAiActionRunRequest
} from '../../types'
import { KnowledgeWorkspace } from './KnowledgeWorkspace'import { WorkspaceErrorBoundary } from './WorkspaceErrorBoundary'import { ProviderSettings } from './ProviderSettings'

const BUILTIN_ACTION_TITLES: Record<string, string> = {  translate: '翻译',
  explain: '解释',
  summarize: '总结',
  polish: '润色'
}
function actionTitle(request: ActionRequest | null): string {
  if (!request) return 'ZhiForge'
  return request.actionName || BUILTIN_ACTION_TITLES[request.action] || request.action
}

const ERROR_LABELS: Record<AiErrorKind, string> = {
  auth: '鉴权失败',
  'rate-limit': '请求受限',
  timeout: '请求超时',
  network: '连接失败',
  provider: '渠道错误',
  'malformed-stream': '响应异常',  'empty-response': '模型无输出',
  interrupted: '连接中断',  config: '配置错误',
  'all-providers-failed': '所有模型均失败'
}

interface ResultError {
  kind: AiErrorKind
  message: string
  detail?: string
}

function friendlyAiError(error: AiErrorEvent): ResultError {
  const message = (() => {    switch (error.kind) {
      case 'auth': return 'API Key 未保存或无效，或当前账号没有访问该模型的权限'
      case 'rate-limit': return '渠道正在限流，或当前额度不足'
      case 'timeout': return '请求超时，请稍后重试'
      case 'network': return '无法连接到当前渠道'
      case 'malformed-stream': return '渠道返回了无法解析的流式响应'
      case 'empty-response': return '模型没有返回可显示内容'
      case 'interrupted': return error.partial ? '生成中断，已保留前面生成的内容' : '生成过程中连接中断'
      case 'config': return '当前动作或模型路由配置不可用'
      case 'all-providers-failed': {
        const detail = error.message.replace(/^all routed providers failed before output:\s*/i, '').trim()
        return detail ? `主模型和备用模型均失败：${detail}` : '主模型和备用模型均未成功返回结果'
      }
      case 'provider': return error.status ? `渠道返回 HTTP ${error.status}` : '渠道返回错误'
    }
  })()
  return { kind: error.kind, message, detail: error.message }
}

export function ResultWindow() {
  const [request, setRequest] = useState<ActionRequest | null>(null)
  const [settings, setSettings] = useState<AiSettings>(() => loadAiSettings())
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [pinned, setPinned] = useState(false)
  const [showOriginal, setShowOriginal] = useState(false)
  const [compact, setCompact] = useState(false)
  const [copiedOriginal, setCopiedOriginal] = useState(false)
  const [copiedResult, setCopiedResult] = useState(false)
  const [content, setContent] = useState('')
  const [error, setError] = useState<ResultError | null>(null)
  const [loading, setLoading] = useState(false)
  const [executionInfo, setExecutionInfo] = useState<AiStartedEvent | null>(null)
  const [homeSearchRequest, setHomeSearchRequest] = useState<{ query: string; nonce: number } | null>(null)

  const settingsRef = useRef(settings)
  const requestRef = useRef<ActionRequest | null>(null)
  const activeRequestIdRef = useRef<string | null>(null)
  const rawContentRef = useRef('')
  const pinnedRef = useRef(pinned)
  const settingsOpenRef = useRef(settingsOpen)
  const settingsRestoreSizeRef = useRef<LogicalSize | null>(null)
  const settingsRestorePositionRef = useRef<PhysicalPosition | null>(null)

  useEffect(() => {    settingsRef.current = settings
  }, [settings])

  useEffect(() => {
    pinnedRef.current = pinned
  }, [pinned])

  useEffect(() => {
    settingsOpenRef.current = settingsOpen
  }, [settingsOpen])

  useEffect(() => {
    const appWindow = getCurrentWindow()
    let disposed = false

    const resizeForSettings = async () => {
      try {
        if (settingsOpen) {
          if (!settingsRestoreSizeRef.current) {
            const [physicalSize, scaleFactor, physicalPosition] = await Promise.all([
              appWindow.outerSize(),
              appWindow.scaleFactor(),
              appWindow.outerPosition()
            ])
            settingsRestoreSizeRef.current = new LogicalSize(
              Math.round(physicalSize.width / scaleFactor),
              Math.round(physicalSize.height / scaleFactor)
            )
            settingsRestorePositionRef.current = physicalPosition
          }
          if (!disposed) {
            await appWindow.setSize(new LogicalSize(1120, 760))
            await appWindow.center()
          }
        } else if (settingsRestoreSizeRef.current) {
          const restoreSize = settingsRestoreSizeRef.current
          const restorePosition = settingsRestorePositionRef.current
          settingsRestoreSizeRef.current = null
          settingsRestorePositionRef.current = null
          if (!disposed) {
            await appWindow.setSize(restoreSize)
            if (restorePosition) await appWindow.setPosition(restorePosition)
          }
        }
      } catch (resizeError) {
        console.error('Failed to resize settings window', resizeError)
      }
    }

    void resizeForSettings()
    return () => {
      disposed = true
    }
  }, [settingsOpen])

  useEffect(() => {
    let disposed = false
    const legacy = settingsRef.current

    void bootstrapV2Settings(legacy, hasStoredAiSettings())
      .then((hydrated) => {
        if (disposed || !hydrated) return
        settingsRef.current = hydrated
        setSettings(hydrated)
        saveAiSettings(hydrated)
      })
      .catch((settingsError) => console.error('Failed to bootstrap v2 settings', settingsError))

    return () => {
      disposed = true
    }
  }, [])

  const runAction = useCallback(async (nextRequest: ActionRequest) => {
    const requestId = crypto.randomUUID()
    activeRequestIdRef.current = requestId
    rawContentRef.current = ''
    setExecutionInfo(null)
    setContent('')
    setError(null)
    setLoading(true)

    const aiRequest: RoutedAiActionRunRequest = {
      requestId,
      action: nextRequest.action,
      text: nextRequest.selection.text
    }

    try {
      await invoke('start_routed_ai_action', { request: aiRequest })
    } catch (invokeError) {
      if (activeRequestIdRef.current !== requestId) return
      activeRequestIdRef.current = null
      setLoading(false)
      setError({ kind: 'config', message: '无法启动当前动作', detail: String(invokeError) })
    }
  }, [])

  useEffect(() => {
    let disposed = false
    const cleanup: Array<() => void> = []

    void Promise.all([
      listen<AiStartedEvent>('ai://started', (event) => {
        if (disposed || event.payload.requestId !== activeRequestIdRef.current) return
        setExecutionInfo(event.payload)
      }),
      listen<AiChunkEvent>('ai://chunk', (event) => {
        if (disposed || event.payload.requestId !== activeRequestIdRef.current) return
        rawContentRef.current += event.payload.chunk
        setContent(sanitizeModelOutput(rawContentRef.current))
      }),
      listen<AiDoneEvent>('ai://done', (event) => {
        if (disposed || event.payload.requestId !== activeRequestIdRef.current) return
        activeRequestIdRef.current = null
        setLoading(false)
        const visible = sanitizeModelOutput(rawContentRef.current)
        setContent(visible)
        if (!visible.trim()) {
          setError({ kind: 'empty-response', message: '模型没有返回可显示内容' })
        }
      }),
      listen<AiErrorEvent>('ai://error', (event) => {
        if (disposed || event.payload.requestId !== activeRequestIdRef.current) return
        activeRequestIdRef.current = null
        setLoading(false)
        setError(friendlyAiError(event.payload))
      }),
      listen<ActionRequest>('selection://action', (event) => {
        if (disposed) return
        requestRef.current = event.payload
        setRequest(event.payload)
        setPinned(false)
        setShowOriginal(false)
        setCopiedOriginal(false)
        setCopiedResult(false)
        rawContentRef.current = ''
        setContent('')
        setError(null)
        setSettingsOpen(false)
        void invoke('set_result_pinned', { pinned: false })
        void runAction(event.payload)
      }),
      listen('app://home', () => {
        if (disposed) return
        const activeRequestId = activeRequestIdRef.current
        activeRequestIdRef.current = null
        if (activeRequestId) void invoke('cancel_ai', { requestId: activeRequestId }).catch(() => undefined)
        requestRef.current = null
        setRequest(null)
        setHomeSearchRequest(null)
        setPinned(false)
        setShowOriginal(false)
        setCopiedOriginal(false)
        setCopiedResult(false)
        rawContentRef.current = ''
        setContent('')
        setError(null)
        setLoading(false)
        setExecutionInfo(null)
        setSettingsOpen(false)
        void invoke('set_result_pinned', { pinned: false })
      }),
      listen<string>('zhihu://search-request', (event) => {
        if (disposed) return
        const activeRequestId = activeRequestIdRef.current
        activeRequestIdRef.current = null
        if (activeRequestId) void invoke('cancel_ai', { requestId: activeRequestId }).catch(() => undefined)
        requestRef.current = null
        setRequest(null)
        setHomeSearchRequest({ query: event.payload || '', nonce: Date.now() })
        setPinned(false)
        setShowOriginal(false)
        setCopiedOriginal(false)
        setCopiedResult(false)
        rawContentRef.current = ''
        setContent('')
        setError(null)
        setLoading(false)
        setExecutionInfo(null)
        setSettingsOpen(false)
        void invoke('set_result_pinned', { pinned: false })
      }),
      listen('app://settings', () => {
        if (disposed) return
        setSettingsOpen(true)
      })
    ]).then((unlisteners) => {
      if (disposed) unlisteners.forEach((unlisten) => unlisten())
      else cleanup.push(...unlisteners)
    })

    return () => {
      disposed = true
      cleanup.forEach((unlisten) => unlisten())
    }
  }, [runAction])

  const cancelAi = useCallback(async () => {
    const requestId = activeRequestIdRef.current
    activeRequestIdRef.current = null
    setLoading(false)
    if (!requestId) return
    await invoke('cancel_ai', { requestId }).catch((cancelError) => console.error('Failed to cancel AI request', cancelError))
  }, [])

  const closeWindow = useCallback(async () => {
    await cancelAi()
    setPinned(false)
    await invoke('set_result_pinned', { pinned: false }).catch(() => undefined)
    await invoke('hide_result').catch((hideError) => console.error('Failed to hide result window', hideError))
  }, [cancelAi])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (settingsOpen) setSettingsOpen(false)
        else void closeWindow()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [closeWindow, settingsOpen])

  useEffect(() => {
    const handleBlur = () => {
      window.setTimeout(() => {
        if (requestRef.current && !document.hasFocus() && !pinnedRef.current && !settingsOpenRef.current) {
          void closeWindow()
        }
      }, 120)
    }
    window.addEventListener('blur', handleBlur)
    return () => window.removeEventListener('blur', handleBlur)
  }, [closeWindow])

  const title = useMemo(() => actionTitle(request), [request])

  const minimizeWindow = () => {
    void getCurrentWindow().minimize().catch((windowError) => {
      console.error('Failed to minimize result window', windowError)
    })
  }

  const toggleMaximizeWindow = () => {
    void getCurrentWindow().toggleMaximize().catch((windowError) => {
      console.error('Failed to toggle maximize result window', windowError)
    })
  }

  const togglePin = async () => {
    const next = !pinned
    setPinned(next)
    try {
      await invoke('set_result_pinned', { pinned: next })
    } catch (pinError) {
      setPinned(!next)
      console.error('Failed to change pin state', pinError)
    }
  }

  const copyText = async (text: string, kind: 'original' | 'result') => {
    if (!text) return
    try {
      await invoke('write_to_clipboard', { text })
      if (kind === 'original') setCopiedOriginal(true)
      else setCopiedResult(true)
      window.setTimeout(() => {
        if (kind === 'original') setCopiedOriginal(false)
        else setCopiedResult(false)
      }, 1400)
    } catch (copyError) {
      console.error('Failed to copy text', copyError)
    }
  }

  const applySettings = async (nextSettings: AiSettings) => {
    if (nextSettings.providerId) saveAiSettings(nextSettings)
    else clearStoredAiSettings()
    settingsRef.current = nextSettings
    setSettings(nextSettings)
    const currentRequest = requestRef.current
    if (currentRequest) await runAction(currentRequest)
  }

  const isHome = !request
  const effectiveModel = executionInfo?.model || settings.model
  const effectiveAdapter = executionInfo?.adapter || settings.adapter
  const effectiveProviderName = executionInfo?.providerName || settings.providerId || '当前渠道'

  return (
    <div className={`result-shell${compact ? ' compact' : ''}${isHome ? ' home' : ''}`}>
      <header className="result-titlebar" data-tauri-drag-region>
        <div className="result-title" data-tauri-drag-region>
          <BookOpenText size={14} strokeWidth={1.8} />
          <span>{title}</span>        </div>
        <div className="result-window-actions">          <button type="button" aria-label="设置" title="设置" onClick={() => setSettingsOpen(true)}>
            <Settings2 size={14} strokeWidth={1.8} />          </button>
          {!isHome && (            <>
              <button className={pinned ? 'active' : undefined} type="button" aria-label={pinned ? '取消置顶' : '置顶'} title={pinned ? '取消置顶' : '置顶'} onClick={() => void togglePin()}>                <Pin size={14} strokeWidth={1.8} className={pinned ? 'pin-active' : undefined} />
              </button>              <button type="button" aria-label={compact ? '舒展模式' : '紧凑模式'} title={compact ? '舒展模式' : '紧凑模式'} onClick={() => setCompact((value) => !value)}>
                {compact ? <PanelTopOpen size={14} strokeWidth={1.8} /> : <PanelTopClose size={14} strokeWidth={1.8} />}              </button>
            </>          )}
          <button type="button" aria-label="最小化" title="最小化" onClick={minimizeWindow}>            <Minus size={15} strokeWidth={1.8} />
          </button>          <button type="button" aria-label="最大化或还原" title="最大化或还原" onClick={toggleMaximizeWindow}>
            <Maximize2 size={14} strokeWidth={1.8} />
          </button>
          <button className="close" type="button" aria-label="关闭" title="关闭" onClick={() => void closeWindow()}>
            <X size={15} strokeWidth={1.8} />
          </button>
        </div>
      </header>
      {isHome ? (
        <main className="home-body">
          <WorkspaceErrorBoundary resetToken={homeSearchRequest}>
            <KnowledgeWorkspace onOpenSettings={() => setSettingsOpen(true)} searchRequest={homeSearchRequest} />
          </WorkspaceErrorBoundary>
        </main>
      ) : (
      <main className="result-body">
        <div className="result-toolbar-row">
          <div className="result-source-chip">{request.selection.programName || '选中文本'}</div>
          <button className="show-original" type="button" onClick={() => setShowOriginal((value) => !value)}>
            <span>{showOriginal ? '隐藏原文' : '显示原文'}</span>
            <ChevronDown size={14} strokeWidth={1.8} className={showOriginal ? 'expanded' : undefined} />
          </button>
        </div>

        {showOriginal && (
          <section className="original-card">
            <div className="original-text">{request.selection.text}</div>
            <button type="button" className="copy-button" aria-label="复制原文" title="复制原文" onClick={() => void copyText(request.selection.text, 'original')}>
              {copiedOriginal ? <Check size={14} strokeWidth={1.8} /> : <Copy size={14} strokeWidth={1.8} />}
            </button>
          </section>
        )}

        <section className="result-content" aria-live="polite">
          {content ? (
            request.outputMode === 'plain-text' ? (
              <div className="plain-result">{content}</div>
            ) : (
              <div className="markdown-result">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
              </div>
            )
          ) : loading ? (
            <div className="result-empty"><LoaderCircle size={18} strokeWidth={1.7} className="spin" /><span>处理中</span></div>
          ) : !executionInfo && !isAiConfigured(settings) ? (
            <button type="button" className="configure-empty" onClick={() => setSettingsOpen(true)}>
              <Settings2 size={17} strokeWidth={1.7} />
              <span>配置模型</span>
            </button>
          ) : (
            <div className="result-empty"><BookOpenText size={18} strokeWidth={1.6} /><span>等待结果</span></div>
          )}
        </section>

        {error && (
          <div className={`result-error ${error.kind}`} title={error.detail || error.message}>
            <span>{ERROR_LABELS[error.kind]}</span>
            <p>{error.message}</p>
          </div>
        )}
      </main>
      )}

      {!isHome && <footer className="result-footer">
        <div className="provider-status" title={`${effectiveProviderName} · ${effectiveAdapter} · ${effectiveModel || '未配置模型'}`}>
          {effectiveModel || '未配置模型'}
        </div>
        <div className="result-actions">
          {content && (
            <button type="button" aria-label="复制结果" title="复制结果" onClick={() => void copyText(content, 'result')}>
              {copiedResult ? <Check size={14} strokeWidth={1.8} /> : <Copy size={14} strokeWidth={1.8} />}
            </button>
          )}
          {loading ? (
            <button type="button" aria-label="停止" title="停止" onClick={() => void cancelAi()}><Square size={13} strokeWidth={1.8} /></button>
          ) : (
            <button type="button" aria-label="重新生成" title="重新生成" onClick={() => void runAction(request)}><RotateCw size={14} strokeWidth={1.8} /></button>
          )}
        </div>
      </footer>}

      {settingsOpen && <ProviderSettings value={settings} onApply={applySettings} onClose={() => setSettingsOpen(false)} />}
    </div>
  )
}
