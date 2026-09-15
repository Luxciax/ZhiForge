export type SelectionMethod = 'uia' | 'accessibility' | 'clipboard'
export type ActionKind = string
export type ProviderAdapter = 'openai-compatible' | 'openai-responses' | 'anthropic' | 'gemini'
export type OutputMode = 'markdown' | 'plain-text'

export interface SelectionPayload {
  text: string
  programName: string
  method: SelectionMethod
  mouseX: number
  mouseY: number
}

export interface ActionRequest {
  action: ActionKind
  actionName?: string
  outputMode?: OutputMode
  selection: SelectionPayload
}

export interface ProviderConfig {
  adapter: ProviderAdapter
  apiBase: string
  apiKey: string
  model: string
  headers: Record<string, string>
}

export interface AiSettings extends ProviderConfig {
  providerId?: string
  credentialId?: string
  targetLanguage: string
  alternateLanguage: string
}

export interface AiActionRunRequest {
  requestId: string
  action: ActionKind
  text: string
  targetLanguage: string
  alternateLanguage: string
  provider: ProviderConfig
}

export interface RoutedAiActionRunRequest {
  requestId: string
  action: ActionKind
  text: string
}

export interface AiStartedEvent {
  requestId: string
  providerId: string
  providerName: string
  adapter: ProviderAdapter
  model: string
  attempt: number
}

export interface AiChunkEvent {
  requestId: string
  chunk: string
}

export interface AiDoneEvent {
  requestId: string
}

export type AiErrorKind =
  | 'auth'
  | 'rate-limit'
  | 'timeout'
  | 'network'
  | 'provider'
  | 'malformed-stream'
  | 'empty-response'
  | 'interrupted'
  | 'config'
  | 'all-providers-failed'

export interface ModelCapabilities {
  systemPrompt: boolean
  temperature: boolean
  topP: boolean
  maxOutputTokens: boolean
  reasoning: boolean
}

export interface AiErrorEvent {
  requestId: string
  kind: AiErrorKind
  message: string
  status?: number
  retryable: boolean
  partial: boolean
}

export type SourcePlatform = 'zhihu' | 'web' | 'pdf' | 'desktop' | 'unknown'

export interface SourceCaptureInput {
  platform: SourcePlatform
  selectedText: string
  url?: string
  title?: string
  author?: string
  contextBefore?: string
  contextAfter?: string
  application?: string
  windowTitle?: string
}

export interface SourceRecord extends SourceCaptureInput {
  id: string
  contentHash: string
  capturedAt: number
}

export type KnowledgeStatus = 'captured' | 'processed' | 'learning' | 'reviewing' | 'weak' | 'mastered'
export type KnowledgeQualityStatus = 'unverified' | 'verified' | 'conflicted' | 'stale' | 'needs_expansion'

export interface KnowledgeUnitRecord {
  id: string
  sourceId: string
  coreClaim: string
  concepts: string[]
  prerequisites: string[]
  importantDetails: string[]
  limitations: string[]
  userNote?: string
  status: KnowledgeStatus
  createdAt: number
  updatedAt: number
}

export interface KnowledgeCaptureResult {
  source: SourceRecord
  knowledgeUnit: KnowledgeUnitRecord
  duplicate: boolean
}

export type AiJobStatus = 'pending' | 'running' | 'completed' | 'failed'
export type AiTaskType = 'internalize' | 'question_generation' | 'librarian' | 'curation'
export interface AiTaskRecord {
  id: string
  taskType: AiTaskType
  status: AiJobStatus
  title: string
  knowledgeUnitId: string | null
  inboxId: string | null
  error: string | null
  createdAt: number
  updatedAt: number
  retryable: boolean
}
export type KnowledgeQuestionType = 'recall' | 'explain' | 'apply'
export type InboxStatus = 'captured' | 'processing' | 'ready' | 'failed' | 'accepted' | 'ignored'
export type KnowledgeDraftDecision = 'pending' | 'accepted' | 'ignored'
export type KnowledgeDraftClassification = 'new' | 'duplicate' | 'supplement' | 'conflict'

export interface InboxItemRecord {
  id: string
  sourceId: string
  anchorKnowledgeId: string
  status: InboxStatus
  processingStage: string
  progressCurrent: number
  progressTotal: number
  error: string | null
  platform: SourcePlatform
  url: string | null
  title: string | null
  author: string | null
  selectedText: string
  capturedAt: number
  draftCount: number
  createdAt: number
  updatedAt: number
}

export interface KnowledgeDraftRecord {
  id: string
  inboxId: string
  sourceId: string
  position: number
  coreClaim: string
  concepts: string[]
  prerequisites: string[]
  importantDetails: string[]
  limitations: string[]
  evidence: string[]
  decision: KnowledgeDraftDecision
  classification: KnowledgeDraftClassification
  relatedKnowledgeId: string | null
  relatedCoreClaim: string | null
  acceptedKnowledgeId: string | null
  acceptedCoreClaim: string | null
  relationType: string | null
  rationale: string | null
  confidence: number | null
  createdAt: number
  updatedAt: number
}

