import { Archive, LoaderCircle, RotateCcw, Search, Sparkles, Trash2, X } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useState } from 'react'
import {
  listKnowledgeLibrary,
  listKnowledgeTrash,
  listTags,
  listTopics,
  restoreKnowledge,
  semanticSearchKnowledge,
  type SemanticSearchHit,
  type SemanticSearchResult
} from '../../services/knowledge-service'
import type { KnowledgeLibraryItem, KnowledgeQualityStatus, KnowledgeStatus, SourcePlatform, TagRecord, TopicRecord, TrashKnowledgeItem } from '../../types'

interface KnowledgeLibraryViewProps {
  initialTopicId?: string | null
  onOpenKnowledge: (knowledgeUnitId: string) => void
}

const STATUS_OPTIONS: Array<{ value: '' | KnowledgeStatus; label: string }> = [
  { value: '', label: '全部' },
  { value: 'captured', label: '待提炼' },
  { value: 'weak', label: '薄弱' },
  { value: 'learning', label: '学习中' },
  { value: 'reviewing', label: '复习中' },
  { value: 'mastered', label: '已掌握' }
]

const QUALITY_OPTIONS: Array<{ value: '' | KnowledgeQualityStatus; label: string }> = [
  { value: '', label: '全部质量' },
  { value: 'unverified', label: '未验证' },
  { value: 'needs_expansion', label: '待扩展' },
  { value: 'conflicted', label: '有冲突' },
  { value: 'stale', label: '可能过时' },
  { value: 'verified', label: '已验证' }
]

const QUALITY_LABELS: Record<KnowledgeQualityStatus, string> = {
  unverified: '未验证',
  verified: '已验证',
  conflicted: '有冲突',
  stale: '可能过时',
  needs_expansion: '待扩展'
}

const PLATFORM_LABELS: Record<SourcePlatform, string> = {
  zhihu: '知乎',
  web: '网页',
  pdf: 'PDF',
  desktop: '桌面',
  unknown: '未知来源'
}

const STATUS_LABELS: Record<KnowledgeStatus, string> = {
  captured: '待提炼',
  processed: '已处理',
  weak: '薄弱',
  learning: '学习中',
  reviewing: '复习中',
  mastered: '已掌握'
}

function itemTitle(item: KnowledgeLibraryItem) {
  return item.coreClaim.trim() || item.selectedText.trim()
}

function itemSource(item: KnowledgeLibraryItem) {
  return item.author || item.title || PLATFORM_LABELS[item.platform]
}

function semanticTitle(item: SemanticSearchHit) {
  return item.coreClaim.trim() || item.selectedText.trim()
}

function semanticSource(item: SemanticSearchHit) {
  return item.author || item.title || PLATFORM_LABELS[item.platform]
}

