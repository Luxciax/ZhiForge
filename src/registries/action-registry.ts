import type { AppConfig } from '../domain/settings'
import type { OutputMode } from '../types'

export interface ToolbarAction {
  id: string
  label: string
  primary: boolean
  order: number
  outputMode: OutputMode
}

const ZHIHU_SEARCH_ACTION: ToolbarAction = {
  id: 'zhihu-search',
  label: '搜知乎',
  primary: true,
  order: -1000,
  outputMode: 'markdown'
}

const FALLBACK_ACTIONS: ToolbarAction[] = [
  { id: 'translate', label: '翻译', primary: false, order: 0, outputMode: 'plain-text' },
  { id: 'internalize', label: '内化', primary: false, order: 1, outputMode: 'markdown' },
  { id: 'explain', label: '解释', primary: false, order: 2, outputMode: 'markdown' },
  { id: 'summarize', label: '总结', primary: false, order: 3, outputMode: 'markdown' },
  { id: 'polish', label: '润色', primary: false, order: 4, outputMode: 'plain-text' }
]

const CORE_ORDER: Record<string, number> = {
  translate: 0,
  internalize: 1,
  explain: 2
}

function configuredActions(config: AppConfig): ToolbarAction[] {
  return config.actions
    .filter((action) => action.enabled && action.showInToolbar)
    .sort((left, right) => {
      const leftOrder = CORE_ORDER[left.id]
      const rightOrder = CORE_ORDER[right.id]
      if (leftOrder !== undefined || rightOrder !== undefined) {
        return (leftOrder ?? 1000 + left.order) - (rightOrder ?? 1000 + right.order)
      }
      return left.order - right.order
    })
    .map((action) => ({
      id: action.id,
      label: action.name,
      primary: false,
      order: action.order,
      outputMode: action.outputMode
    }))
}

export function toolbarActions(config: AppConfig | null): ToolbarAction[] {
  const actions = config ? configuredActions(config) : FALLBACK_ACTIONS.map((action) => ({ ...action }))
  return [{ ...ZHIHU_SEARCH_ACTION }, ...actions]
}

export function fallbackToolbarActions(): ToolbarAction[] {
  return [{ ...ZHIHU_SEARCH_ACTION }, ...FALLBACK_ACTIONS.map((action) => ({ ...action }))]
}
