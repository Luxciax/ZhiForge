export type CurationClassification = 'duplicate' | 'related' | 'conflict'
export type CurationCandidateStatus = 'pending' | 'applied' | 'dismissed' | 'stale'

export interface CurationCandidateRecord {
  id: string
  runId: string
  leftKnowledgeId: string
  rightKnowledgeId: string
  leftTitle: string
  rightTitle: string
  classification: CurationClassification
  relationType: string | null
  rationale: string
  confidence: number
  status: CurationCandidateStatus
  createdAt: number
  decidedAt: number | null
}

export interface CurationRunDetail {
  id: string
  status: 'running' | 'ready' | 'failed'
  knowledgeCount: number
  candidateCount: number
  summary: string | null
  error: string | null
  createdAt: number
  updatedAt: number
  candidates: CurationCandidateRecord[]
}