export function KnowledgeLibraryView({ initialTopicId = null, onOpenKnowledge }: KnowledgeLibraryViewProps) {
  const [items, setItems] = useState<KnowledgeLibraryItem[]>([])
  const [topics, setTopics] = useState<TopicRecord[]>([])
  const [tags, setTags] = useState<TagRecord[]>([])
  const [trashItems, setTrashItems] = useState<TrashKnowledgeItem[]>([])
  const [showTrash, setShowTrash] = useState(false)
  const [restoringId, setRestoringId] = useState<string | null>(null)
  const [queryDraft, setQueryDraft] = useState('')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState<'' | KnowledgeStatus>('')
  const [qualityStatus, setQualityStatus] = useState<'' | KnowledgeQualityStatus>('')
  const [topicId, setTopicId] = useState(initialTopicId ?? '')
  const [tagId, setTagId] = useState('')
  const [includeArchived, setIncludeArchived] = useState(false)
  const [semanticResult, setSemanticResult] = useState<SemanticSearchResult | null>(null)
  const [semanticSearching, setSemanticSearching] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const [knowledge, topicList, tagList, trash] = await Promise.all([
        listKnowledgeLibrary({
          query: query || undefined,
          status: status || undefined,
          qualityStatus: qualityStatus || undefined,
          topicId: topicId || undefined,
          tagId: tagId || undefined,
          includeArchived,
          limit: 200
        }),
        listTopics(),
        listTags(),
        listKnowledgeTrash()
      ])
      setItems(knowledge)
      setTopics(topicList)
      setTags(tagList)
      setTrashItems(trash)
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }, [includeArchived, qualityStatus, query, status, tagId, topicId])

  useEffect(() => {
    setTopicId(initialTopicId ?? '')
  }, [initialTopicId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const submitSearch = (event: FormEvent) => {
    event.preventDefault()
    setSemanticResult(null)
    setQuery(queryDraft.trim())
  }

  const runSemanticSearch = async () => {
    const value = queryDraft.trim()
    if (!value || semanticSearching) return
    setSemanticSearching(true)
    setError(null)
    try {
      const result = await semanticSearchKnowledge(value, {
        status: status || undefined,
        qualityStatus: qualityStatus || undefined,
        topicId: topicId || undefined,
        tagId: tagId || undefined,
        includeArchived,
        limit: 20
      })
      setSemanticResult(result)
    } catch (searchError) {
      setSemanticResult(null)
      setError(String(searchError))
    } finally {
      setSemanticSearching(false)
    }
  }

  const clearSemanticForFilter = () => {
    if (semanticResult) {
      setSemanticResult(null)
      setQuery(queryDraft.trim())
    }
  }

  const restore = async (knowledgeUnitId: string) => {
    if (restoringId) return
    setRestoringId(knowledgeUnitId)
    try {
      await restoreKnowledge(knowledgeUnitId)
      await refresh()
    } catch (restoreError) {
      setError(String(restoreError))
    } finally {
      setRestoringId(null)
    }
  }

  return (
    <section className="library-view" aria-label="知识库">
      <header className="workspace-page-heading">
        <div>
          <h1>{showTrash ? '回收站' : '知识库'}</h1>
          <p>
            {loading
              ? '正在读取…'
              : showTrash
                ? `${trashItems.length} 条可恢复知识`
                : semanticResult
                  ? `${semanticResult.hits.length} 条智能结果`
                  : `${items.length} 条当前结果`}
          </p>
        </div>
        <button type="button" className={`library-trash-toggle${showTrash ? ' active' : ''}`} onClick={() => setShowTrash((value) => !value)}>
          {showTrash ? <RotateCcw size={14} strokeWidth={1.8} /> : <Trash2 size={14} strokeWidth={1.8} />}
          <span>{showTrash ? '返回知识库' : `回收站${trashItems.length ? ` ${trashItems.length}` : ''}`}</span>
        </button>
      </header>

      {!showTrash && <form className="library-search" onSubmit={submitSearch}>
        <Search size={17} strokeWidth={1.8} />
        <input
          value={queryDraft}
          placeholder="搜索知识、原文、作者或笔记…"
          onChange={(event) => setQueryDraft(event.target.value)}
        />
        {queryDraft && <button type="submit" disabled={semanticSearching}>搜索</button>}
        {queryDraft && (
          <button type="button" className="library-smart-search" disabled={semanticSearching} onClick={() => void runSemanticSearch()}>
            {semanticSearching ? <LoaderCircle size={13} className="spin" /> : <Sparkles size={13} strokeWidth={1.8} />}
            <span>{semanticSearching ? '正在理解' : '智能搜索'}</span>
          </button>
        )}
      </form>}

      {!showTrash && <div className="library-filters">
        <div className="library-status-tabs">
          {STATUS_OPTIONS.map((option) => (
            <button
              type="button"
              key={option.value || 'all'}
              className={status === option.value ? 'active' : undefined}
              onClick={() => {
                clearSemanticForFilter()
                setStatus(option.value)
              }}>
              {option.label}
            </button>
          ))}
        </div>
        <div className="library-filter-row">
          <label>
            <span>知识质量</span>
            <select value={qualityStatus} onChange={(event) => {
              clearSemanticForFilter()
              setQualityStatus(event.target.value as '' | KnowledgeQualityStatus)
            }}>
              {QUALITY_OPTIONS.map((option) => <option key={option.value || 'all-quality'} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label>
            <span>主题</span>
            <select value={topicId} onChange={(event) => {
              clearSemanticForFilter()
              setTopicId(event.target.value)
            }}>
              <option value="">全部主题</option>
              {topics.map((topic) => <option key={topic.id} value={topic.id}>{topic.name}</option>)}
            </select>
          </label>
          <label>
            <span>标签</span>
            <select value={tagId} onChange={(event) => {
              clearSemanticForFilter()
              setTagId(event.target.value)
            }}>
              <option value="">全部标签</option>
              {tags.map((tag) => <option key={tag.id} value={tag.id}>#{tag.name}</option>)}
            </select>
          </label>
          <button
            type="button"
            className={includeArchived ? 'active' : undefined}
            onClick={() => {
              clearSemanticForFilter()
              setIncludeArchived((value) => !value)
            }}>
            <Archive size={14} strokeWidth={1.8} />
            <span>包含归档</span>
          </button>
        </div>
      </div>}

      {!showTrash && semanticResult && (
        <div className="semantic-search-summary">
          <div>
            <span><Sparkles size={13} strokeWidth={1.8} />智能搜索</span>
            <strong>“{semanticResult.query}”</strong>
            <small>
              从 {semanticResult.candidateCount} 个候选中重排
              {semanticResult.usedExpansion && semanticResult.expandedTerms.length > 0
                ? ` · 扩展：${semanticResult.expandedTerms.join(' / ')}`
                : ''}
            </small>
          </div>
          <button type="button" title="返回普通搜索结果" onClick={() => setSemanticResult(null)}><X size={13} strokeWidth={1.8} /></button>
        </div>
      )}

      {error ? (
        <button type="button" className="workspace-error" onClick={() => void refresh()}>读取失败，点击重试</button>
      ) : semanticSearching ? (
        <div className="workspace-empty">正在扩展检索并重排知识…</div>
      ) : loading ? (
        <div className="workspace-empty">正在读取知识库…</div>
      ) : showTrash ? (
        trashItems.length === 0 ? (
          <div className="workspace-empty">回收站是空的。</div>
        ) : (
          <div className="trash-list">
            {trashItems.map((item) => (
              <div className="trash-item" key={item.knowledgeUnitId}>
                <div>
                  <strong>{item.coreClaim.trim() || item.selectedText.trim()}</strong>
                  <small>{item.title || PLATFORM_LABELS[item.platform]} · 删除于 {new Date(item.deletedAt).toLocaleDateString()}</small>
                </div>
                <button type="button" disabled={restoringId !== null} onClick={() => void restore(item.knowledgeUnitId)}>
                  <RotateCcw size={13} strokeWidth={1.8} />
                  <span>{restoringId === item.knowledgeUnitId ? '恢复中' : '恢复'}</span>
                </button>
              </div>
            ))}
          </div>
        )
      ) : semanticResult ? (
        semanticResult.hits.length === 0 ? (
          <div className="workspace-empty">当前知识库里没有找到足够相关的内容。</div>
        ) : (
          <div className="semantic-search-list">
            {semanticResult.hits.map((item) => (
              <button type="button" className="semantic-search-item" key={item.knowledgeUnitId} onClick={() => onOpenKnowledge(item.knowledgeUnitId)}>
                <div className="semantic-search-main">
                  <div className="library-item-title-row">
                    <strong>{semanticTitle(item)}</strong>
                    <span className={`library-quality ${item.qualityStatus}`}>{QUALITY_LABELS[item.qualityStatus]}</span>
                  </div>
                  <p>{item.reason}</p>
                  <div className="library-item-chips">
                    {item.topicNames.slice(0, 3).map((topic) => <span className="topic-chip" key={`topic:${topic}`}>{topic}</span>)}
                    {item.tagNames.slice(0, 4).map((tag) => <span className="tag-chip" key={`tag:${tag}`}>#{tag}</span>)}
                  </div>
                  <small>{semanticSource(item)} · {item.reviewCount === 0 ? '待检验' : STATUS_LABELS[item.status]}</small>
                </div>
                <div className="semantic-search-score">
                  <b>{Math.round(item.score * 100)}</b>
                  <span>相关度</span>
                </div>
              </button>
            ))}
          </div>
        )
      ) : items.length === 0 ? (
        <div className="workspace-empty">没有符合当前条件的知识。</div>
      ) : (
        <div className="library-list">
          {items.map((item) => (
            <button type="button" className="library-item" key={item.id} onClick={() => onOpenKnowledge(item.id)}>
              <div className="library-item-main">
                <div className="library-item-title-row">
                  <strong>{itemTitle(item)}</strong>
                  <span className={`library-quality ${item.qualityStatus}`}>{QUALITY_LABELS[item.qualityStatus]}</span>
                  {item.archivedAt && <span className="library-archived">已归档</span>}
                </div>
                {item.coreClaim && item.selectedText && item.coreClaim.trim() !== item.selectedText.trim() && (
                  <p>{item.selectedText.length > 150 ? `${item.selectedText.slice(0, 150)}…` : item.selectedText}</p>
                )}
                <div className="library-item-chips">
                  {item.topics.slice(0, 3).map((topic) => <span className="topic-chip" key={topic.id}>{topic.name}</span>)}
                  {item.tags.slice(0, 4).map((tag) => <span className="tag-chip" key={tag.id}>#{tag.name}</span>)}
                </div>
                <small>{itemSource(item)} · {new Date(item.updatedAt).toLocaleDateString()}</small>
              </div>
              <div className="library-item-state">
                <span>{item.reviewCount === 0 ? '待检验' : STATUS_LABELS[item.status]}</span>
                {item.reviewCount > 0 && <b>{item.masteryScore}</b>}
              </div>
            </button>
          ))}
        </div>
      )}
    </section>
  )
}
