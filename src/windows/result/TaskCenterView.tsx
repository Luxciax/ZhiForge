import {
  AlertCircle,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  LoaderCircle,
  RefreshCw,
  RotateCcw
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { analyzeKnowledgeCuration } from '../../services/curation-service'
import {
  generateKnowledgeQuestions,
  internalizeKnowledge,
  listKnowledgeTasks
} from '../../services/knowledge-service'
import { analyzeKnowledgeLibrary } from '../../services/librarian-service'
import type { AiTaskRecord, AiTaskType } from '../../types'
import './task-center.css'

const TYPE_LABELS: Record<AiTaskType, string> = {
  internalize: '知识提炼',
  question_generation: '学习题生成',
  librarian: '知识整理',
  curation: '重复 / 冲突检查'
}

const STATUS_LABELS: Record<AiTaskRecord['status'], string> = {
  pending: '等待中',
  running: '处理中',
  completed: '已完成',
  failed: '失败'
}

type TaskFilter = 'active' | 'failed' | 'all'

interface TaskCenterViewProps {
  onOpenKnowledge: (knowledgeUnitId: string) => void
  onOpenInbox: (inboxId: string | null) => void
  onOpenSuggestions: () => void
}

function taskTime(timestamp: number) {
  return new Intl.DateTimeFormat('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  }).format(new Date(timestamp))
}

export function TaskCenterView({ onOpenKnowledge, onOpenInbox, onOpenSuggestions }: TaskCenterViewProps) {
  const [tasks, setTasks] = useState<AiTaskRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<TaskFilter>('active')
  const [retryingId, setRetryingId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true)
    try {
      setTasks(await listKnowledgeTasks(60))
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      if (!quiet) setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const hasActive = tasks.some((task) => task.status === 'pending' || task.status === 'running')
  useEffect(() => {
    if (!hasActive) return
    const timer = window.setInterval(() => void refresh(true), 1400)
    return () => window.clearInterval(timer)
  }, [hasActive, refresh])

  const counts = useMemo(() => ({
    active: tasks.filter((task) => task.status === 'pending' || task.status === 'running').length,
    failed: tasks.filter((task) => task.status === 'failed').length,
    completed: tasks.filter((task) => task.status === 'completed').length
  }), [tasks])

  const visible = useMemo(() => {
    if (filter === 'active') {
      const active = tasks.filter((task) => task.status === 'pending' || task.status === 'running')
      const failed = tasks.filter((task) => task.status === 'failed')
      return [...active, ...failed]
    }
    if (filter === 'failed') return tasks.filter((task) => task.status === 'failed')
    return tasks
  }, [filter, tasks])

  const openTask = (task: AiTaskRecord) => {
    if (task.taskType === 'internalize' && task.inboxId) {
      onOpenInbox(task.inboxId)
      return
    }
    if ((task.taskType === 'internalize' || task.taskType === 'question_generation') && task.knowledgeUnitId) {
      onOpenKnowledge(task.knowledgeUnitId)
      return
    }
    onOpenSuggestions()
  }

  const retry = async (task: AiTaskRecord) => {
    if (!task.retryable || retryingId) return
    setRetryingId(task.id)
    setError(null)
    try {
      if (task.taskType === 'internalize') {
        if (!task.knowledgeUnitId) throw new Error('任务缺少知识 ID')
        await internalizeKnowledge(task.knowledgeUnitId)
      } else if (task.taskType === 'question_generation') {
        if (!task.knowledgeUnitId) throw new Error('任务缺少知识 ID')
        await generateKnowledgeQuestions(task.knowledgeUnitId)
      } else if (task.taskType === 'librarian') {
        await analyzeKnowledgeLibrary()
      } else {
        await analyzeKnowledgeCuration()
      }
      await refresh(true)
    } catch (retryError) {
      setError(`重试失败：${String(retryError)}`)
      await refresh(true)
    } finally {
      setRetryingId(null)
    }
  }

  return (
    <section className="task-center-view" aria-label="AI 任务中心">
      <header className="workspace-page-heading task-center-heading">
        <div>
          <h1>AI 任务</h1>
          <p>集中查看知识提炼、学习题生成和知识整理的当前状态。已被后续成功覆盖的旧失败不会继续显示。</p>
        </div>
        <button type="button" className="task-refresh" disabled={loading || Boolean(retryingId)} onClick={() => void refresh()} title="刷新任务">
          <RefreshCw size={14} strokeWidth={1.8} className={loading ? 'spin' : undefined} />
        </button>
      </header>

      <div className="task-summary">
        <button type="button" className={filter === 'active' ? 'active' : undefined} onClick={() => setFilter('active')}>
          <CircleDot size={14} strokeWidth={1.8} />
          <span>当前</span>
          <strong>{counts.active + counts.failed}</strong>
          <small>{counts.active} 处理中 · {counts.failed} 失败</small>
        </button>
        <button type="button" className={filter === 'failed' ? 'active' : undefined} onClick={() => setFilter('failed')}>
          <AlertCircle size={14} strokeWidth={1.8} />
          <span>失败</span>
          <strong>{counts.failed}</strong>
          <small>可以直接重试</small>
        </button>
        <button type="button" className={filter === 'all' ? 'active' : undefined} onClick={() => setFilter('all')}>
          <CheckCircle2 size={14} strokeWidth={1.8} />
          <span>全部</span>
          <strong>{tasks.length}</strong>
          <small>{counts.completed} 已完成</small>
        </button>
      </div>

      {error && <div className="task-error"><AlertCircle size={14} strokeWidth={1.8} /><span>{error}</span></div>}

      {loading ? (
        <div className="task-empty"><LoaderCircle size={17} className="spin" />正在读取任务状态</div>
      ) : visible.length === 0 ? (
        <div className="task-empty">
          <CheckCircle2 size={18} strokeWidth={1.7} />
          <strong>{filter === 'failed' ? '没有失败任务' : filter === 'active' ? '当前没有需要处理的 AI 任务' : '还没有任务记录'}</strong>
          <span>{filter === 'active' ? '新的提炼、出题和知识整理任务会自动出现在这里。' : '切换筛选可以查看其他任务。'}</span>
        </div>
      ) : (
        <div className="task-list">
          {visible.map((task) => {
            const active = task.status === 'pending' || task.status === 'running'
            return (
              <article key={task.id} className={task.status}>
                <div className="task-state-icon">
                  {active ? <LoaderCircle size={15} strokeWidth={1.8} className="spin" /> : task.status === 'failed' ? <AlertCircle size={15} strokeWidth={1.8} /> : <CheckCircle2 size={15} strokeWidth={1.8} />}
                </div>
                <div className="task-copy">
                  <div className="task-meta">
                    <span className={`task-status ${task.status}`}>{STATUS_LABELS[task.status]}</span>
                    <span>{TYPE_LABELS[task.taskType]}</span>
                    <small>{taskTime(task.updatedAt)}</small>
                  </div>
                  <strong>{task.title}</strong>
                  {task.error && <p>{task.error}</p>}
                </div>
                <div className="task-actions">
                  {task.retryable && (
                    <button type="button" className="retry" disabled={Boolean(retryingId)} onClick={() => void retry(task)}>
                      {retryingId === task.id ? <LoaderCircle size={12} className="spin" /> : <RotateCcw size={12} strokeWidth={1.8} />}
                      <span>{retryingId === task.id ? '重试中' : '重试'}</span>
                    </button>
                  )}
                  <button type="button" onClick={() => openTask(task)}>
                    <span>{task.taskType === 'librarian' || task.taskType === 'curation' ? '整理建议' : task.inboxId && task.taskType === 'internalize' ? '收件箱' : '查看'}</span>
                    <ChevronRight size={12} strokeWidth={1.8} />
                  </button>
                </div>
              </article>
            )
          })}
        </div>
      )}
    </section>
  )
}
