export interface MergeSideSummary {
  id: string
  title: string
  sourceCount: number
  evidenceCount: number
  topicCount: number
  tagCount: number
  relationCount: number
  reviewCount: number
  masteryScore: number
  updatedAt: number
  archivedAt: number | null
}

export interface DuplicateMergeRecord {
  id: string
  curationCandidateId: string
  targetKnowledgeId: string
  sourceKnowledgeId: string
  status: 'applied' | 'reverted'
  createdAt: number
  revertedAt: number | null
}

export interface DuplicateMergePreview {
  candidateId: string
  left: MergeSideSummary
  right: MergeSideSummary
  recommendedTargetId: string
  recommendationReason: string
  existingMerge: DuplicateMergeRecord | null
}
