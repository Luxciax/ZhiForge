import { listen } from '@tauri-apps/api/event'
import {
  Check,
  ChevronRight,
  CircleAlert,
  FileCheck2,
  Inbox,
  ListChecks,
  LoaderCircle,
  Pencil,
  RefreshCw,
  RotateCcw,
  Save,
  Sparkles,
  SquareArrowOutUpRight,
  X
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  acceptKnowledgeInbox,
  getKnowledgeInbox,
  ignoreKnowledgeInbox,
  internalizeKnowledge,
  listKnowledgeInbox,
  listKnowledgeLibrary,
  openSourceUrl,
  restoreKnowledgeInbox,
  updateKnowledgeInboxDraft
} from '../../services/knowledge-service'
import type { InboxDetailRecord, InboxItemRecord, KnowledgeDraftClassification, KnowledgeLibraryItem } from '../../types'
import './inbox-view.css'

const STATUS_LABELS: Record<InboxItemRecord['status'], string> = {
  captured: '待提炼',
  processing: '处理中',
  ready: '待确认',
  failed: '处理失败',
  accepted: '已入库',
  ignored: '已忽略'
}

const STAGE_LABELS: Record<string, string> = {
  captured: '等待 AI 提炼',
  extracting: '正在提炼知识',
  saving_drafts: '正在保存提炼草稿',
  classifying: '正在检查重复、补充和冲突',
  awaiting_review: '等待确认提炼结果',
  accepted: '已进入知识库',
  failed: 'AI 处理失败'
}

function excerpt(value: string, max = 120) {
  const normalized = value.replace(/\s+/g, ' ').trim()
  return normalized.length > max ? `${normalized.slice(0, max)}…` : normalized
}

function sourceTitle(item: InboxItemRecord) {
  return item.title?.trim() || excerpt(item.selectedText, 52) || '未命名来源'
}

const CLASSIFICATION_LABELS = {
  new: '新知识',
  duplicate: '重复',
  supplement: '补充',
  conflict: '冲突'
} as const

interface DraftEditState {
  draftId: string
  coreClaim: string
  concepts: string
  prerequisites: string
  importantDetails: string
  limitations: string
  classification: KnowledgeDraftClassification
  relatedKnowledgeId: string
}

function lines(values: string[]) {
  return values.join('\n')
}

function parseLines(value: string) {
  return [...new Set(value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean))]
}

type InboxListFilter = 'active' | 'accepted' | 'ignored'

function inboxFilterForStatus(status: InboxItemRecord['status']): InboxListFilter {
  if (status === 'accepted') return 'accepted'
  if (status === 'ignored') return 'ignored'
  return 'active'
}

function filterInboxItems(items: InboxItemRecord[], filter: InboxListFilter) {
  return items.filter((item) => inboxFilterForStatus(item.status) === filter)
}

interface InboxViewProps {
  initialInboxId?: string | null
  onOpenKnowledge: (knowledgeUnitId: string) => void
}

