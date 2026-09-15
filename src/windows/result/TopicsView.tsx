import {
  Archive,
  ArchiveRestore,
  Check,
  LockKeyhole,
  Pencil,
  Plus,
  Tags,
  Trash2,
  X
} from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react'
import {
  createTopic,
  deleteEmptyTag,
  listTags,
  listTopics,
  setTopicArchived,
  updateTag,
  updateTopic
} from '../../services/knowledge-service'
import type { TagRecord, TopicRecord } from '../../types'

interface TopicsViewProps {
  onOpenTopic: (topicId: string) => void
}

export function TopicsView({ onOpenTopic }: TopicsViewProps) {
  const [topics, setTopics] = useState<TopicRecord[]>([])
  const [tags, setTags] = useState<TagRecord[]>([])
  const [name, setName] = useState('')
  const [creating, setCreating] = useState(false)
  const [loading, setLoading] = useState(true)
  const [showArchived, setShowArchived] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editName, setEditName] = useState('')
  const [editDescription, setEditDescription] = useState('')
  const [busyId, setBusyId] = useState<string | null>(null)
  const [editingTagId, setEditingTagId] = useState<string | null>(null)
  const [tagNameDraft, setTagNameDraft] = useState('')
  const [renamingTagId, setRenamingTagId] = useState<string | null>(null)
  const [confirmTagId, setConfirmTagId] = useState<string | null>(null)
  const [deletingTagId, setDeletingTagId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true)
    try {
      const [nextTopics, nextTags] = await Promise.all([listTopics(true), listTags()])
      setTopics(nextTopics)
      setTags(nextTags)
      setError(null)
    } catch (loadError) {
      setError(`读取主题与标签失败：${String(loadError)}`)
    } finally {
      if (!quiet) setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const activeTopics = useMemo(() => topics.filter((topic) => topic.archivedAt === null), [topics])
  const archivedTopics = useMemo(() => topics.filter((topic) => topic.archivedAt !== null), [topics])
  const visibleTopics = showArchived ? topics : activeTopics
  const emptyTagCount = tags.filter((tag) => tag.knowledgeCount === 0).length

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const value = name.trim()
    if (!value || creating) return
    setCreating(true)
    setError(null)
    try {
      await createTopic(value)
      setName('')
      await refresh(true)
    } catch (createError) {
      setError(`创建主题失败：${String(createError)}`)
    } finally {
      setCreating(false)
    }
  }

  const beginEdit = (topic: TopicRecord) => {
    if (topic.locked || topic.archivedAt !== null || busyId) return
    setEditingId(topic.id)
    setEditName(topic.name)
    setEditDescription(topic.description)
    setError(null)
  }

  const saveEdit = async (event: FormEvent, topic: TopicRecord) => {
    event.preventDefault()
    const nextName = editName.trim()
    if (!nextName || busyId) return
    setBusyId(topic.id)
    setError(null)
    try {
      await updateTopic(topic.id, nextName, editDescription)
      setEditingId(null)
      await refresh(true)
    } catch (updateError) {
      setError(`保存主题失败：${String(updateError)}`)
    } finally {
      setBusyId(null)
    }
  }

  const changeArchiveState = async (topic: TopicRecord, archived: boolean) => {
    if (topic.locked || busyId) return
    setBusyId(topic.id)
    setError(null)
    try {
      await setTopicArchived(topic.id, archived)
      if (editingId === topic.id) setEditingId(null)
      await refresh(true)
    } catch (archiveError) {
      setError(`${archived ? '归档' : '恢复'}主题失败：${String(archiveError)}`)
    } finally {
      setBusyId(null)
    }
  }

  const beginTagEdit = (tag: TagRecord) => {
    if (renamingTagId || deletingTagId) return
    setEditingTagId(tag.id)
    setTagNameDraft(tag.name)
    setConfirmTagId(null)
    setError(null)
  }

  const saveTagEdit = async (event: FormEvent, tag: TagRecord) => {
    event.preventDefault()
    const nextName = tagNameDraft.trim()
    if (!nextName || renamingTagId || deletingTagId) return
    setRenamingTagId(tag.id)
    setError(null)
    try {
      await updateTag(tag.id, nextName)
      setEditingTagId(null)
      await refresh(true)
    } catch (updateError) {
      setError(`保存标签失败：${String(updateError)}`)
    } finally {
      setRenamingTagId(null)
    }
  }

  const removeEmptyTag = async (tag: TagRecord) => {
    if (tag.knowledgeCount !== 0 || deletingTagId || renamingTagId) return
    if (confirmTagId !== tag.id) {
      setConfirmTagId(tag.id)
      return
    }
    setDeletingTagId(tag.id)
    setError(null)
    try {
      await deleteEmptyTag(tag.id)
      setConfirmTagId(null)
      await refresh(true)
    } catch (deleteError) {
      setError(`删除空标签失败：${String(deleteError)}`)
    } finally {
      setDeletingTagId(null)
    }
  }

  return (
    <section className="topics-view" aria-label="主题与标签">
      <header className="workspace-page-heading topic-heading">
        <div>
          <h1>主题</h1>
          <p>主题负责长期组织知识；归档只隐藏主题，不会删除原有知识或关联。</p>
        </div>
        {archivedTopics.length > 0 && (
          <button type="button" className={showArchived ? 'topic-history-toggle active' : 'topic-history-toggle'} onClick={() => setShowArchived((value) => !value)}>
            <Archive size={13} strokeWidth={1.8} />
            <span>{showArchived ? '隐藏历史' : `历史 ${archivedTopics.length}`}</span>
          </button>
        )}
      </header>

      <form className="topic-create" onSubmit={(event) => void submit(event)}>
        <Tags size={16} strokeWidth={1.8} />
        <input value={name} maxLength={120} placeholder="新建主题…" onChange={(event) => setName(event.target.value)} />
        <button type="submit" disabled={!name.trim() || creating}>
          <Plus size={14} strokeWidth={1.8} />
          <span>{creating ? '创建中' : '创建'}</span>
        </button>
      </form>

      {error && <button type="button" className="workspace-error" onClick={() => void refresh()}> {error} · 点击刷新</button>}
      {!error && loading ? (
        <div className="workspace-empty">正在读取主题与标签…</div>
      ) : !error && visibleTopics.length === 0 ? (
        <div className="workspace-empty">{showArchived ? '没有历史主题。' : '还没有活动主题。可以先保存知识，再逐步整理。'}</div>
      ) : !error && (
        <div className="topic-list">
          {visibleTopics.map((topic) => {
            const archived = topic.archivedAt !== null
            const busy = busyId === topic.id
            return (
              <div className={archived ? 'topic-item archived' : 'topic-item'} key={topic.id}>
                <div className="topic-item-row">
                  <button type="button" className="topic-open" onClick={() => onOpenTopic(topic.id)}>
                    <span>
                      <span className="topic-name-row">
                        <strong>{topic.name}</strong>
                        {archived && <em>已归档</em>}
                        {topic.locked && <LockKeyhole size={11} strokeWidth={1.8} />}
                      </span>
                      <small>{topic.description || (archived ? '历史主题 · 关联知识仍保留' : topic.createdBy === 'agent' ? '由 AI 整理建议创建' : '暂无描述')}</small>
                    </span>
                    <b>{topic.knowledgeCount}</b>
                  </button>
                  <div className="topic-actions">
                    {!topic.locked && !archived && (
                      <>
                        <button type="button" title="编辑主题" disabled={Boolean(busyId)} onClick={() => beginEdit(topic)}><Pencil size={13} strokeWidth={1.8} /></button>
                        <button type="button" title="归档主题" disabled={Boolean(busyId)} onClick={() => void changeArchiveState(topic, true)}>{busy ? <span className="topic-action-wait">…</span> : <Archive size={13} strokeWidth={1.8} />}</button>
                      </>
                    )}
                    {!topic.locked && archived && (
                      <button type="button" title="恢复主题" disabled={Boolean(busyId)} onClick={() => void changeArchiveState(topic, false)}>{busy ? <span className="topic-action-wait">…</span> : <ArchiveRestore size={13} strokeWidth={1.8} />}</button>
                    )}
                  </div>
                </div>

                {editingId === topic.id && (
                  <form className="topic-edit-panel" onSubmit={(event) => void saveEdit(event, topic)}>
                    <input value={editName} maxLength={120} aria-label="主题名称" onChange={(event) => setEditName(event.target.value)} />
                    <textarea value={editDescription} maxLength={2000} rows={2} placeholder="主题描述（可选）" aria-label="主题描述" onChange={(event) => setEditDescription(event.target.value)} />
                    <div>
                      <button type="button" disabled={busy} onClick={() => setEditingId(null)}><X size={12} strokeWidth={1.8} />取消</button>
                      <button type="submit" className="primary" disabled={!editName.trim() || busy}><Check size={12} strokeWidth={1.8} />{busy ? '保存中' : '保存'}</button>
                    </div>
                  </form>
                )}
              </div>
            )
          })}
        </div>
      )}

      {!loading && (
        <section className="tag-maintenance" aria-label="标签维护">
          <div className="tag-maintenance-head">
            <div>
              <Tags size={14} strokeWidth={1.8} />
              <strong>标签维护</strong>
              <span>{tags.length} 个标签 · {emptyTagCount} 个未使用</span>
            </div>
            <small>只允许删除没有任何知识关联的空标签。</small>
          </div>
          {tags.length === 0 ? (
            <div className="tag-maintenance-empty">还没有标签。标签会在知识详情或 AI 整理建议中逐步产生。</div>
          ) : (
            <div className="tag-maintenance-list">
              {tags.map((tag) => (
                <div key={tag.id} className={tag.knowledgeCount === 0 ? 'empty' : undefined}>
                  {editingTagId === tag.id ? (
                    <form className="tag-rename" onSubmit={(event) => void saveTagEdit(event, tag)}>
                      <input value={tagNameDraft} maxLength={80} autoFocus aria-label="标签名称" onChange={(event) => setTagNameDraft(event.target.value)} />
                      <button type="submit" className="save" disabled={!tagNameDraft.trim() || Boolean(renamingTagId)}><Check size={11} strokeWidth={1.8} /></button>
                      <button type="button" disabled={Boolean(renamingTagId)} onClick={() => setEditingTagId(null)}><X size={11} strokeWidth={1.8} /></button>
                    </form>
                  ) : (
                    <>
                      <span><strong>{tag.name}</strong><small>{tag.knowledgeCount} 条知识</small></span>
                      <div className="tag-maintenance-actions">
                        <button type="button" title="重命名标签" disabled={Boolean(renamingTagId || deletingTagId)} onClick={() => beginTagEdit(tag)}><Pencil size={11} strokeWidth={1.8} /></button>
                        {tag.knowledgeCount === 0 && (
                          <button
                            type="button"
                            className={confirmTagId === tag.id ? 'confirm' : undefined}
                            disabled={Boolean(deletingTagId || renamingTagId)}
                            onClick={() => void removeEmptyTag(tag)}>
                            {confirmTagId === tag.id ? <Check size={11} strokeWidth={1.8} /> : <Trash2 size={11} strokeWidth={1.8} />}
                            <span>{deletingTagId === tag.id ? '删除中' : confirmTagId === tag.id ? '确认' : '删除'}</span>
                          </button>
                        )}
                      </div>
                    </>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      )}
    </section>
  )
}
