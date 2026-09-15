import { emit } from '@tauri-apps/api/event'
import {
  BookOpenText,
  Check,
  CircleCheck,
  FileText,
  ExternalLink,
  LoaderCircle,
  RefreshCcw,
  Search,
  Sparkles,
  X
} from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react'
import {
  analyzeResearchPlan,
  captureKnowledge,
  getLatestResearchPlan,
  getResearchPlan,
  internalizeKnowledge,
  linkResearchGapSource,
  listResearchGapSources,
  listResearchPlans,
  openSourceUrl,
  setResearchGapStatus
} from '../../services/knowledge-service'
import { searchZhihuExpanded } from '../../services/zhihu-service'
import type { ResearchGapRecord, ResearchPlanDetail, ResearchPlanSummary, SourceLibraryItem, ZhihuSearchItem } from '../../types'
import './topic-expansion.css'

interface TopicExpansionViewProps {
  onOpenKnowledge: (knowledgeUnitId: string) => void
  onOpenSource: (sourceId: string) => void
  onOpenSettings: () => void
  initialPlanId?: string | null
  initialGapId?: string | null
}

const SEARCH_INTERVAL_MS = 5_000

const PRIORITY_LABELS = {
  high: '高优先级',
  medium: '中优先级',
  low: '低优先级'
} as const

const GAP_STATUS_LABELS = {
  open: '待研究',
  collecting: '收集中',
  covered: '已补全',
  dismissed: '已忽略'
} as const

const PLAN_STATUS_LABELS = {
  active: '进行中',
  completed: '已完成',
  archived: '历史'
} as const

function planTime(timestamp: number) {
  return new Intl.DateTimeFormat('zh-CN', {
    month: 'numeric',
    day: 'numeric'
  }).format(new Date(timestamp))
}

function externalKey(item: ZhihuSearchItem) {
  if (item.url.trim()) return `url:${item.url.trim()}`
  if (item.contentId.trim()) return `${item.contentType}:${item.contentId}`
  return `${item.title}:${item.authorName}`
}

function alreadyLinked(item: ZhihuSearchItem, sources: SourceLibraryItem[]) {
  const url = item.url.trim()
  if (url && sources.some((source) => source.url?.trim() === url)) return true
  const text = item.contentText.trim()
  return Boolean(text) && sources.some((source) => source.selectedText.trim() === text)
}

function excerpt(value: string, limit = 220) {
  const normalized = value.replace(/\s+/g, ' ').trim()
  if (!normalized) return '该候选没有返回可直接内化的正文，可先打开知乎查看。'
  return normalized.length > limit ? `${normalized.slice(0, limit)}…` : normalized
}