export function InboxView({ initialInboxId = null, onOpenKnowledge }: InboxViewProps) {
  const [items, setItems] = useState<InboxItemRecord[]>([])
  const [listFilter, setListFilter] = useState<InboxListFilter>('active')
  const [selectedId, setSelectedId] = useState<string | null>(initialInboxId)
  const [detail, setDetail] = useState<InboxDetailRecord | null>(null)
  const [selectedDrafts, setSelectedDrafts] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [action, setAction] = useState<'process' | 'accept' | 'ignore' | 'restore' | null>(null)
  const [draftEdit, setDraftEdit] = useState<DraftEditState | null>(null)
  const [relatedKnowledge, setRelatedKnowledge] = useState<KnowledgeLibraryItem[]>([])
  const [editSaving, setEditSaving] = useState(false)
  const [editLoadingTargets, setEditLoadingTargets] = useState(false)
  const [batchMode, setBatchMode] = useState(false)
  const [batchSelected, setBatchSelected] = useState<Set<string>>(new Set())
  const [batchAction, setBatchAction] = useState<'process' | 'ignore' | null>(null)
  const [batchIgnoreConfirm, setBatchIgnoreConfirm] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refreshList = useCallback(async () => {
    const next = await listKnowledgeInbox(200)
    setItems(next)
    return next
  }, [])

  const refreshDetail = useCallback(async (inboxId: string) => {
    setDetailLoading(true)
    try {
      const next = await getKnowledgeInbox(inboxId)
      setDetail(next)
      setListFilter(inboxFilterForStatus(next.item.status))
      setSelectedDrafts((current) => {
        if (next.item.status !== 'ready') return new Set()
        const valid = new Set(next.drafts.filter((draft) => draft.decision === 'pending').map((draft) => draft.id))
        const retained = new Set([...current].filter((id) => valid.has(id)))
        return retained.size > 0 ? retained : valid
      })
    } finally {
      setDetailLoading(false)
    }
  }, [])

  const refresh = useCallback(async () => {
    try {
      setError(null)
      const next = await refreshList()
      const visible = filterInboxItems(next, listFilter)
      const activeId = selectedId && next.some((item) => item.id === selectedId)
        ? selectedId
        : visible[0]?.id
      if (activeId) await refreshDetail(activeId)
      else setDetail(null)
    } catch (refreshError) {
      setError(String(refreshError))
    } finally {
      setLoading(false)
    }
  }, [listFilter, refreshDetail, refreshList, selectedId])

  useEffect(() => {
    void refresh()
  }, [])

  useEffect(() => {
    if (initialInboxId) setSelectedId(initialInboxId)
  }, [initialInboxId])

  useEffect(() => {
    if (!selectedId) {
      setDetail(null)
      return
    }
    void refreshDetail(selectedId).catch((detailError) => setError(String(detailError)))
  }, [selectedId, refreshDetail])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen('knowledge://changed', () => {
      if (!disposed) void refresh()
    }).then((dispose) => {
      if (disposed) dispose()
      else unlisten = dispose
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [refresh])

  const hasProcessing = items.some((item) => item.status === 'processing')
  useEffect(() => {
    if (!hasProcessing) return
    const timer = window.setInterval(() => void refresh(), 1200)
    return () => window.clearInterval(timer)
  }, [hasProcessing, refresh])

  const visibleItems = useMemo(() => filterInboxItems(items, listFilter), [items, listFilter])
  const filterCounts = useMemo(() => ({
    active: filterInboxItems(items, 'active').length,
    accepted: filterInboxItems(items, 'accepted').length,
    ignored: filterInboxItems(items, 'ignored').length
  }), [items])

  const selectedCount = selectedDrafts.size
  const allPendingDraftIds = useMemo(
    () => detail?.drafts.filter((draft) => draft.decision === 'pending').map((draft) => draft.id) ?? [],
    [detail]
  )

  const runProcessing = async () => {
    if (!detail || action) return
    setAction('process')
    setError(null)
    try {
      await internalizeKnowledge(detail.item.anchorKnowledgeId)
      await refresh()
    } catch (processError) {
      setError(String(processError))
    } finally {
      setAction(null)
    }
  }

  const acceptSelected = async () => {
    if (!detail || selectedDrafts.size === 0 || action) return
    setAction('accept')
    setError(null)
    try {
      await acceptKnowledgeInbox(detail.item.id, [...selectedDrafts])
      await refresh()
    } catch (acceptError) {
      setError(String(acceptError))
    } finally {
      setAction(null)
    }
  }

  const ignore = async () => {
    if (!detail || action) return
    setAction('ignore')
    setError(null)
    try {
      await ignoreKnowledgeInbox(detail.item.id)
      setDetail(null)
      const next = await refreshList()
      const active = filterInboxItems(next, 'active')
      setSelectedId(active[0]?.id ?? null)
    } catch (ignoreError) {
      setError(String(ignoreError))
    } finally {
      setAction(null)
    }
  }

  const restoreIgnored = async () => {
    if (!detail || detail.item.status !== 'ignored' || action) return
    setAction('restore')
    setError(null)
    try {
      const restored = await restoreKnowledgeInbox(detail.item.id)
      setListFilter('active')
      setSelectedId(restored.id)
      await refreshList()
      await refreshDetail(restored.id)
    } catch (restoreError) {
      setError(`恢复失败：${String(restoreError)}`)
    } finally {
      setAction(null)
    }
  }

  const changeListFilter = (nextFilter: InboxListFilter) => {
    setBatchMode(false)
    setBatchSelected(new Set())
    setBatchIgnoreConfirm(false)
    setDraftEdit(null)
    setListFilter(nextFilter)
    const next = filterInboxItems(items, nextFilter)
    setSelectedId(next[0]?.id ?? null)
  }

  const toggleDraft = (draftId: string) => {
    setSelectedDrafts((current) => {
      const next = new Set(current)
      if (next.has(draftId)) next.delete(draftId)
      else next.add(draftId)
      return next
    })
  }

  const openDraftEditor = async () => {
    if (!detail || selectedDrafts.size !== 1 || editSaving) return
    const draftId = [...selectedDrafts][0]
    const draft = detail.drafts.find((item) => item.id === draftId)
    if (!draft || draft.decision !== 'pending') return
    setDraftEdit({
      draftId: draft.id,
      coreClaim: draft.coreClaim,
      concepts: lines(draft.concepts),
      prerequisites: lines(draft.prerequisites),
      importantDetails: lines(draft.importantDetails),
      limitations: lines(draft.limitations),
      classification: draft.classification,
      relatedKnowledgeId: draft.relatedKnowledgeId ?? ''
    })
    setEditLoadingTargets(true)
    try {
      setRelatedKnowledge(await listKnowledgeLibrary({ limit: 100 }))
    } catch (loadError) {
      setError(`现有知识列表读取失败：${String(loadError)}`)
    } finally {
      setEditLoadingTargets(false)
    }
  }

  const saveDraftEdit = async () => {
    if (!detail || !draftEdit || editSaving) return
    if (!draftEdit.coreClaim.trim()) {
      setError('核心知识不能为空。')
      return
    }
    if (draftEdit.classification !== 'new' && !draftEdit.relatedKnowledgeId) {
      setError('重复、补充或冲突必须选择一条现有知识。')
      return
    }
    setEditSaving(true)
    setError(null)
    try {
      await updateKnowledgeInboxDraft(draftEdit.draftId, {
        coreClaim: draftEdit.coreClaim.trim(),
        concepts: parseLines(draftEdit.concepts),
        prerequisites: parseLines(draftEdit.prerequisites),
        importantDetails: parseLines(draftEdit.importantDetails),
        limitations: parseLines(draftEdit.limitations),
        classification: draftEdit.classification,
        relatedKnowledgeId: draftEdit.classification === 'new' ? null : draftEdit.relatedKnowledgeId
      })
      setDraftEdit(null)
      await refreshDetail(detail.item.id)
    } catch (saveError) {
      setError(`草稿修正失败：${String(saveError)}`)
    } finally {
      setEditSaving(false)
    }
  }

  const editingDraft = draftEdit ? detail?.drafts.find((draft) => draft.id === draftEdit.draftId) ?? null : null
  const batchItems = items.filter((item) => batchSelected.has(item.id))
  const batchProcessable = batchItems.filter((item) => item.status === 'captured' || item.status === 'failed')
  const batchIgnorable = batchItems.filter((item) => item.status === 'captured' || item.status === 'ready' || item.status === 'failed')

  const toggleBatchMode = () => {
    setBatchMode((current) => {
      if (current) {
        setBatchSelected(new Set())
        setBatchIgnoreConfirm(false)
      }
      return !current
    })
  }

  const toggleBatchItem = (inboxId: string) => {
    setBatchSelected((current) => {
      const next = new Set(current)
      if (next.has(inboxId)) next.delete(inboxId)
      else next.add(inboxId)
      return next
    })
    setBatchIgnoreConfirm(false)
  }

  const processBatch = async () => {
    if (batchAction || batchProcessable.length === 0) return
    setBatchAction('process')
    setError(null)
    let failed = 0
    for (const item of batchProcessable.slice(0, 20)) {
      try {
        await internalizeKnowledge(item.anchorKnowledgeId)
      } catch (processError) {
        failed += 1
        console.error('Failed to schedule batch Inbox processing', processError)
      }
    }
    if (failed > 0) setError(`批量提炼有 ${failed} 项启动失败，其余项目已继续处理。`)
    setBatchAction(null)
    setBatchSelected(new Set())
    await refresh()
  }

  const ignoreBatch = async () => {
    if (batchAction || batchIgnorable.length === 0) return
    if (!batchIgnoreConfirm) {
      setBatchIgnoreConfirm(true)
      return
    }
    setBatchAction('ignore')
    setError(null)
    let failed = 0
    for (const item of batchIgnorable) {
      try {
        await ignoreKnowledgeInbox(item.id)
      } catch (ignoreError) {
        failed += 1
        console.error('Failed to ignore Inbox item in batch', ignoreError)
      }
    }
    if (failed > 0) setError(`批量忽略有 ${failed} 项失败，其余项目已隐藏。`)
    setBatchAction(null)
    setBatchIgnoreConfirm(false)
    setBatchSelected(new Set())
    await refresh()
  }

  if (loading) {
    return <div className="inbox-loading"><LoaderCircle size={18} strokeWidth={1.8} className="spin" />正在读取收件箱</div>
  }

  return (
    <section className="inbox-view">
      <header className="inbox-header">
        <div>
          <span>知识入口</span>
          <h1>收件箱</h1>
          <p>新捕获的来源先在这里提炼和确认，再进入正式知识库。</p>
        </div>
        <div className="inbox-header-actions">
          {listFilter === 'active' && (
            <button type="button" className={batchMode ? 'active' : undefined} disabled={Boolean(batchAction)} onClick={toggleBatchMode} title={batchMode ? '退出批量处理' : '批量处理'}>
              <ListChecks size={14} strokeWidth={1.8} />
            </button>
          )}
          <button type="button" className="inbox-refresh" disabled={Boolean(batchAction)} onClick={() => void refresh()} title="刷新">
            <RefreshCw size={14} strokeWidth={1.8} />
          </button>
        </div>
      </header>

      <div className="inbox-filter-tabs" role="tablist" aria-label="收件箱筛选">
        <button type="button" className={listFilter === 'active' ? 'active' : undefined} onClick={() => changeListFilter('active')}>待处理 <span>{filterCounts.active}</span></button>
        <button type="button" className={listFilter === 'accepted' ? 'active' : undefined} onClick={() => changeListFilter('accepted')}>已入库 <span>{filterCounts.accepted}</span></button>
        <button type="button" className={listFilter === 'ignored' ? 'active' : undefined} onClick={() => changeListFilter('ignored')}>已忽略 <span>{filterCounts.ignored}</span></button>
      </div>

      {error && <div className="inbox-error"><CircleAlert size={14} strokeWidth={1.8} /><span>{error}</span></div>}

      {batchMode && visibleItems.length > 0 && (
        <div className="inbox-batch-bar">
          <div>
            <strong>已选 {batchSelected.size} 项</strong>
            <span>{batchProcessable.length} 项可开始/重试提炼 · {batchIgnorable.length} 项可忽略</span>
          </div>
          <div>
            <button type="button" disabled={batchProcessable.length === 0 || Boolean(batchAction)} onClick={() => void processBatch()}>
              {batchAction === 'process' ? <LoaderCircle size={12} className="spin" /> : <Sparkles size={12} strokeWidth={1.8} />}
              {batchAction === 'process' ? '正在启动' : '提炼 / 重试'}
            </button>
            <button type="button" className={batchIgnoreConfirm ? 'danger-confirm' : undefined} disabled={batchIgnorable.length === 0 || Boolean(batchAction)} onClick={() => void ignoreBatch()}>
              {batchAction === 'ignore' ? <LoaderCircle size={12} className="spin" /> : <X size={12} strokeWidth={1.8} />}
              {batchAction === 'ignore' ? '正在忽略' : batchIgnoreConfirm ? `确认忽略 ${batchIgnorable.length} 项` : '忽略'}
            </button>
          </div>
        </div>
      )}

      {visibleItems.length === 0 ? (
        <div className="inbox-empty">
          <Inbox size={20} strokeWidth={1.7} />
          <strong>{listFilter === 'active' ? '没有待处理内容' : listFilter === 'accepted' ? '还没有已入库记录' : '没有已忽略内容'}</strong>
          <span>{listFilter === 'active' ? '从知乎搜索、主题研究或划词入口加入内容后，会先出现在这里。' : listFilter === 'accepted' ? '接收知识草稿后，来源会保留在这里作为处理历史。' : '忽略的来源会保留在这里，并且可以随时恢复。'}</span>
        </div>
      ) : (
        <div className="inbox-layout">
          <div className="inbox-list" role="list">
            {visibleItems.map((item) => (
              <button
                type="button"
                role="listitem"
                key={item.id}
                className={`${selectedId === item.id && !batchMode ? 'active' : ''}${batchSelected.has(item.id) ? ' batch-selected' : ''}`.trim() || undefined}
                onClick={() => batchMode ? toggleBatchItem(item.id) : setSelectedId(item.id)}>
                {batchMode && <span className="inbox-batch-check">{batchSelected.has(item.id) && <Check size={11} strokeWidth={2.2} />}</span>}
                <div className="inbox-list-copy">
                  <div className="inbox-list-topline">
                    <span className={`inbox-status ${item.status}`}>{STATUS_LABELS[item.status]}</span>
                    <small>{item.platform === 'zhihu' ? '知乎' : item.platform}</small>
                  </div>
                  <strong>{sourceTitle(item)}</strong>
                  <span>{item.author || excerpt(item.selectedText, 76)}</span>
                </div>
                {!batchMode && <ChevronRight size={14} strokeWidth={1.7} />}
              </button>
            ))}
          </div>

          <div className="inbox-detail">
            {detailLoading && !detail ? (
              <div className="inbox-loading"><LoaderCircle size={17} strokeWidth={1.8} className="spin" />读取提炼结果</div>
            ) : detail ? (
              <>
                <div className="inbox-source-head">
                  <div className="inbox-source-title">
                    <span>{detail.item.platform === 'zhihu' ? '知乎来源' : detail.item.platform}</span>
                    <h2>{sourceTitle(detail.item)}</h2>
                    <p>{detail.item.author || '未知作者'}</p>
                  </div>
                  {detail.item.url && (
                    <button type="button" onClick={() => void openSourceUrl(detail.item.url!)} title="打开原始来源">
                      <SquareArrowOutUpRight size={14} strokeWidth={1.8} />
                    </button>
                  )}
                </div>

                <div className="inbox-source-excerpt">{excerpt(detail.item.selectedText, 360)}</div>

                {detail.item.status === 'captured' && (
                  <div className="inbox-action-card">
                    <Sparkles size={17} strokeWidth={1.7} />
                    <div><strong>等待提炼</strong><span>AI 会先生成知识草稿，不会直接写入正式知识库。</span></div>
                    <button type="button" disabled={Boolean(action)} onClick={() => void runProcessing()}>
                      {action === 'process' && <LoaderCircle size={13} className="spin" />}
                      开始提炼
                    </button>
                  </div>
                )}

                {detail.item.status === 'processing' && (
                  <div className="inbox-action-card processing">
                    <LoaderCircle size={17} strokeWidth={1.7} className="spin" />
                    <div>
                      <strong>{STAGE_LABELS[detail.item.processingStage] || 'AI 正在处理'}</strong>
                      <span>{detail.item.progressTotal > 0 ? `${detail.item.progressCurrent} / ${detail.item.progressTotal}` : '正在准备处理流程'}</span>
                      {detail.item.progressTotal > 0 && (
                        <div className="inbox-progress"><i style={{ width: `${Math.min(100, detail.item.progressCurrent / detail.item.progressTotal * 100)}%` }} /></div>
                      )}
                    </div>
                  </div>
                )}

                {detail.item.status === 'failed' && (
                  <div className="inbox-action-card failed">
                    <CircleAlert size={17} strokeWidth={1.7} />
                    <div><strong>AI 处理失败</strong><span>{detail.item.error || '没有返回错误信息。'}</span></div>
                    <button type="button" disabled={Boolean(action)} onClick={() => void runProcessing()}>
                      {action === 'process' ? <LoaderCircle size={13} className="spin" /> : <RefreshCw size={13} />}
                      重新提炼
                    </button>
                  </div>
                )}

                {detail.item.status === 'ready' && (
                  <>
                    {detail.item.error && (
                      <div className="inbox-classification-warning">
                        <CircleAlert size={13} strokeWidth={1.8} />
                        <span>{detail.item.error}</span>
                      </div>
                    )}
                    <div className="inbox-draft-toolbar">
                      <div>
                        <strong>AI 提炼了 {detail.drafts.length} 条知识草稿</strong>
                        <span>取消不值得独立保存的条目；AI 判断不准确时可以先修正再入库。</span>
                      </div>
                      <div className="inbox-draft-toolbar-actions">
                        <button type="button" disabled={selectedCount !== 1 || Boolean(action)} onClick={() => void openDraftEditor()}>
                          <Pencil size={12} strokeWidth={1.8} />修正选中
                        </button>
                        <button
                          type="button"
                          className="select-all"
                          onClick={() => setSelectedDrafts(selectedCount === allPendingDraftIds.length ? new Set() : new Set(allPendingDraftIds))}>
                          {selectedCount === allPendingDraftIds.length ? '取消全选' : '全选'}
                        </button>
                      </div>
                    </div>

                    {draftEdit && editingDraft && (
                      <section className="inbox-draft-editor" aria-label="修正知识草稿">
                        <div className="inbox-draft-editor-head">
                          <div>
                            <strong>修正知识草稿</strong>
                            <span>Evidence 保持原文只读；这里只修正知识表达和入库关系。</span>
                          </div>
                          <button type="button" disabled={editSaving} onClick={() => setDraftEdit(null)}><X size={13} strokeWidth={1.8} /></button>
                        </div>
                        <label className="draft-edit-wide">
                          <span>核心知识</span>
                          <textarea rows={3} value={draftEdit.coreClaim} onChange={(event) => setDraftEdit((current) => current ? { ...current, coreClaim: event.target.value } : current)} />
                        </label>
                        <div className="draft-edit-grid">
                          <label>
                            <span>入库判断</span>
                            <select value={draftEdit.classification} onChange={(event) => setDraftEdit((current) => current ? { ...current, classification: event.target.value as KnowledgeDraftClassification, relatedKnowledgeId: event.target.value === 'new' ? '' : current.relatedKnowledgeId } : current)}>
                              <option value="new">新知识</option>
                              <option value="duplicate">重复：证据合并到已有知识</option>
                              <option value="supplement">补充：保留新知识并建立 extends</option>
                              <option value="conflict">冲突：保留双方并建立 contradicts</option>
                            </select>
                          </label>
                          <label>
                            <span>关联现有知识</span>
                            <select
                              value={draftEdit.relatedKnowledgeId}
                              disabled={draftEdit.classification === 'new' || editLoadingTargets}
                              onChange={(event) => setDraftEdit((current) => current ? { ...current, relatedKnowledgeId: event.target.value } : current)}>
                              <option value="">{editLoadingTargets ? '正在读取…' : draftEdit.classification === 'new' ? '新知识不需要关联' : '选择一条现有知识…'}</option>
                              {editingDraft.relatedKnowledgeId && !relatedKnowledge.some((item) => item.id === editingDraft.relatedKnowledgeId) && (
                                <option value={editingDraft.relatedKnowledgeId}>{editingDraft.relatedCoreClaim || editingDraft.relatedKnowledgeId}</option>
                              )}
                              {relatedKnowledge.map((item) => <option key={item.id} value={item.id}>{item.coreClaim || item.selectedText}</option>)}
                            </select>
                          </label>
                          <label>
                            <span>概念 · 每行一项</span>
                            <textarea rows={4} value={draftEdit.concepts} onChange={(event) => setDraftEdit((current) => current ? { ...current, concepts: event.target.value } : current)} />
                          </label>
                          <label>
                            <span>前置知识 · 每行一项</span>
                            <textarea rows={4} value={draftEdit.prerequisites} onChange={(event) => setDraftEdit((current) => current ? { ...current, prerequisites: event.target.value } : current)} />
                          </label>
                          <label>
                            <span>重要细节 · 每行一项</span>
                            <textarea rows={4} value={draftEdit.importantDetails} onChange={(event) => setDraftEdit((current) => current ? { ...current, importantDetails: event.target.value } : current)} />
                          </label>
                          <label>
                            <span>局限 / 条件 · 每行一项</span>
                            <textarea rows={4} value={draftEdit.limitations} onChange={(event) => setDraftEdit((current) => current ? { ...current, limitations: event.target.value } : current)} />
                          </label>
                        </div>
                        <div className="draft-edit-evidence">
                          <span>原文 Evidence · 只读</span>
                          <blockquote>{editingDraft.evidence.join('\n\n')}</blockquote>
                        </div>
                        <div className="draft-edit-actions">
                          <button type="button" disabled={editSaving} onClick={() => setDraftEdit(null)}>取消</button>
                          <button type="button" className="primary" disabled={editSaving || !draftEdit.coreClaim.trim() || (draftEdit.classification !== 'new' && !draftEdit.relatedKnowledgeId)} onClick={() => void saveDraftEdit()}>
                            {editSaving ? <LoaderCircle size={12} className="spin" /> : <Save size={12} strokeWidth={1.9} />}
                            {editSaving ? '保存中' : '保存修正'}
                          </button>
                        </div>
                      </section>
                    )}

                    <div className="inbox-drafts">
                      {detail.drafts.map((draft) => {
                        const checked = selectedDrafts.has(draft.id)
                        return (
                          <button
                            type="button"
                            key={draft.id}
                            className={checked ? 'selected' : undefined}
                            onClick={() => toggleDraft(draft.id)}>
                            <span className="draft-check">{checked && <Check size={12} strokeWidth={2.2} />}</span>
                            <div>
                              <div className="draft-heading-row">
                                <strong>{draft.coreClaim}</strong>
                                <span className={`draft-classification ${draft.classification}`}>{CLASSIFICATION_LABELS[draft.classification]}</span>
                              </div>
                              {draft.relatedCoreClaim && draft.classification !== 'new' && (
                                <div className="draft-related">
                                  <span>{draft.classification === 'duplicate' ? '将合并到' : draft.classification === 'conflict' ? '与此冲突' : '补充现有知识'}</span>
                                  <strong>{draft.relatedCoreClaim}</strong>
                                </div>
                              )}
                              {draft.rationale && <p className="draft-rationale">{draft.rationale}{draft.confidence != null ? ` · ${Math.round(draft.confidence * 100)}%` : ''}</p>}
                              {draft.importantDetails.length > 0 && <p>{draft.importantDetails.slice(0, 2).join(' · ')}</p>}
                              {draft.evidence[0] && <blockquote>{excerpt(draft.evidence[0], 190)}</blockquote>}
                              {draft.concepts.length > 0 && (
                                <div className="draft-concepts">{draft.concepts.slice(0, 6).map((concept) => <span key={concept}>{concept}</span>)}</div>
                              )}
                            </div>
                          </button>
                        )
                      })}
                    </div>

                    <div className="inbox-ready-actions">
                      <button type="button" className="secondary" disabled={Boolean(action)} onClick={() => void ignore()}>
                        <X size={13} strokeWidth={1.9} />忽略来源
                      </button>
                      <button type="button" className="primary" disabled={selectedCount === 0 || Boolean(action)} onClick={() => void acceptSelected()}>
                        {action === 'accept' ? <LoaderCircle size={13} className="spin" /> : <FileCheck2 size={13} strokeWidth={1.9} />}
                        接收选中 {selectedCount} 条
                      </button>
                    </div>
                  </>
                )}

                {detail.item.status === 'accepted' && (
                  <>
                    <div className="inbox-action-card accepted">
                      <FileCheck2 size={17} strokeWidth={1.7} />
                      <div><strong>已进入知识库</strong><span>这里保留本次处理结果；每条已接收草稿都可以追踪到最终正式知识。</span></div>
                    </div>
                    <section className="inbox-accepted-results" aria-label="已入库处理结果">
                      <div className="inbox-accepted-heading">
                        <div>
                          <strong>处理结果</strong>
                          <span>{detail.drafts.filter((draft) => draft.decision === 'accepted').length} 条已接收 · {detail.drafts.filter((draft) => draft.decision === 'ignored').length} 条未入库</span>
                        </div>
                      </div>
                      <div className="inbox-accepted-list">
                        {detail.drafts.filter((draft) => draft.decision === 'accepted').map((draft) => (
                          <button
                            type="button"
                            key={draft.id}
                            disabled={!draft.acceptedKnowledgeId}
                            onClick={() => draft.acceptedKnowledgeId && onOpenKnowledge(draft.acceptedKnowledgeId)}>
                            <div>
                              <span className={`draft-classification ${draft.classification}`}>{CLASSIFICATION_LABELS[draft.classification]}</span>
                              <strong>{draft.acceptedCoreClaim || draft.coreClaim}</strong>
                              <small>
                                {draft.classification === 'duplicate'
                                  ? '证据已合并到现有知识'
                                  : draft.classification === 'supplement'
                                    ? '已创建知识并建立补充关系'
                                    : draft.classification === 'conflict'
                                      ? '已保留双方并建立冲突关系'
                                      : '已创建为正式知识'}
                                {!draft.acceptedKnowledgeId ? ' · 旧记录未找到唯一目标' : ''}
                              </small>
                            </div>
                            {draft.acceptedKnowledgeId && <ChevronRight size={14} strokeWidth={1.8} />}
                          </button>
                        ))}
                      </div>
                      {detail.drafts.some((draft) => draft.decision === 'ignored') && (
                        <details className="inbox-skipped-drafts">
                          <summary>查看本次未入库草稿</summary>
                          <div>
                            {detail.drafts.filter((draft) => draft.decision === 'ignored').map((draft) => (
                              <div key={draft.id}>
                                <strong>{draft.coreClaim}</strong>
                                <span>未选择入库</span>
                              </div>
                            ))}
                          </div>
                        </details>
                      )}
                    </section>
                  </>
                )}

                {detail.item.status === 'ignored' && (
                  <div className="inbox-action-card ignored">
                    <RotateCcw size={17} strokeWidth={1.7} />
                    <div><strong>已忽略，但没有删除</strong><span>Source 和已有知识草稿仍然保留。恢复后，有草稿的项目回到待确认，否则回到待提炼。</span></div>
                    <button type="button" disabled={Boolean(action)} onClick={() => void restoreIgnored()}>
                      {action === 'restore' ? <LoaderCircle size={13} className="spin" /> : <RotateCcw size={13} strokeWidth={1.8} />}
                      {action === 'restore' ? '正在恢复' : '恢复到收件箱'}
                    </button>
                  </div>
                )}
              </>
            ) : (
              <div className="inbox-empty"><Inbox size={20} strokeWidth={1.7} /><span>选择一个来源查看处理状态。</span></div>
            )}
          </div>
        </div>
      )}
    </section>
  )
}
