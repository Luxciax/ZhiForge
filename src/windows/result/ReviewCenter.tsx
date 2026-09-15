import { Check, Circle, History, LoaderCircle, Play, RotateCcw, Sparkles, X } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  abandonReviewSession,
  applyReviewPlan,
  getCurrentReviewSession,
  listKnowledgeReviewQueue,
  listReviewSessionHistory,
  previewReviewPlan,
  startReviewSession,
  type ReviewPlanItem,
  type ReviewPlanPreview,
  type ReviewQueueItem,
  type ReviewSessionDetail,
  type ReviewSessionItem,
  type ReviewSessionSummary
} from '../../services/knowledge-service'
import type { SourcePlatform } from '../../types'

interface ReviewCenterProps {
  onOpenKnowledge: (knowledgeUnitId: string, questionId?: string | null) => void
}

const PLATFORM_LABELS: Record<SourcePlatform, string> = {
  zhihu: '知乎',
  web: '网页',
  pdf: 'PDF',
  desktop: '桌面',
  unknown: '未知来源'
}

function reviewTitle(item: ReviewQueueItem | ReviewSessionItem | ReviewPlanItem) {
  return item.coreClaim.trim() || item.selectedText.trim()
}

function reviewSource(item: ReviewQueueItem | ReviewSessionItem | ReviewPlanItem) {
  return item.author || item.title || PLATFORM_LABELS[item.platform]
}

function schedulerMeta(item: ReviewQueueItem | ReviewPlanItem) {
  const stability = item.stability < 10 ? item.stability.toFixed(1) : Math.round(item.stability).toString()
  const parts = [`稳定 ${stability} 天`, `难度 ${item.difficulty.toFixed(1)}`]
  if (item.lapseCount > 0) parts.push(`遗忘 ${item.lapseCount}`)
  if (item.scheduledDays > 0) parts.push(`上次间隔 ${item.scheduledDays} 天`)
  return parts.join(' · ')
}