export interface InboxDetailRecord {
  item: InboxItemRecord
  drafts: KnowledgeDraftRecord[]
}

export interface InboxAcceptResult {
  inboxId: string
  knowledgeUnitIds: string[]
  updatedKnowledgeUnitIds: string[]
}

export type ResearchGapPriority = 'high' | 'medium' | 'low'
export type ResearchGapStatus = 'open' | 'collecting' | 'covered' | 'dismissed'
export type ResearchPlanStatus = 'active' | 'completed' | 'archived'

export interface ResearchKnowledgeRecord {
  id: string
  coreClaim: string
  title: string | null
  author: string | null
  masteryScore: number
  qualityStatus: KnowledgeQualityStatus
}

export interface ResearchGapRecord {
  id: string
  planId: string
  position: number
  title: string
  rationale: string
  priority: ResearchGapPriority
  searchQuery: string
  status: ResearchGapStatus
  relatedKnowledgeIds: string[]
  sourceIds: string[]
  createdAt: number
  updatedAt: number
}

export interface ResearchPlanDetail {
  id: string
  topic: string
  summary: string
  coverageScore: number
  status: ResearchPlanStatus
  sourceKnowledgeIds: string[]
  knowledge: ResearchKnowledgeRecord[]
  gaps: ResearchGapRecord[]
  createdAt: number
  updatedAt: number
}

export interface ResearchPlanSummary {
  id: string
  topic: string
  summary: string
  coverageScore: number
  status: ResearchPlanStatus
  openCount: number
  collectingCount: number
  coveredCount: number
  dismissedCount: number
  createdAt: number
  updatedAt: number
}

export interface KnowledgeListItem {
  id: string
  sourceId: string
  coreClaim: string
  selectedText: string
  status: KnowledgeStatus
  platform: SourcePlatform
  title?: string
  author?: string
  capturedAt: number
  masteryScore: number
  reviewCount: number
  aiJobStatus: AiJobStatus | null
  aiJobError: string | null
}

export interface KnowledgeJobStart {
  jobId: string
  knowledgeUnitId: string
  status: AiJobStatus
  reused: boolean
}

export interface EvidenceRecord {
  id: string
  knowledgeUnitId: string
  sourceId: string
  text: string
  startOffset: number | null
  endOffset: number | null
}

export interface KnowledgeQualityState {
  knowledgeUnitId: string
  status: KnowledgeQualityStatus
  reason: string | null
  updatedBy: string
  updatedAt: number
}

export interface ClaimEvidenceRecord {
  evidenceId: string
  sourceId: string
  text: string
  startOffset: number | null
  endOffset: number | null
  stance: 'supports' | 'conflicts'
  confidence: number | null
  createdBy: string
  sourcePlatform: SourcePlatform
  sourceUrl: string | null
  sourceTitle: string | null
  sourceAuthor: string | null
}

export interface KnowledgeClaimRecord {
  id: string
  knowledgeUnitId: string
  text: string
  claimType: 'primary' | 'supporting'
  createdBy: string
  createdAt: number
  updatedAt: number
  evidence: ClaimEvidenceRecord[]
}

export interface KnowledgeSourceLinkRecord {
  sourceId: string
  role: 'origin' | 'supporting' | 'conflicting'
  platform: SourcePlatform
  url: string | null
  title: string | null
  author: string | null
  capturedAt: number
}

export interface KnowledgeQuestionRecord {
  id: string
  knowledgeUnitId: string
  questionType: KnowledgeQuestionType
  question: string
  referencePoints: string[]
  evidenceIds: string[]
  difficulty: number
  createdAt: number
}

export interface AiJobRecord {
  id: string
  knowledgeUnitId: string
  jobType: string
  status: AiJobStatus
  error: string | null
  createdAt: number
  updatedAt: number
}

export interface SourceKnowledgeUnitSummary {
  id: string
  coreClaim: string
  status: KnowledgeStatus
  masteryScore: number
  reviewCount: number
  archivedAt: number | null
}

export interface SourceLibraryItem {
  id: string
  platform: SourcePlatform
  url: string | null
  title: string | null
  author: string | null
  selectedText: string
  capturedAt: number
  knowledgeCount: number
}

export interface SourceDetailRecord {
  source: SourceRecord
  knowledgeUnits: SourceKnowledgeUnitSummary[]
}

