import { emit } from '@tauri-apps/api/event'
import { ArrowLeft, BookmarkPlus, ExternalLink, LoaderCircle, Search } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { captureKnowledge, internalizeKnowledge, openSourceUrl } from '../../services/knowledge-service'
import { searchZhihu, searchZhihuExpanded } from '../../services/zhihu-service'
import type { ZhihuSearchItem, ZhihuSearchResult } from '../../types'

interface ZhihuSearchViewProps {
  initialQuery: string
  onBack: () => void
  onCaptured: () => void
  onOpenSettings: () => void
}

type SearchSort = 'relevance' | 'votes' | 'comments' | 'latest'
type CaptureTarget = 'selection' | 'full'

const SEARCH_SORTS: Array<{ id: SearchSort; label: string }> = [
  { id: 'relevance', label: '综合' },
  { id: 'votes', label: '赞同' },
  { id: 'comments', label: '讨论' },
  { id: 'latest', label: '最新' }
]

function normalizedSeed(value: string) {
  return value.replace(/\s+/g, ' ').trim()
}

function queryFromSelection(value: string) {
  const normalized = normalizedSeed(value)
  if (normalized.length <= 100) return normalized
  const sentence = normalized.split(/[。！？!?；;\n]/).map((item) => item.trim()).find((item) => item.length >= 2)
  return (sentence || normalized).slice(0, 100).trim()
}

function resultExcerpt(item: ZhihuSearchItem) {
  const text = item.contentText.replace(/\s+/g, ' ').trim()
  if (!text) return '打开回答查看原文。'
  return text.length > 190 ? `${text.slice(0, 190)}…` : text
}

function selectionContext(content: string, selectedText: string) {
  const start = content.indexOf(selectedText)
  if (start < 0) return { contextBefore: undefined, contextAfter: undefined }
  const before = content.slice(Math.max(0, start - 700), start).trim()
  const after = content.slice(start + selectedText.length, start + selectedText.length + 700).trim()
  return {
    contextBefore: before || undefined,
    contextAfter: after || undefined
  }
}

function metric(value: number) {
  if (value >= 10_000) return `${(value / 10_000).toFixed(value >= 100_000 ? 0 : 1)}万`
  return String(value)
}

