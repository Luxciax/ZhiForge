import { Check, ExternalLink, LoaderCircle, LocateFixed, RotateCw } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  createKnowledgeAttempt,
  judgeKnowledgeAttempt,
  listKnowledgeAttempts
} from '../../services/knowledge-service'
import type {
  EvidenceRecord,
  KnowledgeAttemptRecord,
  KnowledgeQuestionRecord
} from '../../types'

const RESULT_LABELS = {
  correct: '理解正确',
  partial: '还差一点',
  wrong: '需要修正'
} as const

const QUESTION_LABELS = {
  recall: '回忆',
  explain: '解释',
  apply: '应用'
} as const

const MASTERY_DELTA = {
  correct: 15,
  partial: 5,
  wrong: -10
} as const

function latestAttempt(attempts: KnowledgeAttemptRecord[], questionId: string) {
  return [...attempts].reverse().find((attempt) => attempt.questionId === questionId) ?? null
}

function PointSection({ label, values, tone }: { label: string; values: string[]; tone: string }) {
  if (!values.length) return null
  return (
    <div className={`quiz-points ${tone}`}>
      <span>{label}</span>
      <ul>{values.map((value) => <li key={value}>{value}</li>)}</ul>
    </div>
  )
}

interface KnowledgeQuizProps {
  knowledgeUnitId: string
  questions: KnowledgeQuestionRecord[]
  evidence: EvidenceRecord[]
  initialQuestionId?: string | null
  currentMasteryScore?: number | null
  canOpenSource?: boolean
  onLocateEvidence: (evidence: EvidenceRecord) => void
  onOpenSource?: () => void
  onJudged: (masteryScore: number) => void
  onContinueReview?: () => void
  preferContinueReview?: boolean
}

