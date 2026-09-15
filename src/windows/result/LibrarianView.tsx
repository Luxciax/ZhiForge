import { Check, ChevronLeft, CircleHelp, Clock3, Copy, FolderTree, HeartPulse, Link2, LoaderCircle, RefreshCw, Tag, TriangleAlert, Waypoints, X } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  analyzeKnowledgeLibrary,
  applyLibrarianProposal,
  dismissLibrarianProposal,
  getLatestLibrarianRun
} from '../../services/librarian-service'
import {
  analyzeKnowledgeCuration,
  applyCurationCandidate,
  dismissCurationCandidate,
  getLatestCurationRun
} from '../../services/curation-service'
import type {
  LibrarianActionType,
  LibrarianProposalRecord,
  LibrarianRunDetail
} from '../../services/librarian-types'
import type {
  CurationCandidateRecord,
  CurationClassification,
  CurationRunDetail
} from '../../services/curation-types'
import { getKnowledgeHealth, type KnowledgeHealthReport } from '../../services/knowledge-service'
import { DuplicateMergePanel } from './DuplicateMergePanel'
import './knowledge-home.css'

interface LibrarianViewProps {
  onBack: () => void
  onChanged: () => void
  onOpenKnowledge: (knowledgeUnitId: string) => void
  backLabel?: string
}

type HealthBucketKey = Exclude<keyof KnowledgeHealthReport, 'totalKnowledge' | 'generatedAt'>

const HEALTH_META: Array<{ key: HealthBucketKey; label: string; description: string; icon: typeof Tag }> = [
  { key: 'uncategorized', label: '未归类', description: '还没有加入任何主题', icon: FolderTree },
  { key: 'noQuestions', label: '无题目', description: '当前没有可检验的问题', icon: CircleHelp },
  { key: 'neverReviewed', label: '从未复习', description: '已有题目，但还没有完成过检验', icon: Clock3 },
  { key: 'possibleDuplicates', label: '待确认重复', description: '来自最新内容审查的重复候选', icon: Copy },
  { key: 'possibleConflicts', label: '待确认冲突', description: '来自最新内容审查的冲突候选', icon: TriangleAlert },
  { key: 'sourceLinkIssues', label: '来源链接异常', description: '网页或知乎来源缺少可用 HTTP 链接', icon: Link2 },
  { key: 'staleKnowledge', label: '可能过时', description: '质量状态已标记为 stale', icon: Clock3 }
]

const ACTION_META: Record<LibrarianActionType, { label: string; icon: typeof Tag }> = {
  topic_assign: { label: '主题', icon: FolderTree },
  tag_add: { label: '标签', icon: Tag },
  relation_create: { label: '关系', icon: Waypoints }
}

const STATUS_LABELS: Record<LibrarianProposalRecord['status'], string> = {
  pending: '待确认',
  applied: '已应用',
  dismissed: '已忽略',
  stale: '已失效'
}

const CURATION_META: Record<CurationClassification, { label: string; icon: typeof Copy; action: string }> = {
  duplicate: { label: '可能重复', icon: Copy, action: '确认重复' },
  related: { label: '相关知识', icon: Link2, action: '建立关系' },
  conflict: { label: '观点冲突', icon: TriangleAlert, action: '确认冲突' }
}

const CURATION_STATUS_LABELS: Record<CurationCandidateRecord['status'], string> = {
  pending: '待确认',
  applied: '已确认',
  dismissed: '已忽略',
  stale: '已失效'
}

function confidenceLabel(value: number) {
  if (value >= 0.9) return '高把握'
  if (value >= 0.78) return '较高把握'
  return '可参考'
}

