import { listen } from '@tauri-apps/api/event'
import { Archive, Check, ChevronLeft, ExternalLink, FolderTree, LoaderCircle, Play, Plus, RotateCw, Search, Tag, Trash2, Waypoints, X } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useRef, useState } from 'react'
import {
  addKnowledgeTag,
  archiveKnowledge,
  assignTopic,
  createKnowledgeRelation,
  getKnowledgeDetail,
  getKnowledgeManagementDetail,
  generateKnowledgeQuestions,
  internalizeKnowledge,
  listKnowledge,
  listKnowledgeLibrary,
  listKnowledgeReviewQueue,
  listTopics,
  openSourceUrl,
  removeKnowledgeRelation,
  removeKnowledgeTag,
  trashKnowledge,
  unassignTopic,
  updateKnowledgeNote,
  type ReviewQueueItem
} from '../../services/knowledge-service'
import type {
  AiJobStatus,
  EvidenceRecord,
  KnowledgeDetail,
  KnowledgeListItem,
  KnowledgeLibraryItem,
  KnowledgeManagementDetail,
  KnowledgeQualityStatus,
  KnowledgeRelationType,
  KnowledgeStatus,
  SourcePlatform,
  TopicRecord
} from '../../types'
import { KnowledgeQuiz } from './KnowledgeQuiz'
import { LibrarianView } from './LibrarianView'
import { ZhihuSearchView } from './ZhihuSearchView'
import './knowledge-home.css'

const RELATION_LABELS: Record<KnowledgeRelationType, string> = {
  related_to: '相关',
  supports: '支持',
  contradicts: '冲突',
  example_of: '示例',
  prerequisite_of: '前置',
  derived_from: '来源于',
  extends: '扩展'
}

const RELATION_OPTIONS: Array<{ value: KnowledgeRelationType; label: string }> = [
  { value: 'related_to', label: '相关' },
  { value: 'supports', label: '支持' },
  { value: 'contradicts', label: '冲突' },
  { value: 'example_of', label: '示例' },
  { value: 'prerequisite_of', label: '前置' },
  { value: 'derived_from', label: '来源于' },
  { value: 'extends', label: '扩展' }
]

const STATUS_LABELS: Record<KnowledgeStatus, string> = {
  captured: '待提炼',
  processed: '已处理',
  learning: '学习中',
  reviewing: '复习中',
  weak: '薄弱',
  mastered: '已掌握'
}

const QUALITY_LABELS: Record<KnowledgeQualityStatus, string> = {
  unverified: '未验证',
  verified: '已验证',
  conflicted: '有冲突',
  stale: '可能过时',
  needs_expansion: '待扩展'
}

const JOB_LABELS: Record<AiJobStatus, string> = {
  pending: '等待处理',
  running: 'AI 处理中',
  completed: '已提炼',
  failed: 'AI 失败'
}

const PLATFORM_LABELS: Record<SourcePlatform, string> = {
  zhihu: '知乎',
  web: '网页',
  pdf: 'PDF',
  desktop: '桌面',
  unknown: '来源未知'
}

function itemTitle(item: KnowledgeListItem) {
  return item.coreClaim.trim() || item.selectedText.trim()
}

function itemSource(item: KnowledgeListItem) {
  return item.author || item.title || PLATFORM_LABELS[item.platform]
}

function itemStatus(item: KnowledgeListItem) {
  if (item.aiJobStatus === 'failed' && item.coreClaim.trim()) return '待出题'
  if (item.aiJobStatus === 'pending' || item.aiJobStatus === 'running' || item.aiJobStatus === 'failed') {
    return JOB_LABELS[item.aiJobStatus]
  }
  if (item.reviewCount === 0 && item.aiJobStatus === 'completed') return '待检验'
  return STATUS_LABELS[item.status]
}

function detailSource(detail: KnowledgeDetail) {
  return detail.source.author || detail.source.title || PLATFORM_LABELS[detail.source.platform]
}

function reviewTitle(item: ReviewQueueItem) {
  return item.coreClaim.trim() || item.selectedText.trim()
}

function reviewSource(item: ReviewQueueItem) {
  return item.author || item.title || PLATFORM_LABELS[item.platform]
}

function sourceWithHighlightedEvidence(text: string, evidence: EvidenceRecord | null) {
  if (!evidence) return { before: text, highlight: '', after: '' }
  const chars = Array.from(text)
  let start = evidence.startOffset === null ? -1 : Math.max(0, Math.min(chars.length, evidence.startOffset))
  let end = evidence.endOffset === null ? -1 : Math.max(start, Math.min(chars.length, evidence.endOffset))
  let highlight = start >= 0 && end >= start ? chars.slice(start, end).join('') : ''
  if (highlight !== evidence.text) {
    const byteStart = text.indexOf(evidence.text)
    if (byteStart < 0) return { before: text, highlight: '', after: '' }
    start = Array.from(text.slice(0, byteStart)).length
    end = start + Array.from(evidence.text).length
    highlight = evidence.text
  }
  return {
    before: chars.slice(0, start).join(''),
    highlight,
    after: chars.slice(end).join('')
  }
}