export interface KnowledgeDetail {
  source: SourceRecord
  knowledgeUnit: KnowledgeUnitRecord
  evidence: EvidenceRecord[]
  questions: KnowledgeQuestionRecord[]
  aiJob: AiJobRecord | null
  reviewState: { knowledgeUnitId: string; masteryScore: number; correctCount: number; wrongCount: number; reviewCount: number; lastReviewedAt: number | null; nextReviewAt: number | null } | null
  qualityState: KnowledgeQualityState
  claims: KnowledgeClaimRecord[]
  sources: KnowledgeSourceLinkRecord[]
  sourceUnits: SourceKnowledgeUnitSummary[]
}

export interface ZhihuSearchItem {
  title: string
  contentType: string
  contentId: string
  contentText: string
  url: string
  commentCount: number
  voteUpCount: number
  authorName: string
  authorAvatar: string
  authorBadgeText: string
  editTime: number
  authorityLevel: string
  rankingScore: number
}

export interface ZhihuSearchResult {
  hasMore: boolean
  searchHashId: string
  items: ZhihuSearchItem[]
  emptyReason: string
}

export interface TopicRecord {
  id: string
  name: string
  description: string
  createdBy: string
  locked: boolean
  knowledgeCount: number
  createdAt: number
  updatedAt: number
  archivedAt: number | null
}

export interface TagRecord {
  id: string
  name: string
  createdBy: string
  knowledgeCount: number
  createdAt: number
  updatedAt: number
}

export type KnowledgeRelationType =
  | 'related_to'
  | 'supports'
  | 'contradicts'
  | 'example_of'
  | 'prerequisite_of'
  | 'derived_from'
  | 'extends'

export interface KnowledgeRelationRecord {
  id: string
  sourceKnowledgeId: string
  targetKnowledgeId: string
  relationType: KnowledgeRelationType
  confidence: number | null
  createdBy: string
  confirmed: boolean
  createdAt: number
  otherKnowledgeId: string
  otherTitle: string
  direction: 'incoming' | 'outgoing'
}

export interface KnowledgeManagementDetail {
  topics: TopicRecord[]
  tags: TagRecord[]
  relations: KnowledgeRelationRecord[]
  archivedAt: number | null
}

export type KnowledgeGraphEdgeKind = 'relation' | 'source' | 'topic' | 'tag'

export interface KnowledgeGraphNode {
  id: string
  title: string
  status: KnowledgeStatus
  qualityStatus: KnowledgeQualityStatus
  platform: SourcePlatform
  author: string | null
  masteryScore: number
  reviewCount: number
  topicNames: string[]
  tagNames: string[]
  relationCount: number
  sourceCount: number
  updatedAt: number
  isCenter: boolean
}

export interface KnowledgeGraphEdge {
  id: string
  sourceKnowledgeId: string
  targetKnowledgeId: string
  kind: KnowledgeGraphEdgeKind
  relationType: KnowledgeRelationType | null
  label: string
  reason: string
  weight: number
  confidence: number | null
  createdBy: string | null
  confirmed: boolean
}

export interface KnowledgeGraphSnapshot {
  centerKnowledgeId: string | null
  depth: number
  totalActive: number
  truncated: boolean
  generatedAt: number
  nodes: KnowledgeGraphNode[]
  edges: KnowledgeGraphEdge[]
}

export interface KnowledgeLibraryItem {
  id: string
  sourceId: string
  coreClaim: string
  selectedText: string
  userNote: string | null
  status: KnowledgeStatus
  qualityStatus: KnowledgeQualityStatus
  platform: SourcePlatform
  title: string | null
  author: string | null
  capturedAt: number
  updatedAt: number
  masteryScore: number
  reviewCount: number
  archivedAt: number | null
  topics: TopicRecord[]
  tags: TagRecord[]
}

export interface TrashKnowledgeItem {
  knowledgeUnitId: string
  coreClaim: string
  selectedText: string
  platform: SourcePlatform
  title: string | null
  deletedAt: number
  expiresAt: number
}

export interface BackupInfo {
  fileName: string
  path: string
  kind: 'automatic' | 'manual' | 'pre-restore'
  createdAt: number
  sizeBytes: number
}

export interface DatabaseSafetyStatus {
  databasePath: string
  backupDirectory: string
  exportDirectory: string
  backups: BackupInfo[]
}

export interface KnowledgeExportInfo {
  format: 'json' | 'markdown'
  path: string
  createdAt: number
  knowledgeCount: number
  sizeBytes: number
}

export interface AuditRecord {
  id: string
  action: string
  entityType: string
  entityId: string | null
  detail: Record<string, unknown> | null
  createdAt: number
}

export type AttemptResult = 'correct' | 'partial' | 'wrong'

export interface KnowledgeAttemptRecord {
  id: string
  questionId: string
  answer: string
  score: number | null
  result: AttemptResult | null
  correctPoints: string[]
  missingPoints: string[]
  wrongPoints: string[]
  feedback: string | null
  evidenceIds: string[]
  createdAt: number
}

export interface JudgedAttemptResult {
  attempt: KnowledgeAttemptRecord
  masteryScore: number
}
