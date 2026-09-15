import { listen } from '@tauri-apps/api/event'
import {
  Activity,
  ChevronRight,
  CircleAlert,
  Inbox,
  ListChecks,
  LoaderCircle,
  RefreshCcw,
  RotateCcw,
  Sparkles,
  Telescope,
  TriangleAlert
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { getTodayOverview, type TodayOverview } from '../../services/knowledge-service'
import './today-view.css'

interface TodayViewProps {
  onOpenInbox: (inboxId?: string | null) => void
  onOpenReview: () => void
  onOpenResearch: (planId?: string | null, gapId?: string | null) => void
  onOpenKnowledge: (knowledgeUnitId: string, questionId?: string | null) => void
  onOpenSuggestions: () => void
  onOpenTasks: () => void
}

const EMPTY_OVERVIEW: TodayOverview = {
  generatedAt: 0,
  inboxCapturedCount: 0,
  inboxReadyCount: 0,
  inboxFailedCount: 0,
  inboxProcessingCount: 0,
  inboxItems: [],
  aiActiveCount: 0,
  aiFailedCount: 0,
  initialLearningCount: 0,
  initialLearningItems: [],
  dueReviewCount: 0,
  activeReviewRemainingCount: 0,
  reviewItems: [],
  activeResearchPlanCount: 0,
  openResearchGapCount: 0,
  collectingResearchGapCount: 0,
  researchGaps: [],
  weakCount: 0,
  conflictedCount: 0,
  needsExpansionCount: 0,
  staleCount: 0,
  maintenanceCount: 0,
  knowledgeSignals: [],
  librarianPendingCount: 0,
  curationPendingCount: 0
}

const SIGNAL_LABELS: Record<string, string> = {
  weak: '薄弱知识',
  conflicted: '存在冲突',
  needs_expansion: '需要扩展',
  stale: '可能过时',
  other: '需要维护'
}

const INBOX_LABELS: Record<string, string> = {
  captured: '待提炼',
  ready: '等待确认',
  failed: '处理失败',
  processing: 'AI 处理中'
}

export function TodayView({
  onOpenInbox,
  onOpenReview,
  onOpenResearch,
  onOpenKnowledge,
  onOpenSuggestions,
  onOpenTasks
}: TodayViewProps) {
  const [overview, setOverview] = useState<TodayOverview>(EMPTY_OVERVIEW)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async (quiet = false) => {
    if (!quiet) setRefreshing(true)
    try {
      const next = await getTodayOverview()
      setOverview(next)
      setError(null)
    } catch (loadError) {
      setError(`今天的工作队列加载失败：${String(loadError)}`)
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }, [])

  useEffect(() => {
    void refresh(true)
    let disposed = false
    const cleanup: Array<() => void> = []
    void Promise.all([
      listen('knowledge://changed', () => { if (!disposed) void refresh(true) }),
      listen('research://changed', () => { if (!disposed) void refresh(true) }),
      listen('librarian://changed', () => { if (!disposed) void refresh(true) }),
      listen('curation://changed', () => { if (!disposed) void refresh(true) })
    ]).then((unlisteners) => {
      if (disposed) unlisteners.forEach((unlisten) => unlisten())
      else cleanup.push(...unlisteners)
    })
    return () => {
      disposed = true
      cleanup.forEach((unlisten) => unlisten())
    }
  }, [refresh])

  useEffect(() => {
    if (overview.aiActiveCount <= 0) return
    const timer = window.setInterval(() => void refresh(true), 5_000)
    return () => window.clearInterval(timer)
  }, [overview.aiActiveCount, refresh])

  const maintenanceCount = overview.maintenanceCount
  const suggestionCount = overview.librarianPendingCount + overview.curationPendingCount
  const inboxOpenCount = overview.inboxCapturedCount + overview.inboxReadyCount + overview.inboxFailedCount + overview.inboxProcessingCount
  const actionCount = useMemo(
    () => overview.inboxCapturedCount + overview.inboxReadyCount + overview.inboxFailedCount + overview.aiFailedCount + overview.initialLearningCount + overview.dueReviewCount + overview.openResearchGapCount + overview.collectingResearchGapCount + maintenanceCount + suggestionCount,
    [maintenanceCount, overview, suggestionCount]
  )

  if (loading) {
    return (
      <section className="today-view today-loading">
        <LoaderCircle size={18} strokeWidth={1.8} className="spin" />
        <span>正在整理今天的工作队列…</span>
      </section>
    )
  }

  return (
    <section className="today-view" aria-label="今天">
      <header className="today-header">
        <div>
          <span>今日工作台</span>
          <h1>{actionCount > 0 ? `${actionCount} 项值得处理` : '今天没有积压任务'}</h1>
          <p>这里只放会推动知识库继续生长或变得更可靠的事情，不展示无关的累计数字。</p>
        </div>
        <button type="button" disabled={refreshing} onClick={() => void refresh()}>
          {refreshing ? <LoaderCircle size={13} strokeWidth={1.8} className="spin" /> : <RefreshCcw size={13} strokeWidth={1.8} />}
          <span>刷新</span>
        </button>
      </header>

      {error && <div className="today-error"><CircleAlert size={14} strokeWidth={1.8} /><span>{error}</span></div>}

      <div className="today-summary-grid">
        <button type="button" onClick={() => onOpenInbox()} className={overview.inboxCapturedCount + overview.inboxReadyCount + overview.inboxFailedCount > 0 ? 'attention' : undefined}>
          <Inbox size={16} strokeWidth={1.8} />
          <span>收件箱</span>
          <strong>{inboxOpenCount}</strong>
          <small>{overview.inboxFailedCount > 0 ? `${overview.inboxFailedCount} 个失败` : overview.inboxCapturedCount > 0 ? `${overview.inboxCapturedCount} 个待提炼` : overview.inboxProcessingCount > 0 ? `${overview.inboxProcessingCount} 个处理中` : `${overview.inboxReadyCount} 个待确认`}</small>
        </button>
        <button type="button" onClick={onOpenReview} className={overview.initialLearningCount + overview.dueReviewCount > 0 ? 'attention' : undefined}>
          <RotateCcw size={16} strokeWidth={1.8} />
          <span>学习与复习</span>
          <strong>{overview.initialLearningCount + overview.dueReviewCount}</strong>
          <small>{overview.initialLearningCount > 0 ? `${overview.initialLearningCount} 条首次学习` : overview.activeReviewRemainingCount > 0 ? `当前会话剩 ${overview.activeReviewRemainingCount}` : `${overview.dueReviewCount} 条到期`}</small>
        </button>
        <button type="button" onClick={() => onOpenResearch()}>
          <Telescope size={16} strokeWidth={1.8} />
          <span>研究缺口</span>
          <strong>{overview.openResearchGapCount + overview.collectingResearchGapCount}</strong>
          <small>{overview.collectingResearchGapCount > 0 ? `${overview.collectingResearchGapCount} 个收集中` : `${overview.activeResearchPlanCount} 个活跃计划`}</small>
        </button>
        <button type="button" onClick={() => overview.knowledgeSignals[0] && onOpenKnowledge(overview.knowledgeSignals[0].knowledgeUnitId)}>
          <TriangleAlert size={16} strokeWidth={1.8} />
          <span>知识维护</span>
          <strong>{maintenanceCount}</strong>
          <small>{overview.conflictedCount > 0 ? `${overview.conflictedCount} 个冲突` : overview.weakCount > 0 ? `${overview.weakCount} 个薄弱` : '质量信号'}</small>
        </button>
        <button type="button" onClick={onOpenSuggestions}>
          <Sparkles size={16} strokeWidth={1.8} />
          <span>整理建议</span>
          <strong>{suggestionCount}</strong>
          <small>{overview.curationPendingCount > 0 ? `${overview.curationPendingCount} 个语义判断` : `${overview.librarianPendingCount} 个组织建议`}</small>
        </button>
        <button type="button" onClick={onOpenTasks} className={overview.aiFailedCount > 0 ? 'attention' : undefined}>
          <Activity size={16} strokeWidth={1.8} />
          <span>AI 任务</span>
          <strong>{overview.aiActiveCount + overview.aiFailedCount}</strong>
          <small>{overview.aiFailedCount > 0 ? `${overview.aiFailedCount} 个失败` : overview.aiActiveCount > 0 ? `${overview.aiActiveCount} 个处理中` : '当前正常'}</small>
        </button>
      </div>

      <div className="today-columns">
        <div className="today-column">
          <section className="today-section">
            <div className="today-section-head">
              <div>
                <Inbox size={14} strokeWidth={1.8} />
                <strong>先清收件箱</strong>
                <span>{overview.inboxReadyCount} 待确认 · {overview.inboxCapturedCount} 待提炼</span>
              </div>
              <button type="button" onClick={() => onOpenInbox()}>全部 <ChevronRight size={12} strokeWidth={1.8} /></button>
            </div>
            {overview.inboxItems.length === 0 ? (
              <div className="today-empty">没有需要人工处理的收件箱项目。</div>
            ) : (
              <div className="today-list">
                {overview.inboxItems.map((item) => (
                  <button type="button" key={item.inboxId} onClick={() => onOpenInbox(item.inboxId)}>
                    <span className={`today-state ${item.status}`}>{INBOX_LABELS[item.status] ?? item.status}</span>
                    <strong>{item.title}</strong>
                    <small>{item.status === 'ready' ? `${item.draftCount} 条提炼草稿` : item.status === 'captured' ? '等待开始提炼' : item.error || item.processingStage}</small>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="today-section">
            <div className="today-section-head">
              <div>
                <RotateCcw size={14} strokeWidth={1.8} />
                <strong>学习与复习</strong>
                <span>{overview.initialLearningCount} 首次 · {overview.dueReviewCount} 到期</span>
              </div>
              <button type="button" onClick={onOpenReview}>学习中心 <ChevronRight size={12} strokeWidth={1.8} /></button>
            </div>
            {overview.initialLearningItems.length === 0 && overview.reviewItems.length === 0 ? (
              <div className="today-empty">当前没有首次学习或到期复习任务。</div>
            ) : (
              <div className="today-list">
                {overview.initialLearningItems.map((item) => (
                  <button type="button" key={`learn:${item.knowledgeUnitId}`} onClick={() => onOpenKnowledge(item.knowledgeUnitId, item.questionId)}>
                    <span className="today-state review">首次学习</span>
                    <strong>{item.coreClaim || item.selectedText}</strong>
                    <small>完成第一次理解检验后开始间隔复习</small>
                  </button>
                ))}
                {overview.reviewItems.map((item) => (
                  <button type="button" key={`review:${item.knowledgeUnitId}`} onClick={() => onOpenKnowledge(item.knowledgeUnitId, item.questionId)}>
                    <span className="today-state review">掌握 {item.masteryScore}%</span>
                    <strong>{item.coreClaim || item.selectedText}</strong>
                    <small>{item.lapseCount > 0 ? `${item.lapseCount} 次遗忘 · ${item.wrongCount} 次答错` : `${item.reviewCount} 次复习`}</small>
                  </button>
                ))}
              </div>
            )}
          </section>
        </div>

        <div className="today-column">
          <section className="today-section">
            <div className="today-section-head">
              <div>
                <Telescope size={14} strokeWidth={1.8} />
                <strong>继续研究</strong>
                <span>{overview.openResearchGapCount + overview.collectingResearchGapCount} 个缺口</span>
              </div>
              <button type="button" onClick={() => onOpenResearch()}>研究计划 <ChevronRight size={12} strokeWidth={1.8} /></button>
            </div>
            {overview.researchGaps.length === 0 ? (
              <div className="today-empty">还没有进行中的研究缺口。可以从“主题扩充”建立一个计划。</div>
            ) : (
              <div className="today-list">
                {overview.researchGaps.map((gap) => (
                  <button type="button" key={gap.gapId} onClick={() => onOpenResearch(gap.planId, gap.gapId)}>
                    <span className={`today-state priority-${gap.priority}`}>{gap.priority === 'high' ? '高优先级' : gap.priority === 'medium' ? '中优先级' : '低优先级'}</span>
                    <strong>{gap.title}</strong>
                    <small>{gap.topic} · {gap.status === 'collecting' ? `已采 ${gap.sourceCount} 个来源` : gap.searchQuery}</small>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="today-section">
            <div className="today-section-head">
              <div>
                <ListChecks size={14} strokeWidth={1.8} />
                <strong>知识维护</strong>
                <span>{maintenanceCount} 个信号</span>
              </div>
            </div>
            {overview.knowledgeSignals.length === 0 ? (
              <div className="today-empty">没有薄弱、冲突、待扩展或过时的知识信号。</div>
            ) : (
              <div className="today-list">
                {overview.knowledgeSignals.map((item) => (
                  <button type="button" key={item.knowledgeUnitId} onClick={() => onOpenKnowledge(item.knowledgeUnitId)}>
                    <span className={`today-state signal-${item.signal}`}>{SIGNAL_LABELS[item.signal] ?? '需要维护'}</span>
                    <strong>{item.title}</strong>
                    <small>当前掌握度 {item.masteryScore}%</small>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="today-section today-suggestions-card">
            <div className="today-section-head">
              <div>
                <Sparkles size={14} strokeWidth={1.8} />
                <strong>整理建议</strong>
                <span>{suggestionCount} 待确认</span>
              </div>
              <button type="button" onClick={onOpenSuggestions}>查看建议 <ChevronRight size={12} strokeWidth={1.8} /></button>
            </div>
            <div className="today-suggestion-summary">
              <span><strong>{overview.librarianPendingCount}</strong> 个主题 / 标签 / 关系建议</span>
              <span><strong>{overview.curationPendingCount}</strong> 个重复 / 关联 / 冲突判断</span>
            </div>
          </section>
        </div>
      </div>
    </section>
  )
}