interface KnowledgeHomeProps {
  onOpenSettings: () => void
  initialKnowledgeId?: string | null
  initialQuestionId?: string | null
  detailBackLabel?: string
  onDetailClose?: () => void
  onReviewContinue?: () => void
  onOpenGraph?: (knowledgeUnitId: string) => void
}

export function KnowledgeHome({
  onOpenSettings,
  initialKnowledgeId = null,
  initialQuestionId = null,
  detailBackLabel = '最近知识',
  onDetailClose,
  onReviewContinue,
  onOpenGraph
}: KnowledgeHomeProps) {
  const [items, setItems] = useState<KnowledgeListItem[]>([])
  const [reviewQueue, setReviewQueue] = useState<ReviewQueueItem[]>([])
  const [loading, setLoading] = useState(true)
  const [loadFailed, setLoadFailed] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [preferredQuestionId, setPreferredQuestionId] = useState<string | null>(null)
  const [detail, setDetail] = useState<KnowledgeDetail | null>(null)
  const [detailLoading, setDetailLoading] = useState(false)
  const [retrying, setRetrying] = useState(false)
  const [questionGenerating, setQuestionGenerating] = useState(false)
  const [questionError, setQuestionError] = useState<string | null>(null)
  const [sourceOpen, setSourceOpen] = useState(false)
  const [focusedEvidenceId, setFocusedEvidenceId] = useState<string | null>(null)
  const [searchOpen, setSearchOpen] = useState(false)
  const [librarianOpen, setLibrarianOpen] = useState(false)
  const [searchSeed, setSearchSeed] = useState('')
  const [homeSearch, setHomeSearch] = useState('')
  const [management, setManagement] = useState<KnowledgeManagementDetail | null>(null)
  const [availableTopics, setAvailableTopics] = useState<TopicRecord[]>([])
  const [relationCandidates, setRelationCandidates] = useState<KnowledgeLibraryItem[]>([])
  const [relationType, setRelationType] = useState<KnowledgeRelationType>('related_to')
  const [relationTargetId, setRelationTargetId] = useState('')
  const [noteDraft, setNoteDraft] = useState('')
  const [noteSaving, setNoteSaving] = useState(false)
  const [noteSaved, setNoteSaved] = useState(false)
  const [tagDraft, setTagDraft] = useState('')
  const [organizing, setOrganizing] = useState(false)
  const [trashConfirming, setTrashConfirming] = useState(false)
  const sourceRef = useRef<HTMLDetailsElement | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [recent, due] = await Promise.all([
        listKnowledge(12),
        listKnowledgeReviewQueue(Date.now(), 20)
      ])
      setItems(recent)
      setReviewQueue(due)
      setLoadFailed(false)
    } catch (error) {
      console.error('Failed to load knowledge library', error)
      setLoadFailed(true)
    } finally {
      setLoading(false)
    }
  }, [])

  const refreshDetail = useCallback(async (knowledgeUnitId: string) => {
    setDetailLoading(true)
    try {
      const [nextDetail, nextManagement, topicList, candidates] = await Promise.all([
        getKnowledgeDetail(knowledgeUnitId),
        getKnowledgeManagementDetail(knowledgeUnitId),
        listTopics(),
        listKnowledgeLibrary({ includeArchived: false, limit: 200 })
      ])
      setDetail(nextDetail)
      setManagement(nextManagement)
      setAvailableTopics(topicList)
      setRelationCandidates(candidates.filter((item) => item.id !== knowledgeUnitId))
      setRelationTargetId('')
      setNoteDraft(nextDetail.knowledgeUnit.userNote ?? '')
      setTrashConfirming(false)
    } catch (error) {
      console.error('Failed to load knowledge detail', error)
      setDetail(null)
      setManagement(null)
    } finally {
      setDetailLoading(false)
    }
  }, [])

  const openSearch = (seed: string) => {
    setSearchSeed(seed)
    setSearchOpen(true)
  }

  const submitHomeSearch = (event: FormEvent) => {
    event.preventDefault()
    const value = homeSearch.trim()
    if (value.length >= 2) openSearch(value)
  }

  const openDetail = (knowledgeUnitId: string, questionId: string | null = null) => {
    setSearchOpen(false)
    setPreferredQuestionId(questionId)
    setSourceOpen(false)
    setFocusedEvidenceId(null)
    setTrashConfirming(false)
    setSelectedId(knowledgeUnitId)
    void refreshDetail(knowledgeUnitId)
  }

  const closeDetail = () => {
    setSelectedId(null)
    setPreferredQuestionId(null)
    setSourceOpen(false)
    setFocusedEvidenceId(null)
    setTrashConfirming(false)
    setDetail(null)
    setManagement(null)
    onDetailClose?.()
  }

  const refreshManagement = async (knowledgeUnitId: string) => {
    const [nextManagement, topicList] = await Promise.all([
      getKnowledgeManagementDetail(knowledgeUnitId),
      listTopics()
    ])
    setManagement(nextManagement)
    setAvailableTopics(topicList)
  }

  const saveNote = async () => {
    if (!selectedId || noteSaving) return
    setNoteSaving(true)
    setNoteSaved(false)
    try {
      const stored = await updateKnowledgeNote(selectedId, noteDraft)
      setDetail((current) => current ? {
        ...current,
        knowledgeUnit: { ...current.knowledgeUnit, userNote: stored ?? undefined }
      } : current)
      setNoteSaved(true)
      window.setTimeout(() => setNoteSaved(false), 1400)
      await refresh()
    } catch (error) {
      console.error('Failed to save knowledge note', error)
    } finally {
      setNoteSaving(false)
    }
  }

  const addTopic = async (topicId: string) => {
    if (!selectedId || !topicId || organizing) return
    setOrganizing(true)
    try {
      await assignTopic(selectedId, topicId)
      await refreshManagement(selectedId)
    } catch (error) {
      console.error('Failed to assign topic', error)
    } finally {
      setOrganizing(false)
    }
  }

  const removeTopic = async (topicId: string) => {
    if (!selectedId || organizing) return
    setOrganizing(true)
    try {
      await unassignTopic(selectedId, topicId)
      await refreshManagement(selectedId)
    } catch (error) {
      console.error('Failed to remove topic', error)
    } finally {
      setOrganizing(false)
    }
  }

  const addTag = async (event: FormEvent) => {
    event.preventDefault()
    const value = tagDraft.trim()
    if (!selectedId || !value || organizing) return
    setOrganizing(true)
    try {
      await addKnowledgeTag(selectedId, value)
      setTagDraft('')
      await refreshManagement(selectedId)
    } catch (error) {
      console.error('Failed to add tag', error)
    } finally {
      setOrganizing(false)
    }
  }

  const removeTag = async (tagId: string) => {
    if (!selectedId || organizing) return
    setOrganizing(true)
    try {
      await removeKnowledgeTag(selectedId, tagId)
      await refreshManagement(selectedId)
    } catch (error) {
      console.error('Failed to remove tag', error)
    } finally {
      setOrganizing(false)
    }
  }

  const createRelation = async (event: FormEvent) => {
    event.preventDefault()
    if (!selectedId || !relationTargetId || organizing) return
    setOrganizing(true)
    try {
      await createKnowledgeRelation(selectedId, relationTargetId, relationType)
      setRelationTargetId('')
      await refreshManagement(selectedId)
    } catch (error) {
      console.error('Failed to create knowledge relation', error)
    } finally {
      setOrganizing(false)
    }
  }

  const deleteRelation = async (relationId: string) => {
    if (!selectedId || organizing) return
    setOrganizing(true)
    try {
      await removeKnowledgeRelation(relationId)
      await refreshManagement(selectedId)
    } catch (error) {
      console.error('Failed to remove knowledge relation', error)
    } finally {
      setOrganizing(false)
    }
  }

  const toggleArchive = async () => {
    if (!selectedId || !management || organizing) return
    setOrganizing(true)
    try {
      await archiveKnowledge(selectedId, management.archivedAt === null)
      await Promise.all([refreshManagement(selectedId), refresh()])
    } catch (error) {
      console.error('Failed to update archive state', error)
    } finally {
      setOrganizing(false)
    }
  }

  const confirmTrash = async () => {
    if (!selectedId || organizing) return
    setOrganizing(true)
    try {
      await trashKnowledge(selectedId)
      await refresh()
      closeDetail()
    } catch (error) {
      console.error('Failed to move knowledge to trash', error)
    } finally {
      setOrganizing(false)
    }
  }

  const locateEvidence = (evidence: EvidenceRecord) => {
    setFocusedEvidenceId(evidence.id)
    setSourceOpen(true)
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => sourceRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }))
    })
  }

  const retry = async () => {
    if (!selectedId || retrying) return
    setRetrying(true)
    try {
      await internalizeKnowledge(selectedId)
      await refreshDetail(selectedId)
      await refresh()
    } catch (error) {
      console.error('Failed to retry knowledge internalization', error)
    } finally {
      setRetrying(false)
    }
  }

  const generateQuestions = async () => {
    if (!selectedId || questionGenerating) return
    setQuestionGenerating(true)
    setQuestionError(null)
    try {
      await generateKnowledgeQuestions(selectedId)
      await refreshDetail(selectedId)
      await refresh()
    } catch (error) {
      const message = String(error)
      setQuestionError(message)
      console.error('Failed to generate knowledge questions', error)
    } finally {
      setQuestionGenerating(false)
    }
  }

  useEffect(() => {
    if (!initialKnowledgeId) return
    setSearchOpen(false)
    setPreferredQuestionId(initialQuestionId)
    setSourceOpen(false)
    setFocusedEvidenceId(null)
    setSelectedId(initialKnowledgeId)
    void refreshDetail(initialKnowledgeId)
  }, [initialKnowledgeId, initialQuestionId, refreshDetail])

  useEffect(() => {
    let disposed = false
    const cleanup: Array<() => void> = []

    void refresh()
    const handleFocus = () => {
      void refresh()
      if (selectedId) void refreshDetail(selectedId)
    }
    window.addEventListener('focus', handleFocus)

    void Promise.all([
      listen<{ knowledgeUnitId?: string }>('knowledge://changed', (event) => {
        if (disposed) return
        void refresh()
        if (selectedId && (!event.payload.knowledgeUnitId || event.payload.knowledgeUnitId === selectedId)) {
          void refreshDetail(selectedId)
        }
      }),
      listen<string>('zhihu://search-request', (event) => {
        if (disposed) return
        setSelectedId(null)
        setDetail(null)
        setSearchSeed(event.payload || '')
        setSearchOpen(true)
      }),
      listen('app://home', () => {
        if (disposed) return
        setSearchOpen(false)
        setSearchSeed('')
        setSelectedId(null)
        setPreferredQuestionId(null)
        setDetail(null)
      })
    ]).then((unlisteners) => {
      if (disposed) unlisteners.forEach((unlisten) => unlisten())
      else cleanup.push(...unlisteners)
    })

    return () => {
      disposed = true
      window.removeEventListener('focus', handleFocus)
      cleanup.forEach((unlisten) => unlisten())
    }
  }, [refresh, refreshDetail, selectedId])

  if (librarianOpen) {
    return (
      <LibrarianView
        onBack={() => setLibrarianOpen(false)}
        onChanged={() => void refresh()}
        onOpenKnowledge={(knowledgeUnitId) => {
          setLibrarianOpen(false)
          openDetail(knowledgeUnitId)
        }}
      />
    )
  }

  if (searchOpen) {
    return (
      <ZhihuSearchView
        initialQuery={searchSeed}
        onBack={() => {
          setSearchOpen(false)
          setSearchSeed('')
        }}
        onCaptured={() => void refresh()}
        onOpenSettings={onOpenSettings}
      />
    )
  }

  if (selectedId) {
    return (
      <section className="knowledge-recent knowledge-detail" aria-label="知识详情">
        <div className="knowledge-detail-heading">
          <button type="button" className="knowledge-back" onClick={closeDetail}>
            <ChevronLeft size={14} strokeWidth={1.8} />
            <span>{detailBackLabel}</span>
          </button>
          <div className="knowledge-detail-heading-actions">
            {detail && <span>{detailSource(detail)}</span>}
            {detail && onOpenGraph && (
              <button type="button" className="knowledge-map-jump" onClick={() => onOpenGraph(detail.knowledgeUnit.id)}>
                <Waypoints size={13} strokeWidth={1.8} />
                <span>知识地图</span>
              </button>
            )}
          </div>
        </div>

        {detailLoading && !detail ? (
          <div className="knowledge-home-empty">正在读取知识详情…</div>
        ) : !detail ? (
          <div className="knowledge-home-empty">知识详情读取失败。</div>
        ) : (
          <div className="knowledge-detail-scroll">
            {(detail.aiJob?.status === 'pending' || detail.aiJob?.status === 'running') && (
              <div className="knowledge-job-banner processing">
                <LoaderCircle size={14} strokeWidth={1.8} className="spin" />
                <div>
                  <strong>{detail.aiJob.jobType === 'question_generation' ? 'AI 正在生成理解题' : '原文已保存，AI 正在提炼'}</strong>
                  <span>{detail.aiJob.jobType === 'question_generation' ? '核心知识和证据已经保存，只处理题目。' : '提炼结果会先保存；题目生成失败也不会丢失知识。'}</span>
                </div>
              </div>
            )}

            {detail.aiJob?.status === 'failed' && (
              <div className="knowledge-job-banner failed">
                <div>
                  <strong>{detail.knowledgeUnit.coreClaim && detail.questions.length === 0 ? '知识已提炼，理解题生成失败。' : '原文已保存，AI 处理失败。'}</strong>
                  <span className="knowledge-job-error">{detail.aiJob.error || '没有返回可用的错误信息。'}</span>
                </div>
                {detail.knowledgeUnit.coreClaim && detail.evidence.length > 0 && detail.questions.length === 0 ? (
                  <button type="button" onClick={() => void generateQuestions()} disabled={questionGenerating}>
                    <RotateCw size={13} strokeWidth={1.8} className={questionGenerating ? 'spin' : undefined} />
                    <span>{questionGenerating ? '出题中' : '重新出题'}</span>
                  </button>
                ) : (
                  <button type="button" onClick={() => void retry()} disabled={retrying}>
                    <RotateCw size={13} strokeWidth={1.8} className={retrying ? 'spin' : undefined} />
                    <span>{retrying ? '重试中' : '重新处理'}</span>
                  </button>
                )}
              </div>
            )}

            {detail.knowledgeUnit.coreClaim && (
              <section className="knowledge-detail-card primary">
                <div className="knowledge-section-heading">
                  <span className="knowledge-detail-label">核心知识</span>
                  {detail.reviewState && <span>{detail.reviewState.reviewCount === 0 ? '待检验' : `${STATUS_LABELS[detail.knowledgeUnit.status]} · ${detail.reviewState.masteryScore}%`}</span>}
                </div>
                <strong>{detail.knowledgeUnit.coreClaim}</strong>
                {detail.knowledgeUnit.concepts.length > 0 && (
                  <div className="knowledge-concepts">
                    {detail.knowledgeUnit.concepts.map((concept) => <span key={concept}>{concept}</span>)}
                  </div>
                )}
              </section>
            )}

            <section className="knowledge-detail-section knowledge-basis-section">
              <div className="knowledge-section-heading">
                <span className="knowledge-detail-label">知识依据</span>
                <span className={`knowledge-quality-badge ${detail.qualityState.status}`}>{QUALITY_LABELS[detail.qualityState.status]}</span>
              </div>
              <div className="knowledge-basis-summary">
                <span>{detail.sources.length} 个来源</span>
                <span>{detail.claims.reduce((count, claim) => count + claim.evidence.filter((evidence) => evidence.stance === 'supports').length, 0)} 条支持证据</span>
                {detail.claims.some((claim) => claim.evidence.some((evidence) => evidence.stance === 'conflicts')) && (
                  <span className="conflicted">{detail.claims.reduce((count, claim) => count + claim.evidence.filter((evidence) => evidence.stance === 'conflicts').length, 0)} 条冲突证据</span>
                )}
              </div>
              {detail.qualityState.reason && <p className="knowledge-quality-reason">{detail.qualityState.reason}</p>}
              {detail.claims.length === 0 ? (
                <div className="knowledge-basis-empty">这条内容尚未形成可审计 Claim，完成 AI 提炼后会自动建立。</div>
              ) : (
                <div className="knowledge-claim-list">
                  {detail.claims.map((claim) => (
                    <article className="knowledge-claim" key={claim.id}>
                      <div className="knowledge-claim-heading">
                        <span>{claim.claimType === 'primary' ? '核心主张' : '补充主张'}</span>
                        <small>{claim.evidence.length} 条 Evidence</small>
                      </div>
                      <p>{claim.text}</p>
                      {claim.evidence.length > 0 && (
                        <div className="knowledge-claim-evidence-list">
                          {claim.evidence.map((evidence) => {
                            const localEvidence = detail.evidence.find((item) => item.id === evidence.evidenceId)
                            const sourceLabel = evidence.sourceAuthor || evidence.sourceTitle || PLATFORM_LABELS[evidence.sourcePlatform]
                            return (
                              <div className={`knowledge-claim-evidence ${evidence.stance}`} key={`${claim.id}:${evidence.evidenceId}:${evidence.stance}`}>
                                <div>
                                  <span>{evidence.stance === 'supports' ? '支持' : '冲突'}</span>
                                  <small>{sourceLabel}</small>
                                </div>
                                <p>{evidence.text}</p>
                                {localEvidence ? (
                                  <button type="button" onClick={() => locateEvidence(localEvidence)}>定位原文</button>
                                ) : evidence.sourceUrl ? (
                                  <button type="button" onClick={() => void openSourceUrl(evidence.sourceUrl!).catch((error) => console.error('Failed to open evidence source URL', error))}>
                                    <ExternalLink size={11} strokeWidth={1.8} />
                                    <span>打开来源</span>
                                  </button>
                                ) : null}
                              </div>
                            )
                          })}
                        </div>
                      )}
                    </article>
                  ))}
                </div>
              )}
            </section>

            <section className="knowledge-detail-section knowledge-note-section">
              <div className="knowledge-section-heading">
                <span className="knowledge-detail-label">我的理解</span>
                <span>只由你编辑，AI 不会自动覆盖</span>
              </div>
              <textarea
                value={noteDraft}
                placeholder="用自己的话写下理解、疑问或以后要补充的内容…"
                onChange={(event) => {
                  setNoteDraft(event.target.value)
                  setNoteSaved(false)
                }}
              />
              <div className="knowledge-note-actions">
                <span>{noteDraft === (detail.knowledgeUnit.userNote ?? '') ? '已同步' : '有未保存修改'}</span>
                <button
                  type="button"
                  disabled={noteSaving || noteDraft === (detail.knowledgeUnit.userNote ?? '')}
                  onClick={() => void saveNote()}>
                  {noteSaving ? <LoaderCircle size={13} strokeWidth={1.8} className="spin" /> : noteSaved ? <Check size={13} strokeWidth={1.9} /> : null}
                  <span>{noteSaving ? '保存中' : noteSaved ? '已保存' : '保存'}</span>
                </button>
              </div>
            </section>

            {management && (
              <section className="knowledge-detail-section knowledge-organize-section">
                <div className="knowledge-section-heading">
                  <span className="knowledge-detail-label">整理</span>
                  <span>主题可以多选，标签用于轻量标记</span>
                </div>

                <div className="knowledge-organize-group">
                  <label>主题</label>
                  <div className="knowledge-organize-row">
                    <div className="knowledge-organize-chips">
                      {management.topics.map((topic) => (
                        <span className="topic-chip editable" key={topic.id}>
                          {topic.name}
                          <button type="button" aria-label={`移除主题 ${topic.name}`} disabled={organizing} onClick={() => void removeTopic(topic.id)}>
                            <X size={11} strokeWidth={1.9} />
                          </button>
                        </span>
                      ))}
                      {management.topics.length === 0 && <span className="knowledge-organize-empty">未归类</span>}
                    </div>
                    <select
                      value=""
                      disabled={organizing}
                      onChange={(event) => {
                        const topicId = event.target.value
                        if (topicId) void addTopic(topicId)
                      }}>
                      <option value="">加入主题…</option>
                      {availableTopics
                        .filter((topic) => !management.topics.some((assigned) => assigned.id === topic.id))
                        .map((topic) => <option key={topic.id} value={topic.id}>{topic.name}</option>)}
                    </select>
                  </div>
                </div>

                <div className="knowledge-organize-group">
                  <label>标签</label>
                  <div className="knowledge-organize-row">
                    <div className="knowledge-organize-chips">
                      {management.tags.map((tag) => (
                        <span className="tag-chip editable" key={tag.id}>
                          #{tag.name}
                          <button type="button" aria-label={`移除标签 ${tag.name}`} disabled={organizing} onClick={() => void removeTag(tag.id)}>
                            <X size={11} strokeWidth={1.9} />
                          </button>
                        </span>
                      ))}
                      {management.tags.length === 0 && <span className="knowledge-organize-empty">暂无标签</span>}
                    </div>
                    <form className="knowledge-tag-add" onSubmit={(event) => void addTag(event)}>
                      <Tag size={13} strokeWidth={1.8} />
                      <input value={tagDraft} maxLength={80} placeholder="添加标签" onChange={(event) => setTagDraft(event.target.value)} />
                      <button type="submit" disabled={!tagDraft.trim() || organizing} aria-label="添加标签">
                        <Plus size={12} strokeWidth={1.9} />
                      </button>
                    </form>
                  </div>
                </div>
              </section>
            )}

            {(detail.knowledgeUnit.prerequisites.length > 0 || detail.knowledgeUnit.importantDetails.length > 0 || detail.knowledgeUnit.limitations.length > 0) && (
              <details className="knowledge-structure-section">
                <summary>查看知识结构</summary>
                {detail.knowledgeUnit.prerequisites.length > 0 && (
                  <div><strong>前置知识</strong><ul>{detail.knowledgeUnit.prerequisites.map((item) => <li key={item}>{item}</li>)}</ul></div>
                )}
                {detail.knowledgeUnit.importantDetails.length > 0 && (
                  <div><strong>重要细节</strong><ul>{detail.knowledgeUnit.importantDetails.map((item) => <li key={item}>{item}</li>)}</ul></div>
                )}
                {detail.knowledgeUnit.limitations.length > 0 && (
                  <div><strong>局限与条件</strong><ul>{detail.knowledgeUnit.limitations.map((item) => <li key={item}>{item}</li>)}</ul></div>
                )}
              </details>
            )}

            {management && (
              <section className="knowledge-detail-section knowledge-relations-section">
                <div className="knowledge-section-heading">
                  <span className="knowledge-detail-label">关系</span>
                  <span>{management.relations.length ? `${management.relations.length} 条` : '建立知识之间的显式关系'}</span>
                </div>
                {management.relations.length > 0 && (
                  <div className="knowledge-relation-list">
                    {management.relations.map((relation) => (
                      <div className="knowledge-relation-row" key={relation.id}>
                        <button type="button" onClick={() => openDetail(relation.otherKnowledgeId)}>
                          <span>{RELATION_LABELS[relation.relationType]}</span>
                          <strong>{relation.otherTitle}</strong>
                        </button>
                        <button type="button" aria-label="移除关系" disabled={organizing} onClick={() => void deleteRelation(relation.id)}>
                          <X size={12} strokeWidth={1.8} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
                <form className="knowledge-relation-add" onSubmit={(event) => void createRelation(event)}>
                  <select value={relationType} disabled={organizing} onChange={(event) => setRelationType(event.target.value as KnowledgeRelationType)}>
                    {RELATION_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                  </select>
                  <select value={relationTargetId} disabled={organizing} onChange={(event) => setRelationTargetId(event.target.value)}>
                    <option value="">选择另一条知识…</option>
                    {relationCandidates
                      .filter((candidate) => !management.relations.some((relation) => relation.otherKnowledgeId === candidate.id && relation.relationType === relationType))
                      .map((candidate) => (
                        <option key={candidate.id} value={candidate.id}>{candidate.coreClaim.trim() || candidate.selectedText.trim()}</option>
                      ))}
                  </select>
                  <button type="submit" disabled={!relationTargetId || organizing}>
                    <Plus size={12} strokeWidth={1.9} />
                    <span>添加</span>
                  </button>
                </form>
              </section>
            )}

            {detail.knowledgeUnit.coreClaim && detail.evidence.length > 0 && detail.questions.length === 0 && detail.aiJob?.status !== 'pending' && detail.aiJob?.status !== 'running' && (
              <section className="knowledge-detail-section knowledge-question-empty">
                <div className="knowledge-section-heading">
                  <span className="knowledge-detail-label">理解检验</span>
                  <span>还没有题目</span>
                </div>
                <div className="knowledge-question-empty-body">
                  <div>
                    <strong>根据这条知识生成理解题</strong>
                    <span>只使用已经保存的原文和 Evidence，不会重新提炼或覆盖核心知识。</span>
                  </div>
                  <button type="button" disabled={questionGenerating} onClick={() => void generateQuestions()}>
                    {questionGenerating ? <LoaderCircle size={13} strokeWidth={1.8} className="spin" /> : <Plus size={13} strokeWidth={1.9} />}
                    <span>{questionGenerating ? '正在出题' : '生成题目'}</span>
                  </button>
                </div>
                {questionError && <div className="quiz-error">{questionError}</div>}
              </section>
            )}

            {detail.questions.length > 0 && (
              <KnowledgeQuiz
                knowledgeUnitId={detail.knowledgeUnit.id}
                questions={detail.questions}
                evidence={detail.evidence}
                initialQuestionId={preferredQuestionId}
                currentMasteryScore={detail.reviewState?.masteryScore ?? null}
                preferContinueReview={Boolean(onReviewContinue)}
                canOpenSource={detail.source.platform === 'zhihu' && Boolean(detail.source.url)}
                onLocateEvidence={locateEvidence}
                onOpenSource={detail.source.url ? () => void openSourceUrl(detail.source.url!).catch((error) => console.error('Failed to open source URL', error)) : undefined}
                onJudged={() => {
                  void refresh()
                  if (selectedId) void refreshDetail(selectedId)
                }}
                onContinueReview={() => {
                  if (onReviewContinue) {
                    onReviewContinue()
                    return
                  }
                  const next = reviewQueue.find((item) => item.knowledgeUnitId !== selectedId)
                  if (next) {
                    openDetail(next.knowledgeUnitId, next.questionId)
                  } else {
                    closeDetail()
                    void refresh()
                  }
                }}
              />
            )}

            {management && (
              <section className="knowledge-detail-actions">
                <button type="button" disabled={organizing} onClick={() => void toggleArchive()}>
                  <Archive size={13} strokeWidth={1.8} />
                  <span>{management.archivedAt === null ? '归档' : '取消归档'}</span>
                </button>
                {!trashConfirming ? (
                  <button type="button" className="danger" disabled={organizing} onClick={() => setTrashConfirming(true)}>
                    <Trash2 size={13} strokeWidth={1.8} />
                    <span>移到回收站</span>
                  </button>
                ) : (
                  <div className="knowledge-trash-confirm">
                    <span>只删除这条知识，原始 Source 会保留，可从回收站恢复。</span>
                    <button type="button" onClick={() => setTrashConfirming(false)}>取消</button>
                    <button type="button" className="danger" disabled={organizing} onClick={() => void confirmTrash()}>确认移入</button>
                  </div>
                )}
              </section>
            )}

            <details
              ref={sourceRef}
              className={`knowledge-source-card${focusedEvidenceId ? ' evidence-focused' : ''}`}
              open={sourceOpen}
              onToggle={(event) => setSourceOpen(event.currentTarget.open)}>
              <summary>{detail.source.platform === 'zhihu' ? '回看知乎原文' : '回看原文'}</summary>
              {(detail.source.title || detail.source.author || detail.source.url) && (
                <div className="knowledge-source-meta">
                  {detail.source.title && <div><span>问题</span><strong>{detail.source.title}</strong></div>}
                  {detail.source.author && <div><span>作者</span><strong>{detail.source.author}</strong></div>}
                  {detail.source.url && (
                    <button type="button" onClick={() => void openSourceUrl(detail.source.url!).catch((error) => console.error('Failed to open source URL', error))}>
                      <ExternalLink size={12} strokeWidth={1.8} />
                      <span>打开原始链接</span>
                    </button>
                  )}
                </div>
              )}
              {detail.sourceUnits.length > 0 && (
                <div className="knowledge-source-units">
                  <div className="knowledge-source-units-head">
                    <strong>此来源已内化 {detail.sourceUnits.length} 条知识</strong>
                    <span>每条知识独立检验和复习</span>
                  </div>
                  <div className="knowledge-source-unit-list">
                    {detail.sourceUnits.map((unit) => {
                      const current = unit.id === detail.knowledgeUnit.id
                      return (
                        <button
                          type="button"
                          className={current ? 'current' : undefined}
                          key={unit.id}
                          disabled={current}
                          onClick={() => openDetail(unit.id)}>
                          <span>
                            <strong>{unit.coreClaim.trim() || '待提炼知识'}</strong>
                            <small>
                              {current ? '当前 · ' : ''}{STATUS_LABELS[unit.status]}
                              {unit.reviewCount > 0 ? ` · 掌握 ${unit.masteryScore}` : ''}
                              {unit.archivedAt !== null ? ' · 已归档' : ''}
                            </small>
                          </span>
                          {!current && <ChevronLeft size={12} strokeWidth={1.8} className="source-unit-open-icon" />}
                        </button>
                      )
                    })}
                  </div>
                </div>
              )}
              <div className="knowledge-source-context">
                {detail.source.contextBefore && <p className="context">{detail.source.contextBefore}</p>}
                {(() => {
                  const focused = detail.evidence.find((item) => item.id === focusedEvidenceId) ?? null
                  const parts = sourceWithHighlightedEvidence(detail.source.selectedText, focused)
                  return (
                    <blockquote>
                      {parts.before}
                      {parts.highlight && <mark>{parts.highlight}</mark>}
                      {parts.after}
                    </blockquote>
                  )
                })()}
                {detail.source.contextAfter && <p className="context">{detail.source.contextAfter}</p>}
              </div>
            </details>
          </div>
        )}
      </section>
    )
  }

  const nextReview = reviewQueue[0] ?? null

  return (
    <section className="knowledge-recent knowledge-home" aria-label="知识首页">
      <section className="knowledge-search-entry" aria-label="知乎搜索入口">
        <div>
          <h1>在知乎里找真实经验</h1>
          <p>从一个问题开始，找到值得内化的真人回答。</p>
        </div>
        <form onSubmit={submitHomeSearch}>
          <Search size={18} strokeWidth={1.8} />
          <input value={homeSearch} maxLength={100} placeholder="搜问题、经验、方法…" onChange={(event) => setHomeSearch(event.target.value)} />
          <button type="submit" disabled={homeSearch.trim().length < 2}>搜索</button>
        </form>
      </section>

      {!loading && !loadFailed && (
        <section className="knowledge-next" aria-label="继续学习">
          <div className="knowledge-home-section-heading">
            <div><strong>继续学习</strong><span>{nextReview ? `${reviewQueue.length} 条已到期` : '目前没有到期复习'}</span></div>
          </div>
          {nextReview ? (
            <button type="button" className="knowledge-next-card" onClick={() => openDetail(nextReview.knowledgeUnitId, nextReview.questionId)}>
              <span>
                <small>下一条</small>
                <strong>{reviewTitle(nextReview)}</strong>
                <em>{reviewSource(nextReview)} · {STATUS_LABELS[nextReview.status]}{nextReview.wrongCount > 0 ? ' · 曾答错' : ''}</em>
              </span>
              <b><Play size={14} strokeWidth={1.9} />继续</b>
            </button>
          ) : (
            <div className="knowledge-next-empty">继续阅读或搜索新的知乎经验，需要复习的知识会在到期后出现在这里。</div>
          )}
        </section>
      )}

      <section className="knowledge-recent-section">
        <div className="knowledge-home-section-heading">
          <div><strong>最近内化</strong><span>{items.length ? `${items.length} 条` : '暂无'}</span></div>
          <button type="button" className="knowledge-organize-entry" onClick={() => setLibrarianOpen(true)}>
            <FolderTree size={14} strokeWidth={1.8} />
            <span>整理知识库</span>
          </button>
        </div>

        {loading ? (
          <div className="knowledge-home-empty">正在读取本地知识库…</div>
        ) : loadFailed ? (
          <button type="button" className="knowledge-home-retry" onClick={() => void refresh()}>读取失败，重试</button>
        ) : items.length === 0 ? (
          <div className="knowledge-home-empty">在知乎搜索或外部阅读时选中有价值的内容，点「内化」后会出现在这里。</div>
        ) : (
          <div className="knowledge-home-list">
            {items.map((item) => (
              <button className="knowledge-home-item" type="button" key={item.id} onClick={() => openDetail(item.id)}>
                <span className="knowledge-home-item-main">
                  <strong title={itemTitle(item)}>{itemTitle(item)}</strong>
                  <span>{itemSource(item)}</span>
                </span>
                <span className={`knowledge-home-item-meta${item.aiJobStatus === 'failed' ? ' failed' : ''}`}>
                  <span>{itemStatus(item)}</span>
                  {item.reviewCount > 0 && <span>{item.masteryScore}%</span>}
                </span>
              </button>
            ))}
          </div>
        )}
      </section>
    </section>
  )
}
