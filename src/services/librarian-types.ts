export type LibrarianActionType = 'topic_assign' | 'tag_add' | 'relation_create'
export type LibrarianProposalStatus = 'pending' | 'applied' | 'dismissed' | 'stale'

export interface LibrarianProposalRecord {
  id: string
  runId: string
  actionType: LibrarianActionType
  description: string
  rationale: string
  confidence: number
  status: LibrarianProposalStatus
  createdAt: number
  decidedAt: number | null
}

export interface LibrarianRunDetail {
  id: string
  status: 'running' | 'ready' | 'failed'
  knowledgeCount: number
  summary: string | null
  error: string | null
  createdAt: number
  updatedAt: number
  proposals: LibrarianProposalRecord[]
}