export function LibrarianView({ onBack, onChanged, onOpenKnowledge, backLabel = '知识首页' }: LibrarianViewProps) {
  const [run, setRun] = useState<LibrarianRunDetail | null>(null)
  const [health, setHealth] = useState<KnowledgeHealthReport | null>(null)
  const [loading, setLoading] = useState(true)
  const [analyzing, setAnalyzing] = useState(false)
  const [busyProposalId, setBusyProposalId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [curationRun, setCurationRun] = useState<CurationRunDetail | null>(null)
  const [curationLoading, setCurationLoading] = useState(true)
  const [curationAnalyzing, setCurationAnalyzing] = useState(false)
  const [busyCandidateId, setBusyCandidateId] = useState<string | null>(null)
  const [curationError, setCurationError] = useState<string | null>(null)
  const [mergeCandidateId, setMergeCandidateId] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [organization, curation, healthReport] = await Promise.all([
        getLatestLibrarianRun(),
        getLatestCurationRun(),
        getKnowledgeHealth()
      ])
      setRun(organization)
      setCurationRun(curation)
      setHealth(healthReport)
      setError(null)
      setCurationError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
      setCurationLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const pendingCount = useMemo(
    () => run?.proposals.filter((proposal) => proposal.status === 'pending').length ?? 0,
    [run]
  )
  const pendingCurationCount = useMemo(
    () => curationRun?.candidates.filter((candidate) => candidate.status === 'pending').length ?? 0,
    [curationRun]
  )
  const healthSignalCount = useMemo(
    () => health ? HEALTH_META.reduce((sum, item) => sum + health[item.key].count, 0) : 0,
    [health]
  )

  const analyze = async () => {
    if (analyzing) return
    setAnalyzing(true)
    setError(null)
    try {
      setRun(await analyzeKnowledgeLibrary())
    } catch (analyzeError) {
      setError(String(analyzeError))
      await refresh()
    } finally {
      setAnalyzing(false)
    }
  }

  const decide = async (proposal: LibrarianProposalRecord, decision: 'apply' | 'dismiss') => {
    if (busyProposalId) return
    setBusyProposalId(proposal.id)
    setError(null)
    try {
      if (decision === 'apply') {
        await applyLibrarianProposal(proposal.id)
        onChanged()
      } else {
        await dismissLibrarianProposal(proposal.id)
      }
      await refresh()
    } catch (decisionError) {
      setError(String(decisionError))
    } finally {
      setBusyProposalId(null)
    }
  }

  const analyzeCuration = async () => {
    if (curationAnalyzing) return
    setCurationAnalyzing(true)
    setCurationError(null)
    try {
      setCurationRun(await analyzeKnowledgeCuration())
      setHealth(await getKnowledgeHealth())
    } catch (analyzeError) {
      setCurationError(String(analyzeError))
      await refresh()
    } finally {
      setCurationAnalyzing(false)
      setCurationLoading(false)
    }
  }

  const decideCuration = async (candidate: CurationCandidateRecord, decision: 'apply' | 'dismiss') => {
    if (busyCandidateId) return
    setBusyCandidateId(candidate.id)
    setCurationError(null)
    try {
      if (decision === 'apply') {
        await applyCurationCandidate(candidate.id)
        onChanged()
      } else {
        await dismissCurationCandidate(candidate.id)
      }
      await refresh()
    } catch (decisionError) {
      setCurationError(String(decisionError))
    } finally {
      setBusyCandidateId(null)
    }
  }

  return (
    <section className="knowledge-recent librarian-view" aria-label="知识整理">
      <div className="librarian-heading">
        <button type="button" className="knowledge-back" onClick={onBack}>
          <ChevronLeft size={14} strokeWidth={1.8} />
          <span>{backLabel}</span>
        </button>
        <span>AI 只提出建议，不会直接修改知识库</span>
      </div>

      <div className="librarian-intro">
        <div>
          <h1>整理建议</h1>
          <p>统一处理主题、标签、知识关系、重复与冲突建议。AI 只提出候选，所有真正写入或合并仍由你确认。</p>
        </div>
        <button type="button" className="librarian-analyze" disabled={analyzing} onClick={() => void analyze()}>
          {analyzing ? <LoaderCircle size={14} strokeWidth={1.8} className="spin" /> : <RefreshCw size={14} strokeWidth={1.8} />}
          <span>{analyzing ? '正在检查…' : run ? '重新检查' : '检查知识库'}</span>
        </button>
      </div>

      <section className="knowledge-health-section" aria-label="知识健康">
        <div className="knowledge-health-heading">
          <div>
            <HeartPulse size={15} strokeWidth={1.8} />
            <span><strong>知识健康</strong><small>本地确定性检查，不调用模型</small></span>
          </div>
          <span>{health ? `${health.totalKnowledge} 条知识 · ${healthSignalCount} 个问题信号` : '正在扫描…'}</span>
        </div>
        {health ? (
          <div className="knowledge-health-list">
            {HEALTH_META.map((meta) => {
              const bucket = health[meta.key]
              const Icon = meta.icon
              return (
                <div className={`knowledge-health-row${bucket.count === 0 ? ' clear' : ''}`} key={meta.key}>
                  <div className="knowledge-health-kind"><Icon size={14} strokeWidth={1.8} /><span>{meta.label}</span><b>{bucket.count}</b></div>
                  <div className="knowledge-health-copy">
                    <span>{meta.description}</span>
                    {bucket.samples.length > 0 && (
                      <div className="knowledge-health-samples">
                        {bucket.samples.map((sample) => (
                          <button type="button" key={`${meta.key}:${sample.knowledgeUnitId}:${sample.otherKnowledgeUnitId ?? ''}`} onClick={() => onOpenKnowledge(sample.knowledgeUnitId)}>
                            <strong>{sample.title}</strong>
                            {sample.otherTitle && <small>↔ {sample.otherTitle}</small>}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )
            })}
          </div>
        ) : (
          <div className="librarian-empty"><LoaderCircle size={14} strokeWidth={1.8} className="spin" />正在扫描知识健康…</div>
        )}
      </section>

      {loading && !run ? (
        <div className="librarian-empty"><LoaderCircle size={15} strokeWidth={1.8} className="spin" />正在读取整理记录…</div>
      ) : analyzing ? (
        <div className="librarian-running">
          <LoaderCircle size={16} strokeWidth={1.8} className="spin" />
          <div><strong>正在检查主题、标签和关系</strong><span>只分析已有核心知识与当前组织结构，不会修改任何内容。</span></div>
        </div>
      ) : run?.status === 'failed' ? (
        <div className="librarian-error-state">
          <strong>这次整理没有完成</strong>
          <span>{run.error || '模型没有返回可用的结构化建议。'}</span>
        </div>
      ) : run ? (
        <>
          <div className="librarian-summary">
            <div><strong>{run.summary || '检查完成。'}</strong><span>检查了 {run.knowledgeCount} 条知识{pendingCount ? ` · ${pendingCount} 条待确认` : ''}</span></div>
          </div>
          <div className="librarian-proposals">
            {run.proposals.length === 0 ? (
              <div className="librarian-empty">目前没有足够明确的整理建议，知识库保持不变。</div>
            ) : run.proposals.map((proposal) => {
              const meta = ACTION_META[proposal.actionType]
              const Icon = meta.icon
              const busy = busyProposalId === proposal.id
              return (
                <article className={`librarian-proposal ${proposal.status}`} key={proposal.id}>
                  <div className="librarian-proposal-kind"><Icon size={14} strokeWidth={1.8} /><span>{meta.label}</span></div>
                  <div className="librarian-proposal-copy">
                    <strong>{proposal.description}</strong>
                    <p>{proposal.rationale}</p>
                    <small>{confidenceLabel(proposal.confidence)}</small>
                  </div>
                  {proposal.status === 'pending' ? (
                    <div className="librarian-proposal-actions">
                      <button type="button" disabled={Boolean(busyProposalId)} onClick={() => void decide(proposal, 'dismiss')}>
                        <X size={12} strokeWidth={1.8} /><span>忽略</span>
                      </button>
                      <button type="button" className="primary" disabled={Boolean(busyProposalId)} onClick={() => void decide(proposal, 'apply')}>
                        {busy ? <LoaderCircle size={12} className="spin" /> : <Check size={12} strokeWidth={1.9} />}<span>应用</span>
                      </button>
                    </div>
                  ) : (
                    <span className={`librarian-proposal-status ${proposal.status}`}>{STATUS_LABELS[proposal.status]}</span>
                  )}
                </article>
              )
            })}
          </div>
        </>
      ) : (
        <div className="librarian-empty">还没有整理记录。运行一次检查后，AI 的建议会先列在这里供你确认。</div>
      )}

      <section className="curation-section" aria-label="内容审查">
        <div className="curation-heading">
          <div>
            <h2>内容审查</h2>
            <p>先用本地 FTS 与同主题关系召回少量候选，再让 AI 只判断这些候选。确认重复不会自动合并原始知识。</p>
          </div>
          <button type="button" disabled={curationAnalyzing} onClick={() => void analyzeCuration()}>
            {curationAnalyzing ? <LoaderCircle size={13} className="spin" /> : <RefreshCw size={13} strokeWidth={1.8} />}
            <span>{curationAnalyzing ? '正在审查…' : curationRun ? '重新审查' : '检查重复与冲突'}</span>
          </button>
        </div>

        {curationLoading && !curationRun ? (
          <div className="curation-empty">正在读取内容审查记录…</div>
        ) : curationAnalyzing ? (
          <div className="curation-empty">正在比较本地候选知识，原始 Source 不会被修改。</div>
        ) : curationRun?.status === 'failed' ? (
          <div className="librarian-error-state">
            <strong>这次内容审查没有完成</strong>
            <span>{curationRun.error || '模型没有返回可用的分类结果。'}</span>
          </div>
        ) : curationRun ? (
          <>
            <div className="curation-summary">
              <strong>{curationRun.summary || '内容审查完成。'}</strong>
              <span>扫描 {curationRun.knowledgeCount} 条知识 · 本地召回 {curationRun.candidateCount} 对{pendingCurationCount ? ` · ${pendingCurationCount} 条待确认` : ''}</span>
            </div>
            <div className="curation-list">
              {curationRun.candidates.length === 0 ? (
                <div className="curation-empty">没有发现足够明确的重复、关联或冲突。</div>
              ) : curationRun.candidates.map((candidate) => {
                const meta = CURATION_META[candidate.classification]
                const Icon = meta.icon
                const busy = busyCandidateId === candidate.id
                return (
                  <article className={`curation-item ${candidate.classification} ${candidate.status}`} key={candidate.id}>
                    <div className="curation-kind"><Icon size={14} strokeWidth={1.8} /><span>{meta.label}</span></div>
                    <div className="curation-copy">
                      <div className="curation-pair">
                        <strong>{candidate.leftTitle}</strong>
                        <span>↔</span>
                        <strong>{candidate.rightTitle}</strong>
                      </div>
                      <p>{candidate.rationale}</p>
                      <small>{confidenceLabel(candidate.confidence)}{candidate.relationType ? ` · ${candidate.relationType}` : ''}</small>
                    </div>
                    {candidate.status === 'pending' ? (
                      <div className="librarian-proposal-actions">
                        <button type="button" disabled={Boolean(busyCandidateId)} onClick={() => void decideCuration(candidate, 'dismiss')}>
                          <X size={12} strokeWidth={1.8} /><span>忽略</span>
                        </button>
                        <button type="button" className="primary" disabled={Boolean(busyCandidateId)} onClick={() => void decideCuration(candidate, 'apply')}>
                          {busy ? <LoaderCircle size={12} className="spin" /> : <Check size={12} strokeWidth={1.9} />}<span>{meta.action}</span>
                        </button>
                      </div>
                    ) : (
                      <div className="curation-decided-actions">
                        <span className={`librarian-proposal-status ${candidate.status}`}>{CURATION_STATUS_LABELS[candidate.status]}</span>
                        {candidate.classification === 'duplicate' && candidate.status === 'applied' && (
                          <button
                            type="button"
                            className="curation-merge-open"
                            onClick={() => setMergeCandidateId((current) => current === candidate.id ? null : candidate.id)}>
                            <span>{mergeCandidateId === candidate.id ? '收起合并' : '合并…'}</span>
                          </button>
                        )}
                      </div>
                    )}
                    {candidate.classification === 'duplicate' && candidate.status === 'applied' && mergeCandidateId === candidate.id && (
                      <DuplicateMergePanel
                        candidateId={candidate.id}
                        onClose={() => setMergeCandidateId(null)}
                        onChanged={() => {
                          onChanged()
                          void refresh()
                        }}
                      />
                    )}
                  </article>
                )
              })}
            </div>
          </>
        ) : (
          <div className="curation-empty">还没有内容审查记录。</div>
        )}
        {curationError && <div className="quiz-error">{curationError}</div>}
      </section>

      {error && <div className="quiz-error">{error}</div>}
    </section>
  )
}