function sessionTime(timestamp: number) {
  return new Intl.DateTimeFormat('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  }).format(new Date(timestamp))
}

function historyLabel(item: ReviewSessionSummary) {
  if (item.status === 'abandoned') return '已结束'
  return item.skippedCount > 0 ? `完成 · 跳过 ${item.skippedCount}` : '完成'
}

export function ReviewCenter({ onOpenKnowledge }: ReviewCenterProps) {
  const [items, setItems] = useState<ReviewQueueItem[]>([])
  const [session, setSession] = useState<ReviewSessionDetail | null>(null)
  const [history, setHistory] = useState<ReviewSessionSummary[]>([])
  const [plan, setPlan] = useState<ReviewPlanPreview | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [planning, setPlanning] = useState(false)
  const [applyingPlan, setApplyingPlan] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const current = await getCurrentReviewSession()
      const [due, recent] = await Promise.all([
        listKnowledgeReviewQueue(Date.now(), 100),
        listReviewSessionHistory(7)
      ])
      setSession(current?.status === 'active' ? current : null)
      setItems(due)
      setHistory(recent)
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const pendingItems = useMemo(
    () => session?.items.filter((item) => item.status === 'pending') ?? [],
    [session]
  )
  const firstLearningItems = useMemo(() => items.filter((item) => item.reviewCount === 0), [items])
  const scheduledReviewItems = useMemo(() => items.filter((item) => item.reviewCount > 0), [items])
  const doneCount = session ? session.completedCount + session.skippedCount : 0
  const progress = session?.itemCount ? Math.round((doneCount / session.itemCount) * 100) : 0

  const openSessionItem = (item: ReviewSessionItem) => {
    if (item.status !== 'pending') return
    onOpenKnowledge(item.knowledgeUnitId, item.questionId)
  }

  const start = async () => {
    if (busy || planning || applyingPlan) return
    setBusy(true)
    setPlan(null)
    setError(null)
    try {
      const next = await startReviewSession(Date.now(), 20)
      setSession(next)
      const first = next.items.find((item) => item.status === 'pending')
      if (first) onOpenKnowledge(first.knowledgeUnitId, first.questionId)
    } catch (startError) {
      setError(String(startError))
      await refresh()
    } finally {
      setBusy(false)
    }
  }

  const createPlan = async () => {
    if (busy || planning || applyingPlan || session) return
    setPlanning(true)
    setError(null)
    try {
      setPlan(await previewReviewPlan(Date.now(), 20))
    } catch (planError) {
      setError(String(planError))
      setPlan(null)
      await refresh()
    } finally {
      setPlanning(false)
    }
  }

  const confirmPlan = async () => {
    if (!plan || busy || planning || applyingPlan) return
    setApplyingPlan(true)
    setError(null)
    try {
      const next = await applyReviewPlan(plan)
      setPlan(null)
      setSession(next)
      const first = next.items.find((item) => item.status === 'pending')
      if (first) onOpenKnowledge(first.knowledgeUnitId, first.questionId)
    } catch (applyError) {
      setError(String(applyError))
      setPlan(null)
      await refresh()
    } finally {
      setApplyingPlan(false)
    }
  }

  const resume = () => {
    const next = pendingItems[0]
    if (next) openSessionItem(next)
  }

  const abandon = async () => {
    if (!session || busy) return
    setBusy(true)
    setError(null)
    try {
      await abandonReviewSession(session.id)
      await refresh()
    } catch (abandonError) {
      setError(String(abandonError))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="review-center" aria-label="学习与复习中心">
      <header className="workspace-page-heading review-heading">
        <div>
          <h1>学习与复习</h1>
          <p>
            {loading
              ? '正在读取…'
              : session
                ? `本次 ${doneCount} / ${session.itemCount} · ${pendingItems.length} 条待完成`
                : items.length
                  ? `${firstLearningItems.length} 条首次学习 · ${scheduledReviewItems.length} 条到期复习`
                  : '今天没有学习或复习任务'}
          </p>
        </div>
        {session && pendingItems.length > 0 ? (
          <button type="button" className="review-start" disabled={busy} onClick={resume}>
            <Play size={14} strokeWidth={1.9} />
            <span>继续</span>
          </button>
        ) : !session && plan ? (
          <div className="review-heading-actions">
            <button type="button" className="review-secondary" disabled={applyingPlan} onClick={() => setPlan(null)}>取消计划</button>
            <button type="button" className="review-start" disabled={applyingPlan} onClick={() => void confirmPlan()}>
              {applyingPlan ? <LoaderCircle size={14} className="spin" /> : <Play size={14} strokeWidth={1.9} />}
              <span>确认并开始</span>
            </button>
          </div>
        ) : !session && items.length > 0 ? (
          <div className="review-heading-actions">
            <button type="button" className="review-secondary" disabled={busy || planning} onClick={() => void start()}>直接开始</button>
            <button type="button" className="review-start" disabled={busy || planning} onClick={() => void createPlan()}>
              {planning ? <LoaderCircle size={14} className="spin" /> : <Sparkles size={14} strokeWidth={1.8} />}
              <span>{planning ? '正在安排' : '智能安排'}</span>
            </button>
          </div>
        ) : null}
      </header>

      {error && (
        <button type="button" className="workspace-error" onClick={() => void refresh()}>
          学习/复习操作失败，点击刷新
        </button>
      )}

      {loading ? (
        <div className="workspace-empty">正在读取学习与复习队列…</div>
      ) : session ? (
        <>
          <section className="review-session-card" aria-label="当前学习与复习">
            <div className="review-session-head">
              <div>
                <span>本次学习与复习</span>
                <strong>{session.itemCount} 条固定任务</strong>
                <small>开始于 {sessionTime(session.startedAt)} · 本次开始后新到期的知识不会插队</small>
              </div>
              <button type="button" disabled={busy} onClick={() => void abandon()}>
                {busy ? <LoaderCircle size={12} className="spin" /> : <X size={12} strokeWidth={1.8} />}
                <span>结束本次</span>
              </button>
            </div>
            <div className="review-progress-row">
              <div className="review-progress-track"><span style={{ width: `${progress}%` }} /></div>
              <b>{doneCount}/{session.itemCount}</b>
            </div>
          </section>

          <div className="review-session-list">
            {session.items.map((item, index) => (
              <button
                type="button"
                key={`${session.id}:${item.position}`}
                className={item.status}
                disabled={item.status !== 'pending'}
                onClick={() => openSessionItem(item)}>
                <span className="review-session-state">
                  {item.status === 'completed'
                    ? <Check size={13} strokeWidth={2} />
                    : item.status === 'skipped'
                      ? <X size={13} strokeWidth={1.8} />
                      : <Circle size={12} strokeWidth={1.7} />}
                </span>
                <span className="review-order">{String(index + 1).padStart(2, '0')}</span>
                <span className="review-list-copy">
                  <strong>{reviewTitle(item)}</strong>
                  <small>
                    {reviewSource(item)}
                    {item.wrongCount > 0 ? ' · 曾答错' : ''}
                    {item.status === 'completed' ? ' · 已完成' : item.status === 'skipped' ? ' · 已跳过' : ''}
                  </small>
                </span>
                <b>{item.masteryScore}</b>
              </button>
            ))}
          </div>
        </>
      ) : items.length === 0 ? (
        <div className="review-clear">
          <RotateCcw size={18} strokeWidth={1.7} />
          <strong>今天已清空</strong>
          <span>新知识生成题目后会先进入首次学习；完成后再按记忆状态安排复习。</span>
        </div>
      ) : plan ? (
        <>
          <section className="review-plan-card" aria-label="复习计划预览">
            <div className="review-plan-head">
              <div>
                <span><Sparkles size={13} strokeWidth={1.8} />本次计划</span>
                <strong>{plan.summary}</strong>
                <small>
                  待学习/复习 {plan.totalDue} 条 · 本次 {plan.selectedCount} 条 · {Math.max(0, plan.totalDue - plan.selectedCount)} 条留待下一轮
                </small>
              </div>
              <b>首次学习固定优先；只编排本次，不修改已存在的复习日期</b>
            </div>
          </section>

          <div className="review-plan-list">
            {plan.selected.map((item, index) => (
              <div key={item.knowledgeUnitId}>
                <span className="review-order">{String(index + 1).padStart(2, '0')}</span>
                <span className="review-list-copy">
                  <strong>{reviewTitle(item)}</strong>
                  <small>{reviewSource(item)} · {item.reason}</small>
                  <small className="review-scheduler-meta">{schedulerMeta(item)}</small>
                </span>
                <b>{item.masteryScore}</b>
              </div>
            ))}
          </div>

          {plan.deferred.length > 0 && (
            <details className="review-plan-deferred">
              <summary>留待下一轮 · 已分析 {plan.deferred.length} 条</summary>
              <div>
                {plan.deferred.map((item) => (
                  <div key={item.knowledgeUnitId}>
                    <span className="review-list-copy">
                      <strong>{reviewTitle(item)}</strong>
                      <small>{item.reason}</small>
                      <small className="review-scheduler-meta">{schedulerMeta(item)}</small>
                    </span>
                    <b>{item.masteryScore}</b>
                  </div>
                ))}
              </div>
            </details>
          )}
          {plan.totalDue > plan.analyzedCount && (
            <div className="review-queue-note">另有 {plan.totalDue - plan.analyzedCount} 条待学习/复习知识未进入本次候选窗口，仍保留到下一轮。</div>
          )}
        </>
      ) : (
        <>
          <div className="review-queue-note">首次学习会优先出现；完成第一次答题后才进入间隔复习。智能安排只影响本次顺序，不修改复习日期。</div>
          <div className="review-list">
            {items.map((item, index) => (
              <button
                type="button"
                key={item.knowledgeUnitId}
                onClick={() => onOpenKnowledge(item.knowledgeUnitId, item.questionId)}>
                <span className="review-order">{String(index + 1).padStart(2, '0')}</span>
                <span className="review-list-copy">
                  <strong>{reviewTitle(item)}</strong>
                  <small>{reviewSource(item)}{item.reviewCount === 0 ? ' · 首次学习' : item.wrongCount > 0 ? ' · 曾答错' : ''}</small>
                  <small className="review-scheduler-meta">{item.reviewCount === 0 ? '完成后开始间隔复习' : schedulerMeta(item)}</small>
                </span>
                <b>{item.masteryScore}</b>
              </button>
            ))}
          </div>
        </>
      )}

      {history.length > 0 && (
        <section className="review-history" aria-label="最近复习">
          <div className="review-history-heading">
            <History size={13} strokeWidth={1.8} />
            <strong>最近复习</strong>
          </div>
          <div className="review-history-list">
            {history.map((item) => (
              <div key={item.id}>
                <span>{sessionTime(item.startedAt)}</span>
                <strong>{item.completedCount}/{item.itemCount}</strong>
                <small>{historyLabel(item)}</small>
              </div>
            ))}
          </div>
        </section>
      )}
    </section>
  )
}
