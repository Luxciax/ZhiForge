import { Check, LoaderCircle, RotateCcw, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { applyDuplicateMerge, previewDuplicateMerge, revertDuplicateMerge } from '../../services/merge-service'
import type { DuplicateMergePreview, MergeSideSummary } from '../../services/merge-types'

interface DuplicateMergePanelProps {
  candidateId: string
  onClose: () => void
  onChanged: () => void
}

function summary(side: MergeSideSummary) {
  return `${side.sourceCount} 来源 · ${side.evidenceCount} 证据 · ${side.reviewCount} 次复习${side.reviewCount ? ` · 掌握 ${side.masteryScore}` : ''}`
}

export function DuplicateMergePanel({ candidateId, onClose, onChanged }: DuplicateMergePanelProps) {
  const [preview, setPreview] = useState<DuplicateMergePreview | null>(null)
  const [targetId, setTargetId] = useState('')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const next = await previewDuplicateMerge(candidateId)
      setPreview(next)
      setTargetId(next.recommendedTargetId)
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void load()
  }, [candidateId])

  const apply = async () => {
    if (!preview || !targetId || busy) return
    setBusy(true)
    setError(null)
    try {
      await applyDuplicateMerge(
        candidateId,
        targetId,
        preview.left.updatedAt,
        preview.right.updatedAt
      )
      await load()
      onChanged()
    } catch (applyError) {
      setError(String(applyError))
    } finally {
      setBusy(false)
    }
  }

  const revert = async () => {
    if (!preview?.existingMerge || busy) return
    setBusy(true)
    setError(null)
    try {
      await revertDuplicateMerge(preview.existingMerge.id)
      await load()
      onChanged()
    } catch (revertError) {
      setError(String(revertError))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="curation-merge-panel">
      <div className="curation-merge-head">
        <div>
          <strong>{preview?.existingMerge ? '重复知识已合并' : '合并重复知识'}</strong>
          <span>{preview?.existingMerge ? '另一条知识已归档，原始 Source 和复习记录仍保留。' : '选择要保留的主知识。另一条只会归档，不会删除 Source，也不会迁移 mastery。'}</span>
        </div>
        <button type="button" onClick={onClose} aria-label="关闭合并预览"><X size={13} strokeWidth={1.8} /></button>
      </div>

      {loading && !preview ? (
        <div className="curation-merge-loading"><LoaderCircle size={14} className="spin" />正在读取合并预览…</div>
      ) : preview ? (
        <>
          <div className="curation-merge-options">
            {[preview.left, preview.right].map((side) => {
              const active = targetId === side.id
              const kept = preview.existingMerge?.targetKnowledgeId === side.id
              return (
                <button
                  type="button"
                  className={`curation-merge-choice${active ? ' active' : ''}`}
                  key={side.id}
                  disabled={Boolean(preview.existingMerge) || busy}
                  onClick={() => setTargetId(side.id)}>
                  <span>{kept ? '当前保留' : side.id === preview.recommendedTargetId ? '推荐保留' : '可选'}</span>
                  <strong>{side.title}</strong>
                  <small>{summary(side)}</small>
                </button>
              )
            })}
          </div>
          <p className="curation-merge-reason">{preview.recommendationReason}</p>
          <div className="curation-merge-note">合并只补齐缺失的来源、支持证据、主题、标签和关系；两边原有的测验、回答、复习次数与 mastery 各自保留，不做数学合并。</div>
          <div className="curation-merge-actions">
            {preview.existingMerge ? (
              <button type="button" disabled={busy} onClick={() => void revert()}>
                {busy ? <LoaderCircle size={12} className="spin" /> : <RotateCcw size={12} strokeWidth={1.8} />}
                <span>撤销合并</span>
              </button>
            ) : (
              <button type="button" className="primary" disabled={busy || !targetId} onClick={() => void apply()}>
                {busy ? <LoaderCircle size={12} className="spin" /> : <Check size={12} strokeWidth={1.9} />}
                <span>执行合并</span>
              </button>
            )}
          </div>
        </>
      ) : null}
      {error && <div className="quiz-error">{error}</div>}
    </div>
  )
}
