import { invoke } from '@tauri-apps/api/core'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { emit, listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Check, CircleAlert, LoaderCircle, MoreHorizontal, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { AppConfig } from '../../domain/settings'
import { fallbackToolbarActions, toolbarActions, type ToolbarAction } from '../../registries/action-registry'
import { captureKnowledge, internalizeKnowledge } from '../../services/knowledge-service'
import { getAppConfig } from '../../services/settings-service'
import type { ActionRequest, SelectionPayload, SourcePlatform } from '../../types'

type CaptureStatus = 'saving' | 'saved' | 'exists' | 'error' | null

function toolbarWidth(actions: ToolbarAction[]): number {
  if (typeof document === 'undefined') return 350
  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  if (!context) return 350
  context.font = '13px "Segoe UI", system-ui, sans-serif'
  const actionWidth = actions.reduce((total, action) => {
    const labelWidth = Math.ceil(context.measureText(action.label).width)
    return total + Math.max(42, labelWidth + 20) + 2
  }, 0)
  const groupDivider = actions.some((action, index) => index > 0 && actions[index - 1]?.primary && !action.primary) ? 5 : 0
  const tail = actions.length > 0 ? 5 + 31 : 31
  return Math.min(960, Math.max(180, 12 + actionWidth + groupDivider + tail))
}

function captureStatusLabel(status: Exclude<CaptureStatus, null>, platform: SourcePlatform | null) {
  switch (status) {
    case 'saving': return '正在内化'
    case 'saved': return platform === 'zhihu' ? '已内化 · 知乎' : '已内化'
    case 'exists': return platform === 'zhihu' ? '已经内化过 · 知乎' : '已经内化过'
    case 'error': return '保存失败'
  }
}

export function Toolbar() {
  const [selection, setSelection] = useState<SelectionPayload | null>(null)
  const [actions, setActions] = useState<ToolbarAction[]>(() => fallbackToolbarActions())
  const [captureStatus, setCaptureStatus] = useState<CaptureStatus>(null)
  const [capturePlatform, setCapturePlatform] = useState<SourcePlatform | null>(null)
  const [showMore, setShowMore] = useState(false)
  const coreActionIds = ['zhihu-search', 'translate', 'internalize', 'explain']
  const extraActions = actions.filter((action) => !coreActionIds.includes(action.id))
  const visibleActions = showMore ? actions : actions.filter((action) => coreActionIds.includes(action.id))

  useEffect(() => {
    const width = captureStatus ? 180 : Math.min(960, toolbarWidth(visibleActions) + (extraActions.length ? 33 : 0))
    void getCurrentWindow().setSize(new LogicalSize(width, 43)).catch((error) => {
      console.error('Failed to resize toolbar', error)
    })
  }, [actions, captureStatus, showMore])

  useEffect(() => {
    let disposed = false
    const cleanup: Array<() => void> = []

    void getAppConfig()
      .then((config) => {
        if (!disposed) setActions(toolbarActions(config))
      })
      .catch(() => undefined)

    void Promise.all([
      listen<SelectionPayload>('selection://changed', (event) => {
        if (!disposed) {
          setSelection(event.payload)
          setCaptureStatus(null)
          setCapturePlatform(null)
          setShowMore(false)
        }
      }),
      listen<AppConfig>('settings://changed', (event) => {
        if (!disposed) setActions(toolbarActions(event.payload))
      })
    ]).then((unlisteners) => {
      if (disposed) unlisteners.forEach((unlisten) => unlisten())
      else cleanup.push(...unlisteners)
    })

    return () => {
      disposed = true
      cleanup.forEach((unlisten) => unlisten())
    }
  }, [])

  const hide = () => void getCurrentWindow().hide()

  const runAction = async (action: ToolbarAction) => {
    if (!selection) return

    if (action.id === 'zhihu-search') {
      const toolbarWindow = getCurrentWindow()
      try {
        await invoke('zhihu_open_search', { query: selection.text })
        await toolbarWindow.hide()
      } catch (error) {
        await toolbarWindow.show().catch(() => undefined)
        await toolbarWindow.setFocus().catch(() => undefined)
        console.error('Failed to open Zhihu search', error)
      }
      return
    }

    if (action.id === 'internalize') {
      setCaptureStatus('saving')
      try {
        const captured = await captureKnowledge({
          platform: 'desktop',
          selectedText: selection.text,
          application: selection.programName || undefined
        })
        setCapturePlatform(captured.source.platform)
        setCaptureStatus(captured.duplicate ? 'exists' : 'saved')
        void emit('knowledge://changed', { knowledgeUnitId: captured.knowledgeUnit.id }).catch((eventError) => {
          console.error('Failed to broadcast knowledge change', eventError)
        })
        try {
          await internalizeKnowledge(captured.knowledgeUnit.id)
        } catch (jobError) {
          console.error('Source was saved but AI internalization could not be scheduled', jobError)
        }
        window.setTimeout(() => void getCurrentWindow().hide(), 650)
      } catch (error) {
        setCaptureStatus('error')
        console.error('Failed to capture knowledge', error)
      }
      return
    }

    const request: ActionRequest = {
      action: action.id,
      actionName: action.label,
      outputMode: action.outputMode,
      selection
    }
    try {
      await invoke('open_action', { request })
      await getCurrentWindow().hide()
    } catch (error) {
      console.error('Failed to open selection action', error)
    }
  }

  return (
    <div className="toolbar" onContextMenu={(event) => event.preventDefault()}>
      {captureStatus ? (
        <div className={`toolbar-feedback ${captureStatus}`} aria-live="polite">
          {captureStatus === 'saving'
            ? <LoaderCircle size={15} strokeWidth={1.8} className="spin" />
            : captureStatus === 'error'
              ? <CircleAlert size={15} strokeWidth={1.8} />
              : <Check size={15} strokeWidth={1.8} />}
          <span>{captureStatusLabel(captureStatus, capturePlatform)}</span>
        </div>
      ) : (
        visibleActions.map((action, index) => {
          const previous = index > 0 ? visibleActions[index - 1] : undefined
          const groupBreak = Boolean(previous?.primary && !action.primary)
          return (
            <span className="toolbar-action-wrap" key={action.id}>
              {groupBreak && <span className="divider" />}
              <button
                className={action.primary ? 'primary' : undefined}
                type="button"
                aria-label={action.label}
                onClick={() => void runAction(action)}>
                {action.label}
              </button>
            </span>
          )
        })
      )}
      {!captureStatus && extraActions.length > 0 && (
        <button
          className={`icon-button${showMore ? ' active' : ''}`}
          type="button"
          aria-label={showMore ? '收起更多动作' : '更多动作'}
          title={showMore ? '收起' : '更多'}
          onClick={() => setShowMore((value) => !value)}>
          <MoreHorizontal size={17} strokeWidth={1.8} />
        </button>
      )}
      <span className="divider" />
      <button className="icon-button" type="button" aria-label="关闭" title="关闭" onClick={hide}>
        <X size={16} strokeWidth={1.8} />
      </button>
    </div>
  )
}