function mergeSearchItems(primary: ZhihuSearchItem[], extra: ZhihuSearchItem[]) {
  const seen = new Set<string>()
  return [...primary, ...extra].filter((item) => {
    const key = item.url.trim()
      ? `url:${item.url.trim()}`
      : item.contentId.trim()
        ? `${item.contentType.trim()}:${item.contentId.trim()}`
        : `${item.title.trim()}:${item.authorName.trim()}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

export function ZhihuSearchView({ initialQuery, onBack, onCaptured, onOpenSettings }: ZhihuSearchViewProps) {
  const initialNormalized = useMemo(() => normalizedSeed(initialQuery), [initialQuery])
  const initialSearchQuery = useMemo(() => queryFromSelection(initialQuery), [initialQuery])
  const [query, setQuery] = useState(initialSearchQuery)
  const [result, setResult] = useState<ZhihuSearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [expanding, setExpanding] = useState(false)
  const [expanded, setExpanded] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [expandError, setExpandError] = useState<string | null>(null)
  const [reader, setReader] = useState<ZhihuSearchItem | null>(null)
  const [selectedText, setSelectedText] = useState('')
  const [capturing, setCapturing] = useState<CaptureTarget | null>(null)
  const [captureMessage, setCaptureMessage] = useState<string | null>(null)
  const [sort, setSort] = useState<SearchSort>('relevance')
  const readerRef = useRef<HTMLDivElement | null>(null)

  const runSearch = useCallback(async (nextQuery?: string) => {
    const value = (nextQuery ?? query).replace(/\s+/g, ' ').trim()
    if (value.length < 2) {
      setError('至少输入 2 个字符。')
      return
    }
    if (value.length > 100) {
      setError('搜索内容不能超过 100 个字符，请先精简问题。')
      return
    }
    setQuery(value)
    setLoading(true)
    setExpanded(false)
    setExpandError(null)
    setError(null)
    setReader(null)
    setSelectedText('')
    try {
      setResult(await searchZhihu(value, 10))
    } catch (searchError) {
      setResult(null)
      setError(String(searchError))
    } finally {
      setLoading(false)
    }
  }, [query])

  const expandSearch = useCallback(async () => {
    const value = query.replace(/\s+/g, ' ').trim()
    if (!result || value.length < 2 || expanding || expanded) return
    setExpanding(true)
    setExpandError(null)
    try {
      const extra = await searchZhihuExpanded(value, 20)
      setResult((current) => current ? {
        ...current,
        hasMore: extra.hasMore,
        items: mergeSearchItems(current.items, extra.items)
      } : extra)
      setExpanded(true)
    } catch (searchError) {
      setExpandError(String(searchError))
    } finally {
      setExpanding(false)
    }
  }, [expanded, expanding, query, result])

  useEffect(() => {
    const nextQuery = queryFromSelection(initialQuery)
    setQuery(nextQuery)
    setReader(null)
    setSelectedText('')
    setCaptureMessage(null)
    setError(null)
    setSort('relevance')
    if (initialNormalized.length >= 2 && initialNormalized.length <= 100) {
      void runSearch(initialNormalized)
    } else {
      setResult(null)
    }
    // Intentionally react only to a new external search seed. Typing in the local input must not retrigger this effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialNormalized, initialQuery])

  const sortedItems = useMemo(() => {
    if (!result) return []
    if (sort === 'relevance') return result.items
    return result.items
      .map((item, index) => ({ item, index }))
      .sort((left, right) => {
        const difference = sort === 'votes'
          ? right.item.voteUpCount - left.item.voteUpCount
          : sort === 'comments'
            ? right.item.commentCount - left.item.commentCount
            : right.item.editTime - left.item.editTime
        return difference || left.index - right.index
      })
      .map(({ item }) => item)
  }, [result, sort])

  const submit = (event: FormEvent) => {
    event.preventDefault()
    void runSearch()
  }

  const openReader = (item: ZhihuSearchItem) => {
    setReader(item)
    setSelectedText('')
    setCaptureMessage(null)
  }

  const captureReaderText = async (text: string, target: CaptureTarget) => {
    const normalizedText = text.trim()
    if (!reader || !normalizedText || capturing) return
    setCapturing(target)
    setCaptureMessage(null)
    try {
      const context = target === 'selection'
        ? selectionContext(reader.contentText, normalizedText)
        : { contextBefore: undefined, contextAfter: undefined }
      const captured = await captureKnowledge({
        platform: 'zhihu',
        selectedText: normalizedText,
        url: reader.url || undefined,
        title: reader.title || undefined,
        author: reader.authorName || undefined,
        ...context
      })
      try {
        await internalizeKnowledge(captured.knowledgeUnit.id)
      } catch (jobError) {
        console.error('Source was saved but AI internalization could not be scheduled', jobError)
      }
      await emit('knowledge://changed', { knowledgeUnitId: captured.knowledgeUnit.id })
      setCaptureMessage(captured.duplicate
        ? target === 'full' ? '这篇回答已经在知识流程中' : '这段内容已经在知识流程中'
        : target === 'full' ? '已加入收件箱 · AI 会按内容量提炼知识草稿' : '已加入收件箱 · AI 正在提炼知识草稿')
      setSelectedText('')
      window.getSelection()?.removeAllRanges()
      onCaptured()
    } catch (captureError) {
      setCaptureMessage(`内化失败：${String(captureError)}`)
    } finally {
      setCapturing(null)
    }
  }

  const captureSelection = () => {
    if (selectedText) void captureReaderText(selectedText, 'selection')
  }

  const captureFullAnswer = () => {
    if (reader?.contentText.trim()) void captureReaderText(reader.contentText, 'full')
  }

  const handleReaderSelection = () => {
    if (!readerRef.current) return
    const selection = window.getSelection()
    const anchor = selection?.anchorNode
    const focus = selection?.focusNode
    if (!selection || !anchor || !focus || !readerRef.current.contains(anchor) || !readerRef.current.contains(focus)) {
      setSelectedText('')
      return
    }
    const value = selection.toString().trim()
    setSelectedText(value.length >= 2 ? value : '')
    if (value) setCaptureMessage(null)
  }

  if (reader) {
    return (
      <section className="zhihu-reader" aria-label="知乎回答阅读">
        <header className="zhihu-view-heading">
          <button type="button" onClick={() => setReader(null)}><ArrowLeft size={16} strokeWidth={1.8} />返回搜索结果</button>
          <div className="zhihu-reader-actions">
            <button
              type="button"
              className="zhihu-reader-internalize"
              disabled={!reader.contentText.trim() || Boolean(capturing)}
              title="把当前回答全文作为 Source，由 AI 按内容量提炼多条独立知识"
              onClick={captureFullAnswer}>
              {capturing === 'full' ? <LoaderCircle size={14} strokeWidth={1.8} className="spin" /> : <BookmarkPlus size={14} strokeWidth={1.8} />}
              {capturing === 'full' ? '正在加入' : '加入收件箱'}
            </button>
            {reader.url && <button type="button" onClick={() => void openSourceUrl(reader.url)}><ExternalLink size={14} strokeWidth={1.8} />打开知乎</button>}
          </div>
        </header>
        <div className="zhihu-reader-scroll">
          <article>
            <div className="zhihu-reader-meta">
              <span>知乎回答</span>
              {reader.authorName && <strong>{reader.authorName}</strong>}
              {reader.voteUpCount > 0 && <small>{metric(reader.voteUpCount)} 赞同</small>}
            </div>
            <h2>{reader.title || '知乎回答'}</h2>
            <div
              ref={readerRef}
              className="zhihu-reader-content"
              tabIndex={0}
              onMouseUp={handleReaderSelection}
              onKeyUp={handleReaderSelection}>
              {reader.contentText || '该搜索结果未返回正文，请打开知乎阅读原文。'}
            </div>
          </article>
        </div>
        {(selectedText || captureMessage) && (
          <div className="zhihu-reader-selection-bar">
            <div>
              {captureMessage ? <strong>{captureMessage}</strong> : <><span>已选择</span><strong>{selectedText.length > 68 ? `${selectedText.slice(0, 68)}…` : selectedText}</strong></>}
            </div>
            {selectedText && (
              <button type="button" disabled={Boolean(capturing)} onClick={captureSelection}>
                {capturing === 'selection' ? <LoaderCircle size={14} strokeWidth={1.8} className="spin" /> : <BookmarkPlus size={14} strokeWidth={1.8} />}
                {capturing === 'selection' ? '正在内化' : '内化选中内容'}
              </button>
            )}
          </div>
        )}
      </section>
    )
  }

  return (
    <section className="zhihu-search-view" aria-label="知乎搜索">
      <header className="zhihu-view-heading">
        <button type="button" onClick={onBack}><ArrowLeft size={16} strokeWidth={1.8} />返回</button>
        <span>知乎搜索</span>
      </header>

      <form className="zhihu-search-form" onSubmit={submit}>
        <Search size={18} strokeWidth={1.8} />
        <input autoFocus value={query} maxLength={100} placeholder="搜问题、经验、方法…" onChange={(event) => setQuery(event.target.value)} />
        <button type="submit" disabled={loading || query.trim().length < 2}>{loading ? <LoaderCircle size={15} strokeWidth={1.8} className="spin" /> : '搜索'}</button>
      </form>

      {initialNormalized.length > 100 && (
        <div className="zhihu-search-seed-note">选中文字较长，已提取前一句作为搜索词。确认或修改后再搜索。</div>
      )}

      {error && (
        <div className="zhihu-search-error">
          <span>{error}</span>
          {error.includes('Access Secret') && <button type="button" onClick={onOpenSettings}>配置知乎 Access Secret</button>}
        </div>
      )}
      {expandError && <div className="zhihu-search-error"><span>扩展搜索失败：{expandError}</span></div>}

      {result && result.items.length > 0 && (
        <div className="zhihu-search-tools">
          <span>{result.items.length} 条结果{expanded ? ' · 已合并扩展搜索' : ' · 站内搜索单次最多 10 条'}</span>
          <div role="group" aria-label="搜索结果排序">
            {SEARCH_SORTS.map((item) => (
              <button
                type="button"
                key={item.id}
                className={sort === item.id ? 'active' : undefined}
                onClick={() => setSort(item.id)}>
                {item.label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="zhihu-search-expand"
            disabled={expanding || expanded}
            title="使用知乎开放平台全网搜索并限定 zhihu.com，补充当前站内搜索结果"
            onClick={() => void expandSearch()}>
            {expanding && <LoaderCircle size={13} strokeWidth={1.8} className="spin" />}
            <span>{expanding ? '扩展中' : expanded ? '已扩展' : '扩展搜索'}</span>
          </button>
        </div>
      )}

      <div className="zhihu-search-results">
        {!loading && result && result.items.length === 0 && (
          <div className="zhihu-search-empty">{result.emptyReason || '没有找到合适的知乎内容，换个问法试试。'}</div>
        )}
        {sortedItems.map((item) => (
          <article key={`${item.contentType}:${item.contentId}:${item.url}`} className="zhihu-result-item">
            <button type="button" className="zhihu-result-main" onClick={() => openReader(item)}>
              <div className="zhihu-result-meta">
                <span>{item.contentType === 'Article' ? '文章' : '回答'}</span>
                {item.authorName && <strong>{item.authorName}</strong>}
                {item.voteUpCount > 0 && <small>{metric(item.voteUpCount)} 赞同</small>}
                {item.commentCount > 0 && <small>{metric(item.commentCount)} 评论</small>}
              </div>
              <h3>{item.title || '知乎内容'}</h3>
              <p>{resultExcerpt(item)}</p>
            </button>
            <div className="zhihu-result-actions">
              <button type="button" onClick={() => openReader(item)}>阅读</button>
              {item.url && <button type="button" onClick={() => void openSourceUrl(item.url)}><ExternalLink size={13} strokeWidth={1.8} />知乎</button>}
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}