function sleep(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function replaceGap(plan: ResearchPlanDetail, nextGap: ResearchGapRecord): ResearchPlanDetail {
  const gaps = plan.gaps.map((gap) => gap.id === nextGap.id ? nextGap : gap)
  const remaining = gaps.filter((gap) => gap.status === 'open' || gap.status === 'collecting').length
  return { ...plan, gaps, status: remaining === 0 ? 'completed' : 'active', updatedAt: Date.now() }
}

export function TopicExpansionView({
  onOpenKnowledge,
  onOpenSource,
  onOpenSettings,
  initialPlanId = null,
  initialGapId = null
}: TopicExpansionViewProps) {
  const [topic, setTopic] = useState('')
  const [plan, setPlan] = useState<ResearchPlanDetail | null>(null)
  const [planHistory, setPlanHistory] = useState<ResearchPlanSummary[]>([])
  const [sourcesByGap, setSourcesByGap] = useState<Record<string, SourceLibraryItem[]>>({})
  const [switchingPlanId, setSwitchingPlanId] = useState<string | null>(null)
  const [activeGapId, setActiveGapId] = useState<string | null>(null)
  const [resultsByGap, setResultsByGap] = useState<Record<string, ZhihuSearchItem[]>>({})
  const [loadingPlan, setLoadingPlan] = useState(false)
  const [searchingGapId, setSearchingGapId] = useState<string | null>(null)
  const [searchingAll, setSearchingAll] = useState(false)
  const [capturingKey, setCapturingKey] = useState<string | null>(null)
  const [capturedKeys, setCapturedKeys] = useState<Set<string>>(new Set())
  const [error, setError] = useState<string | null>(null)
  const [warning, setWarning] = useState<string | null>(null)

  const refreshGapSources = useCallback(async (nextPlan: ResearchPlanDetail) => {
    try {
      const entries = await Promise.all(
        nextPlan.gaps
          .filter((gap) => gap.sourceIds.length > 0)
          .map(async (gap) => [gap.id, await listResearchGapSources(gap.id)] as const)
      )
      setSourcesByGap(Object.fromEntries(entries))
    } catch (sourceError) {
      console.error('Failed to load research gap sources', sourceError)
    }
  }, [])

  const refreshPlanHistory = useCallback(async () => {
    try {
      setPlanHistory(await listResearchPlans(12))
    } catch (historyError) {
      console.error('Failed to load research plan history', historyError)
    }
  }, [])

  useEffect(() => {
    let disposed = false
    const load = initialPlanId ? getResearchPlan(initialPlanId) : getLatestResearchPlan()
    void Promise.all([load, listResearchPlans(12)])
      .then(([latest, history]) => {
        if (disposed) return
        setPlanHistory(history)
        if (!latest) return
        setPlan(latest)
        void refreshGapSources(latest)
        setTopic(latest.topic)
        const requestedGap = initialGapId && latest.gaps.some((gap) => gap.id === initialGapId)
          ? initialGapId
          : null
        setActiveGapId(requestedGap ?? latest.gaps.find((gap) => gap.status === 'open' || gap.status === 'collecting')?.id ?? latest.gaps[0]?.id ?? null)
      })
      .catch((loadError) => console.error('Failed to restore research workspace', loadError))
    return () => { disposed = true }
  }, [initialGapId, initialPlanId, refreshGapSources])

  const activeGap = useMemo(
    () => plan?.gaps.find((gap) => gap.id === activeGapId) ?? null,
    [activeGapId, plan]
  )
  const activeResults = activeGap ? resultsByGap[activeGap.id] ?? [] : []
  const activeSources = activeGap ? sourcesByGap[activeGap.id] ?? [] : []
  const planReadOnly = plan?.status === 'archived'
  const openGapCount = plan?.gaps.filter((gap) => gap.status === 'open').length ?? 0
  const collectingGapCount = plan?.gaps.filter((gap) => gap.status === 'collecting').length ?? 0
  const coveredGapCount = plan?.gaps.filter((gap) => gap.status === 'covered').length ?? 0

  const reloadPlan = async (topicValue: string) => {
    const latest = await getLatestResearchPlan(topicValue)
    if (latest) setPlan(latest)
    return latest
  }

  const openPlan = async (planId: string) => {
    if (switchingPlanId || loadingPlan || planId === plan?.id) return
    setSwitchingPlanId(planId)
    setError(null)
    setWarning(null)
    setResultsByGap({})
    setCapturedKeys(new Set())
    setSourcesByGap({})
    try {
      const next = await getResearchPlan(planId)
      setPlan(next)
      await refreshGapSources(next)
      setTopic(next.topic)
      setActiveGapId(next.gaps.find((gap) => gap.status === 'open' || gap.status === 'collecting')?.id ?? next.gaps[0]?.id ?? null)
    } catch (loadError) {
      setError(`研究计划读取失败：${String(loadError)}`)
    } finally {
      setSwitchingPlanId(null)
    }
  }

  const runAnalysis = async (event?: FormEvent) => {
    event?.preventDefault()
    const value = topic.replace(/\s+/g, ' ').trim()
    if (value.length < 2 || loadingPlan) return
    setLoadingPlan(true)
    setError(null)
    setWarning(null)
    setResultsByGap({})
    setCapturedKeys(new Set())
    setSourcesByGap({})
    try {
      const next = await analyzeResearchPlan(value)
      setPlan(next)
      setTopic(next.topic)
      setActiveGapId(next.gaps[0]?.id ?? null)
      await refreshPlanHistory()
    } catch (analysisError) {
      setError(`缺口分析失败：${String(analysisError)}`)
    } finally {
      setLoadingPlan(false)
    }
  }

  const performGapSearch = async (gap: ResearchGapRecord) => {
    const external = await searchZhihuExpanded(gap.searchQuery, 20)
    const seen = new Set<string>()
    const items = external.items.filter((item) => {
      const key = externalKey(item)
      if (seen.has(key)) return false
      seen.add(key)
      return true
    })
    setResultsByGap((current) => ({ ...current, [gap.id]: items }))
    return items
  }

  const searchGap = async (gap: ResearchGapRecord) => {
    if (planReadOnly || searchingGapId || searchingAll) return
    setActiveGapId(gap.id)
    setSearchingGapId(gap.id)
    setError(null)
    try {
      await performGapSearch(gap)
    } catch (searchError) {
      setError(`“${gap.title}”检索失败：${String(searchError)}`)
    } finally {
      setSearchingGapId(null)
    }
  }

  const searchAllGaps = async () => {
    if (!plan || planReadOnly || searchingAll || searchingGapId) return
    const pending = plan.gaps.filter((gap) => gap.status === 'open' || gap.status === 'collecting')
    if (pending.length === 0) return
    setSearchingAll(true)
    setError(null)
    setWarning(null)
    const failed: string[] = []
    for (const [index, gap] of pending.entries()) {
      setActiveGapId(gap.id)
      setSearchingGapId(gap.id)
      try {
        await performGapSearch(gap)
      } catch {
        failed.push(gap.title)
      }
      if (index < pending.length - 1) await sleep(SEARCH_INTERVAL_MS)
    }
    setSearchingGapId(null)
    setSearchingAll(false)
    if (failed.length > 0) {
      setWarning(`有 ${failed.length} 个缺口检索失败，已保留其余成功结果：${failed.join('、')}`)
    }
  }

  const updateGapStatus = async (gap: ResearchGapRecord, status: 'open' | 'covered' | 'dismissed') => {
    if (!plan || planReadOnly) return
    setError(null)
    try {
      const nextGap = await setResearchGapStatus(gap.id, status)
      setPlan((current) => current ? replaceGap(current, nextGap) : current)
      await reloadPlan(plan.topic)
      await refreshPlanHistory()
    } catch (statusError) {
      setError(`更新研究缺口失败：${String(statusError)}`)
    }
  }

  const internalizeCandidate = async (gap: ResearchGapRecord, item: ZhihuSearchItem) => {
    const key = `${gap.id}:${externalKey(item)}`
    const linkedSources = sourcesByGap[gap.id] ?? []
    if (planReadOnly || capturingKey || capturedKeys.has(key) || alreadyLinked(item, linkedSources) || !item.contentText.trim()) return
    setCapturingKey(key)
    setError(null)
    try {
      const captured = await captureKnowledge({
        platform: 'zhihu',
        selectedText: item.contentText.trim(),
        url: item.url || undefined,
        title: item.title || gap.searchQuery || plan?.topic || undefined,
        author: item.authorName || undefined
      })
      try {
        await internalizeKnowledge(captured.knowledgeUnit.id)
      } catch (jobError) {
        console.error('Research source was saved but AI processing could not be scheduled', jobError)
      }
      const nextGap = await linkResearchGapSource(gap.id, captured.source.id)
      setPlan((current) => current ? replaceGap(current, nextGap) : current)
      const nextSources = await listResearchGapSources(gap.id)
      setSourcesByGap((current) => ({ ...current, [gap.id]: nextSources }))
      await refreshPlanHistory()
      await emit('knowledge://changed', { knowledgeUnitId: captured.knowledgeUnit.id })
      setCapturedKeys((current) => new Set(current).add(key))
    } catch (captureError) {
      setError(`加入收件箱失败：${String(captureError)}`)
    } finally {
      setCapturingKey(null)
    }
  }

  return (
    <section className="topic-expansion-view" aria-label="主题研究计划">
      <header className="topic-expansion-header">
        <div>
          <span>知识库生长</span>
          <h1>知识缺口与主题研究</h1>
          <p>先让 AI 只基于你已经内化的知识判断“还缺什么”，再围绕具体缺口去知乎补材料。避免漫无目的收藏和重复知识。</p>
        </div>
      </header>

      <form className="topic-expansion-form" onSubmit={(event) => void runAnalysis(event)}>
        <Search size={18} strokeWidth={1.8} />
        <input
          autoFocus
          value={topic}
          maxLength={120}
          placeholder="例如：AI Agent 长期记忆、复杂任务规划、甜菜病害管理…"
          onChange={(event) => setTopic(event.target.value)}
        />
        <button type="submit" disabled={loadingPlan || topic.trim().length < 2}>
          {loadingPlan ? <LoaderCircle size={14} strokeWidth={1.8} className="spin" /> : <Sparkles size={14} strokeWidth={1.8} />}
          <span>{loadingPlan ? '正在分析' : plan ? '重新分析' : '分析知识缺口'}</span>
        </button>
      </form>

      {error && (
        <div className="topic-expansion-error">
          <span>{error}</span>
          {error.includes('Access Secret') && <button type="button" onClick={onOpenSettings}>配置知乎 Access Secret</button>}
        </div>
      )}
      {warning && <div className="topic-expansion-warning">{warning}</div>}

      {planHistory.length > 0 && (
        <section className="research-history" aria-label="最近研究计划">
          <div className="research-history-head">
            <strong>最近研究</strong>
            <span>{planHistory.length} 个计划</span>
          </div>
          <div className="research-history-list">
            {planHistory.map((item) => {
              const unfinished = item.openCount + item.collectingCount
              const active = item.id === plan?.id
              return (
                <button
                  type="button"
                  key={item.id}
                  className={active ? 'active' : undefined}
                  disabled={Boolean(switchingPlanId)}
                  onClick={() => void openPlan(item.id)}>
                  <div>
                    <span className={`research-plan-status ${item.status}`}>{PLAN_STATUS_LABELS[item.status]}</span>
                    <small>{planTime(item.updatedAt)}</small>
                  </div>
                  <strong>{item.topic}</strong>
                  <span>{item.coverageScore}% 覆盖 · {unfinished > 0 ? `${unfinished} 个未完成缺口` : `${item.coveredCount} 个缺口已补全`}</span>
                  {switchingPlanId === item.id && <LoaderCircle size={12} strokeWidth={1.8} className="spin" />}
                </button>
              )
            })}
          </div>
        </section>
      )}

      {!plan && !loadingPlan && (
        <div className="topic-expansion-empty">
          <BookOpenText size={19} strokeWidth={1.7} />
          <strong>输入一个你真正想建立体系的主题</strong>
          <span>系统会读取相关正式知识，形成可推进的研究计划：现有覆盖、缺口、优先级和下一条知乎搜索词。</span>
        </div>
      )}

      {plan && (
        <div className="research-plan-shell">
          <section className="research-plan-summary">
            <div className="coverage-score" aria-label={`当前覆盖度 ${plan.coverageScore}%`}>
              <strong>{plan.coverageScore}</strong>
              <span>当前覆盖</span>
            </div>
            <div className="research-plan-summary-copy">
              <div className="research-plan-title-row">
                <div>
                  <span>研究主题</span>
                  <h2>{plan.topic}</h2>
                </div>
                <button type="button" disabled={planReadOnly || searchingAll || Boolean(searchingGapId)} onClick={() => void searchAllGaps()}>
                  {searchingAll ? <LoaderCircle size={13} strokeWidth={1.8} className="spin" /> : <Search size={13} strokeWidth={1.8} />}
                  <span>{searchingAll ? '逐项检索中' : '搜索全部未完成缺口'}</span>
                </button>
              </div>
              <p>{plan.summary}</p>
              {planReadOnly && (
                <div className="research-plan-readonly">这是已被新分析取代的历史计划，仅供回看。要继续推进，请在上方重新分析这个主题。</div>
              )}
              <div className="research-plan-stats">
                <span>{plan.knowledge.length} 条相关本地知识</span>
                <span>{openGapCount} 待研究</span>
                <span>{collectingGapCount} 收集中</span>
                <span>{coveredGapCount} 已补全</span>
              </div>
            </div>
          </section>

          <div className="research-plan-grid">
            <section className="research-existing-column">
              <div className="topic-expansion-section-head">
                <div>
                  <strong>当前已有</strong>
                  <span>{plan.knowledge.length} 条用于判断覆盖度</span>
                </div>
              </div>
              {plan.knowledge.length === 0 ? (
                <div className="topic-expansion-note">当前知识库没有召回到这个主题的正式知识，因此计划会从基础研究问题开始。</div>
              ) : (
                <div className="research-knowledge-list">
                  {plan.knowledge.map((item) => (
                    <button type="button" key={item.id} onClick={() => onOpenKnowledge(item.id)}>
                      <strong>{item.coreClaim}</strong>
                      <span>{item.author || item.title || '本地知识'} · 掌握度 {item.masteryScore}% · {item.qualityStatus}</span>
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section className="research-gap-column">
              <div className="topic-expansion-section-head">
                <div>
                  <strong>研究缺口</strong>
                  <span>{plan.gaps.length} 项 · 按优先级排列</span>
                </div>
              </div>
              <div className="research-gap-list">
                {plan.gaps.map((gap) => {
                  const active = gap.id === activeGapId
                  const searching = gap.id === searchingGapId
                  const resultCount = resultsByGap[gap.id]?.length ?? 0
                  return (
                    <article key={gap.id} className={`${active ? 'active' : ''} ${gap.status}`}>
                      <button type="button" className="research-gap-main" onClick={() => setActiveGapId(gap.id)}>
                        <div className="research-gap-meta">
                          <span className={`priority ${gap.priority}`}>{PRIORITY_LABELS[gap.priority]}</span>
                          <span className={`gap-status ${gap.status}`}>{GAP_STATUS_LABELS[gap.status]}</span>
                          {gap.sourceIds.length > 0 && <small>{gap.sourceIds.length} 个来源</small>}
                        </div>
                        <strong>{gap.title}</strong>
                        <p>{gap.rationale}</p>
                        <code>{gap.searchQuery}</code>
                      </button>
                      <div className="research-gap-actions">
                        <button
                          type="button"
                          disabled={planReadOnly || searchingAll || Boolean(searchingGapId) || gap.status === 'dismissed'}
                          onClick={() => void searchGap(gap)}>
                          {searching ? <LoaderCircle size={12} strokeWidth={1.8} className="spin" /> : resultCount > 0 ? <RefreshCcw size={12} strokeWidth={1.8} /> : <Search size={12} strokeWidth={1.8} />}
                          <span>{searching ? '搜索中' : resultCount > 0 ? `${resultCount} 条，重搜` : '搜知乎'}</span>
                        </button>
                        {gap.status !== 'covered' ? (
                          <button type="button" disabled={planReadOnly || gap.status === 'dismissed'} onClick={() => void updateGapStatus(gap, 'covered')}>
                            <Check size={12} strokeWidth={1.8} />
                            <span>标记补全</span>
                          </button>
                        ) : (
                          <button type="button" disabled={planReadOnly} onClick={() => void updateGapStatus(gap, 'open')}>
                            <RefreshCcw size={12} strokeWidth={1.8} />
                            <span>重新打开</span>
                          </button>
                        )}
                        {gap.status !== 'dismissed' && (
                          <button type="button" className="quiet" disabled={planReadOnly} onClick={() => void updateGapStatus(gap, 'dismissed')}>
                            <X size={12} strokeWidth={1.8} />
                            <span>忽略</span>
                          </button>
                        )}
                      </div>
                    </article>
                  )
                })}
              </div>
            </section>
          </div>

          {activeGap && (
            <section className="research-results-section">
              <div className="research-results-head">
                <div>
                  <span>当前缺口</span>
                  <strong>{activeGap.title}</strong>
                  <small>知乎查询：{activeGap.searchQuery}</small>
                </div>
                {!resultsByGap[activeGap.id] && !searchingGapId && (
                  <button type="button" disabled={planReadOnly} onClick={() => void searchGap(activeGap)}>
                    <Search size={13} strokeWidth={1.8} />
                    <span>开始搜索</span>
                  </button>
                )}
              </div>

              {activeSources.length > 0 && (
                <div className="research-linked-sources">
                  <div className="research-linked-sources-head">
                    <FileText size={13} strokeWidth={1.8} />
                    <strong>已采来源</strong>
                    <span>{activeSources.length} 个</span>
                  </div>
                  <div className="research-linked-source-list">
                    {activeSources.map((source) => (
                      <button type="button" key={source.id} onClick={() => onOpenSource(source.id)}>
                        <strong>{source.title || source.author || '未命名来源'}</strong>
                        <span>{source.author || (source.platform === 'zhihu' ? '知乎' : '来源')} · {source.knowledgeCount > 0 ? `已形成 ${source.knowledgeCount} 条知识` : '等待内化 / 确认'}</span>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {searchingGapId === activeGap.id && activeResults.length === 0 ? (
                <div className="topic-expansion-note">正在检索这个缺口的知乎材料…</div>
              ) : activeResults.length === 0 ? (
                <div className="topic-expansion-note">还没有为这个缺口执行搜索，或者没有找到候选材料。</div>
              ) : (
                <div className="topic-external-list">
                  {activeResults.map((item) => {
                    const key = `${activeGap.id}:${externalKey(item)}`
                    const captured = capturedKeys.has(key) || alreadyLinked(item, activeSources)
                    return (
                      <article key={key}>
                        <div className="topic-external-copy">
                          <div className="topic-external-meta">
                            <span>{item.contentType === 'Article' ? '文章' : '回答'}</span>
                            {item.authorName && <strong>{item.authorName}</strong>}
                            {item.voteUpCount > 0 && <small>{item.voteUpCount} 赞同</small>}
                          </div>
                          <h2>{item.title || activeGap.searchQuery}</h2>
                          <p>{excerpt(item.contentText)}</p>
                        </div>
                        <div className="topic-external-actions">
                          {item.url && (
                            <button type="button" onClick={() => void openSourceUrl(item.url)}>
                              <ExternalLink size={12} strokeWidth={1.8} />
                              <span>打开知乎</span>
                            </button>
                          )}
                          <button
                            type="button"
                            className="primary"
                            disabled={planReadOnly || captured || Boolean(capturingKey) || !item.contentText.trim()}
                            onClick={() => void internalizeCandidate(activeGap, item)}>
                            {capturingKey === key ? <LoaderCircle size={12} strokeWidth={1.8} className="spin" /> : captured ? <CircleCheck size={12} strokeWidth={1.8} /> : null}
                            <span>{captured ? '已加入这个缺口' : capturingKey === key ? '正在加入' : '加入收件箱'}</span>
                          </button>
                        </div>
                      </article>
                    )
                  })}
                </div>
              )}
            </section>
          )}
        </div>
      )}
    </section>
  )
}