export function KnowledgeQuiz({
  knowledgeUnitId,
  questions,
  evidence,
  initialQuestionId,
  currentMasteryScore = null,
  canOpenSource = false,
  onLocateEvidence,
  onOpenSource,
  onJudged,
  onContinueReview,
  preferContinueReview = false
}: KnowledgeQuizProps) {
  const initialId = initialQuestionId && questions.some((question) => question.id === initialQuestionId)
    ? initialQuestionId
    : (questions[0]?.id ?? '')
  const [attempts, setAttempts] = useState<KnowledgeAttemptRecord[]>([])
  const [activeQuestionId, setActiveQuestionId] = useState(initialId)
  const [answer, setAnswer] = useState('')
  const [loading, setLoading] = useState(true)
  const [judging, setJudging] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [masteryScore, setMasteryScore] = useState<number | null>(currentMasteryScore)
  const feedbackRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    setActiveQuestionId((current) => {
      if (initialQuestionId && questions.some((question) => question.id === initialQuestionId)) {
        return initialQuestionId
      }
      return questions.some((question) => question.id === current) ? current : (questions[0]?.id ?? '')
    })
  }, [initialQuestionId, questions])

  useEffect(() => {
    setMasteryScore(currentMasteryScore)
  }, [currentMasteryScore])

  useEffect(() => {
    let disposed = false
    setAttempts([])
    setAnswer('')
    setError(null)
    setLoading(true)
    void listKnowledgeAttempts(knowledgeUnitId)
      .then((items) => {
        if (!disposed) setAttempts(items)
      })
      .catch((loadError) => {
        console.error('Failed to load attempts', loadError)
        if (!disposed) setError('答题记录读取失败。')
      })
      .finally(() => {
        if (!disposed) setLoading(false)
      })
    return () => { disposed = true }
  }, [knowledgeUnitId])

  const activeQuestion = questions.find((question) => question.id === activeQuestionId) ?? questions[0]
  const activeAttempt = activeQuestion ? latestAttempt(attempts, activeQuestion.id) : null
  const pendingAttempt = activeAttempt && !activeAttempt.result ? activeAttempt : null
  const activeQuestionIndex = activeQuestion ? questions.findIndex((question) => question.id === activeQuestion.id) : -1
  const nextQuestion = useMemo(() => {
    if (activeQuestionIndex < 0 || questions.length <= 1) return undefined
    const candidates = [
      ...questions.slice(activeQuestionIndex + 1),
      ...questions.slice(0, activeQuestionIndex)
    ]
    return candidates.find((question) => latestAttempt(attempts, question.id)?.result !== 'correct')
  }, [activeQuestionIndex, attempts, questions])

  const attemptEvidence = useMemo(() => {
    if (!activeAttempt?.evidenceIds.length) return []
    const ids = new Set(activeAttempt.evidenceIds)
    return evidence.filter((item) => ids.has(item.id))
  }, [activeAttempt, evidence])

  if (!activeQuestion) return null

  const judgeAttempt = async (attempt: KnowledgeAttemptRecord) => {
    setJudging(true)
    setError(null)
    try {
      const judged = await judgeKnowledgeAttempt(attempt.id)
      setAttempts((current) => current.map((item) => item.id === judged.attempt.id ? judged.attempt : item))
      setMasteryScore(judged.masteryScore)
      setAnswer('')
      onJudged(judged.masteryScore)
      window.setTimeout(() => feedbackRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }), 0)
    } catch (judgeError) {
      console.error('Failed to judge attempt', judgeError)
      setError('答案已保存，但 AI 判题失败。可以直接重新判题。')
    } finally {
      setJudging(false)
    }
  }

  const submit = async () => {
    if (judging) return
    if (pendingAttempt) {
      await judgeAttempt(pendingAttempt)
      return
    }
    const normalized = answer.trim()
    if (!normalized) {
      setError('先写下你的理解。')
      return
    }

    setJudging(true)
    setError(null)
    try {
      const created = await createKnowledgeAttempt(activeQuestion.id, normalized)
      setAttempts((current) => [...current, created])
      setJudging(false)
      await judgeAttempt(created)
    } catch (submitError) {
      console.error('Failed to save attempt', submitError)
      setError('答案保存失败，请重试。')
      setJudging(false)
    }
  }

  return (
    <section className="knowledge-detail-card knowledge-quiz">
      <div className="knowledge-section-heading">
        <span className="knowledge-detail-label">理解检验</span>
        <span>第 {activeQuestionIndex + 1} / {questions.length} 题</span>
      </div>

      <div className="quiz-tabs" role="tablist" aria-label="理解题">
        {questions.map((question, index) => {
          const attempt = latestAttempt(attempts, question.id)
          return (
            <button
              type="button"
              key={question.id}
              className={question.id === activeQuestion.id ? 'active' : undefined}
              onClick={() => {
                setActiveQuestionId(question.id)
                setAnswer('')
                setError(null)
              }}>
              <span>{index + 1}</span>
              {attempt?.result === 'correct' && <Check size={11} strokeWidth={2} />}
            </button>
          )
        })}
      </div>

      <div className="quiz-question">
        <div className="quiz-question-meta">
          <span>{QUESTION_LABELS[activeQuestion.questionType]}</span>
          <span>难度 {activeQuestion.difficulty}</span>
        </div>
        <strong>{activeQuestion.question}</strong>
      </div>

      {loading ? (
        <div className="quiz-loading"><LoaderCircle size={14} strokeWidth={1.8} className="spin" />读取答题记录</div>
      ) : pendingAttempt ? (
        <div className="quiz-pending">
          <span>已保存的答案</span>
          <p>{pendingAttempt.answer}</p>
          <button type="button" onClick={() => void submit()} disabled={judging}>
            {judging ? <LoaderCircle size={13} strokeWidth={1.8} className="spin" /> : <RotateCw size={13} strokeWidth={1.8} />}
            <span>{judging ? '判题中' : '重新判题'}</span>
          </button>
        </div>
      ) : (
        <div className="quiz-answer-box">
          <textarea
            value={answer}
            rows={4}
            maxLength={20000}
            placeholder="用自己的话写下理解…"
            onChange={(event) => setAnswer(event.target.value)}
            onKeyDown={(event) => {
              if ((event.ctrlKey || event.metaKey) && event.key === 'Enter' && answer.trim() && !judging) {
                event.preventDefault()
                void submit()
              }
            }}
          />
          <button type="button" onClick={() => void submit()} disabled={judging || !answer.trim()}>
            {judging && <LoaderCircle size={13} strokeWidth={1.8} className="spin" />}
            <span>{judging ? 'AI 判题中' : activeAttempt?.result ? '再次作答' : '提交答案'}</span>
          </button>
        </div>
      )}

      {error && <div className="quiz-error">{error}</div>}

      {activeAttempt?.result && (
        <div ref={feedbackRef} className={`quiz-feedback ${activeAttempt.result}`}>
          <div className="quiz-feedback-head">
            <strong>{RESULT_LABELS[activeAttempt.result]}</strong>
            <small>
              本次 {MASTERY_DELTA[activeAttempt.result] > 0 ? '+' : ''}{MASTERY_DELTA[activeAttempt.result]}
              {masteryScore !== null && <> · 当前 {masteryScore}%</>}
            </small>
          </div>
          {activeAttempt.feedback && <p>{activeAttempt.feedback}</p>}
          <PointSection label="你理解了" values={activeAttempt.correctPoints} tone="correct" />
          <PointSection label="还缺" values={activeAttempt.missingPoints} tone="missing" />
          <PointSection label="需要修正" values={activeAttempt.wrongPoints} tone="wrong" />
          {(attemptEvidence.length > 0 || canOpenSource) && (
            <div className="quiz-feedback-actions">
              {attemptEvidence[0] && (
                <button type="button" onClick={() => onLocateEvidence(attemptEvidence[0])}>
                  <LocateFixed size={12} strokeWidth={1.8} />
                  <span>回原文看这里</span>
                </button>
              )}
              {canOpenSource && onOpenSource && (
                <button type="button" onClick={onOpenSource}>
                  <ExternalLink size={12} strokeWidth={1.8} />
                  <span>打开知乎原文</span>
                </button>
              )}
            </div>
          )}
          {activeAttempt.result === 'correct' && ((preferContinueReview && onContinueReview) || nextQuestion || onContinueReview) && (
            <div className="quiz-continue-row">
              <button
                type="button"
                onClick={() => {
                  if (preferContinueReview && onContinueReview) {
                    onContinueReview()
                  } else if (nextQuestion) {
                    setActiveQuestionId(nextQuestion.id)
                    setAnswer('')
                    setError(null)
                    feedbackRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
                  } else {
                    onContinueReview?.()
                  }
                }}>
                {preferContinueReview && onContinueReview ? '下一条知识' : nextQuestion ? '下一题' : '下一条知识'}
              </button>
            </div>
          )}
        </div>
      )}
    </section>
  )
}
