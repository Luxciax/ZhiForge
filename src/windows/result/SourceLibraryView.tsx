import { ArrowLeft, ExternalLink, Search } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useState } from 'react'
import { getSourceDetail, listSources, openSourceUrl } from '../../services/knowledge-service'
import type { KnowledgeStatus, SourceDetailRecord, SourceLibraryItem, SourcePlatform } from '../../types'

interface SourceLibraryViewProps {
  initialSourceId?: string | null
  onSourceChange: (sourceId: string | null) => void
  onOpenKnowledge: (knowledgeUnitId: string) => void
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

const PLATFORM_OPTIONS: Array<{ value: '' | SourcePlatform; label: string }> = [
  { value: '', label: '全部来源' },
  { value: 'zhihu', label: '知乎' },
  { value: 'web', label: '网页' },
  { value: 'pdf', label: 'PDF' },
  { value: 'desktop', label: '桌面' },
  { value: 'unknown', label: '未知来源' }
]

function sourceTitle(item: SourceLibraryItem) {
  return item.title?.trim() || item.author?.trim() || `${PLATFORM_LABELS[item.platform]}来源`
}

function sourceExcerpt(value: string) {
  const text = value.replace(/\s+/g, ' ').trim()
  return text.length > 180 ? `${text.slice(0, 180)}…` : text
}

export function SourceLibraryView({ initialSourceId = null, onSourceChange, onOpenKnowledge }: SourceLibraryViewProps) {
  const [items, setItems] = useState<SourceLibraryItem[]>([])
  const [detail, setDetail] = useState<SourceDetailRecord | null>(null)
  const [queryDraft, setQueryDraft] = useState('')
  const [query, setQuery] = useState('')
  const [platform, setPlatform] = useState<'' | SourcePlatform>('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refreshList = useCallback(async () => {
    setLoading(true)
    try {
      setItems(await listSources(query || undefined, platform || undefined, 200))
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }, [platform, query])

  const loadDetail = useCallback(async (sourceId: string) => {
    setLoading(true)
    try {
      setDetail(await getSourceDetail(sourceId))
      setError(null)
    } catch (loadError) {
      setDetail(null)
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    if (initialSourceId) void loadDetail(initialSourceId)
    else {
      setDetail(null)
      void refreshList()
    }
  }, [initialSourceId, loadDetail, refreshList])

  const submitSearch = (event: FormEvent) => {
    event.preventDefault()
    setQuery(queryDraft.trim())
  }

  const openSource = (sourceId: string) => {
    onSourceChange(sourceId)
  }

  const closeSource = () => {
    setDetail(null)
    onSourceChange(null)
  }

  if (initialSourceId) {
    return (
      <section className="sources-view source-detail-view" aria-label="来源详情">
        <header className="source-detail-toolbar">
          <button type="button" onClick={closeSource}><ArrowLeft size={15} strokeWidth={1.8} />来源</button>
          {detail?.source.url && (
            <button type="button" onClick={() => void openSourceUrl(detail.source.url!)}>
              <ExternalLink size={13} strokeWidth={1.8} />打开原始链接
            </button>
          )}
        </header>
        {error ? (
          <button type="button" className="workspace-error" onClick={() => void loadDetail(initialSourceId)}>读取失败，点击重试</button>
        ) : loading || !detail ? (
          <div className="workspace-empty">正在读取来源…</div>
        ) : (
          <div className="source-detail-content">
            <header className="source-detail-heading">
              <span>{PLATFORM_LABELS[detail.source.platform]}</span>
              <h1>{detail.source.title || detail.source.author || '未命名来源'}</h1>
              <p>
                {detail.source.author ? `${detail.source.author} · ` : ''}
                {new Date(detail.source.capturedAt).toLocaleDateString()} · 已内化 {detail.knowledgeUnits.length} 条知识
              </p>
            </header>

            <section className="source-original-section">
              <div className="source-section-heading"><strong>原文</strong><span>Source</span></div>
              {detail.source.contextBefore && <p className="source-context">{detail.source.contextBefore}</p>}
              <blockquote>{detail.source.selectedText}</blockquote>
              {detail.source.contextAfter && <p className="source-context">{detail.source.contextAfter}</p>}
            </section>

            <section className="source-derived-section">
              <div className="source-section-heading"><strong>内化知识</strong><span>{detail.knowledgeUnits.length} 条</span></div>
              <div className="source-derived-list">
                {detail.knowledgeUnits.map((unit, index) => (
                  <button type="button" key={unit.id} onClick={() => onOpenKnowledge(unit.id)}>
                    <span className="source-derived-order">{String(index + 1).padStart(2, '0')}</span>
                    <span className="source-derived-copy">
                      <strong>{unit.coreClaim.trim() || '待提炼知识'}</strong>
                      <small>
                        {STATUS_LABELS[unit.status]}
                        {unit.reviewCount > 0 ? ` · 掌握 ${unit.masteryScore}` : ' · 待检验'}
                        {unit.archivedAt !== null ? ' · 已归档' : ''}
                      </small>
                    </span>
                    <b>查看</b>
                  </button>
                ))}
              </div>
            </section>
          </div>
        )}
      </section>
    )
  }

  return (
    <section className="sources-view" aria-label="来源">
      <header className="workspace-page-heading">
        <div>
          <h1>来源</h1>
          <p>{loading ? '正在读取…' : `${items.length} 个原始来源`}</p>
        </div>
      </header>

      <form className="library-search" onSubmit={submitSearch}>
        <Search size={17} strokeWidth={1.8} />
        <input
          value={queryDraft}
          placeholder="搜索标题、作者、链接或原文…"
          onChange={(event) => setQueryDraft(event.target.value)}
        />
        {queryDraft && <button type="submit">搜索</button>}
      </form>

      <div className="source-filter-row">
        <label>
          <span>平台</span>
          <select value={platform} onChange={(event) => setPlatform(event.target.value as '' | SourcePlatform)}>
            {PLATFORM_OPTIONS.map((option) => <option key={option.value || 'all'} value={option.value}>{option.label}</option>)}
          </select>
        </label>
      </div>

      {error ? (
        <button type="button" className="workspace-error" onClick={() => void refreshList()}>读取失败，点击重试</button>
      ) : loading ? (
        <div className="workspace-empty">正在读取来源…</div>
      ) : items.length === 0 ? (
        <div className="workspace-empty">没有符合当前条件的来源。</div>
      ) : (
        <div className="source-list">
          {items.map((item) => (
            <button type="button" className="source-item" key={item.id} onClick={() => openSource(item.id)}>
              <div className="source-item-main">
                <div className="source-item-meta">
                  <span>{PLATFORM_LABELS[item.platform]}</span>
                  {item.author && <strong>{item.author}</strong>}
                  <small>{new Date(item.capturedAt).toLocaleDateString()}</small>
                </div>
                <h3>{sourceTitle(item)}</h3>
                <p>{sourceExcerpt(item.selectedText)}</p>
              </div>
              <div className="source-item-count"><b>{item.knowledgeCount}</b><span>条知识</span></div>
            </button>
          ))}
        </div>
      )}
    </section>
  )
}
