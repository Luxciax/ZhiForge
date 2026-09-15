import { invoke } from '@tauri-apps/api/core'
import type {
  AiTaskRecord,
  InboxAcceptResult,
  InboxDetailRecord,
  InboxItemRecord,
  JudgedAttemptResult,
  KnowledgeAttemptRecord,
  KnowledgeCaptureResult,
  KnowledgeDetail,
  KnowledgeDraftClassification,
  KnowledgeDraftRecord,
  KnowledgeGraphSnapshot,
  KnowledgeJobStart,
  KnowledgeLibraryItem,
  KnowledgeListItem,
  KnowledgeManagementDetail,
  KnowledgeRelationRecord,
  KnowledgeRelationType,
  KnowledgeQualityStatus,
  KnowledgeStatus,
  SourceCaptureInput,
  SourceDetailRecord,
  SourceLibraryItem,
  SourcePlatform,
  ResearchGapRecord,
  ResearchGapStatus,
  ResearchPlanDetail,
  ResearchPlanSummary,
  TagRecord,
  TopicRecord,
  TrashKnowledgeItem
} from '../types'

export interface KnowledgeLibraryFilters {
  query?: string
  status?: KnowledgeStatus
  qualityStatus?: KnowledgeQualityStatus
  topicId?: string
  tagId?: string
  includeArchived?: boolean
  limit?: number
}

export interface SemanticSearchHit {
  knowledgeUnitId: string
  coreClaim: string
  selectedText: string
  status: KnowledgeStatus
  qualityStatus: KnowledgeQualityStatus
  platform: SourcePlatform
  title: string | null
  author: string | null
  masteryScore: number
  reviewCount: number
  topicNames: string[]
  tagNames: string[]
  score: number
  reason: string
}

export interface SemanticSearchResult {
  query: string
  usedExpansion: boolean
  expandedTerms: string[]
  candidateCount: number
  hits: SemanticSearchHit[]
}

export interface ReviewQueueItem {
  knowledgeUnitId: string
  questionId: string | null
  coreClaim: string
  selectedText: string
  status: KnowledgeStatus
  platform: SourcePlatform
  title: string | null
  author: string | null
  masteryScore: number
  wrongCount: number
  reviewCount: number
  nextReviewAt: number
  stability: number
  difficulty: number
  lapseCount: number
  scheduledDays: number
}

export interface TodayInboxItem {
  inboxId: string
  title: string
  status: 'captured' | 'ready' | 'failed' | 'processing'
  processingStage: string
  draftCount: number
  error: string | null
  updatedAt: number
}

export interface TodayResearchGapItem {
  gapId: string
  planId: string
  topic: string
  title: string
  priority: 'high' | 'medium' | 'low'
  status: 'open' | 'collecting'
  searchQuery: string
  sourceCount: number
  updatedAt: number
}

export interface TodayKnowledgeSignal {
  knowledgeUnitId: string
  title: string
  signal: 'weak' | 'conflicted' | 'needs_expansion' | 'stale' | 'other'
  masteryScore: number
  updatedAt: number
}

export interface TodayOverview {
  generatedAt: number
  inboxCapturedCount: number
  inboxReadyCount: number
  inboxFailedCount: number
  inboxProcessingCount: number
  inboxItems: TodayInboxItem[]
  aiActiveCount: number
  aiFailedCount: number
  initialLearningCount: number
  initialLearningItems: ReviewQueueItem[]
  dueReviewCount: number
  activeReviewRemainingCount: number
  reviewItems: ReviewQueueItem[]
  activeResearchPlanCount: number
  openResearchGapCount: number
  collectingResearchGapCount: number
  researchGaps: TodayResearchGapItem[]
  weakCount: number
  conflictedCount: number
  needsExpansionCount: number
  staleCount: number
  maintenanceCount: number
  knowledgeSignals: TodayKnowledgeSignal[]
  librarianPendingCount: number
  curationPendingCount: number
}

export type ReviewSessionStatus = 'active' | 'completed' | 'abandoned'
export type ReviewSessionItemStatus = 'pending' | 'completed' | 'skipped'

export interface ReviewSessionItem {
  position: number
  knowledgeUnitId: string
  questionId: string
  status: ReviewSessionItemStatus
  completedAt: number | null
  coreClaim: string
  selectedText: string
  platform: SourcePlatform
  title: string | null
  author: string | null
  masteryScore: number
  wrongCount: number
  reviewCount: number
  nextReviewAt: number
}

export interface ReviewSessionDetail {
  id: string
  status: ReviewSessionStatus
  dueBefore: number
  itemCount: number
  completedCount: number
  skippedCount: number
  startedAt: number
  updatedAt: number
  endedAt: number | null
  items: ReviewSessionItem[]
}

export interface ReviewSessionSummary {
  id: string
  status: Exclude<ReviewSessionStatus, 'active'>
  itemCount: number
  completedCount: number
  skippedCount: number
  startedAt: number
  endedAt: number | null
}

export interface ReviewPlanItem {
  knowledgeUnitId: string
  questionId: string
  coreClaim: string
  selectedText: string
  status: KnowledgeStatus
  platform: SourcePlatform
  title: string | null
  author: string | null
  masteryScore: number
  wrongCount: number
  reviewCount: number
  nextReviewAt: number
  stability: number
  difficulty: number
  lapseCount: number
  scheduledDays: number
  reason: string
}

export interface ReviewPlanPreview {
  queueRevision: string
  dueBefore: number
  totalDue: number
  analyzedCount: number
  selectedCount: number
  deferredCount: number
  generatedAt: number
  summary: string
  selected: ReviewPlanItem[]
  deferred: ReviewPlanItem[]
}

export interface KnowledgeHealthSample {
  knowledgeUnitId: string
  title: string
  status: KnowledgeStatus
  otherKnowledgeUnitId: string | null
  otherTitle: string | null
}

export interface KnowledgeHealthBucket {
  count: number
  samples: KnowledgeHealthSample[]
}

export interface KnowledgeHealthReport {
  totalKnowledge: number
  generatedAt: number
  uncategorized: KnowledgeHealthBucket
  noQuestions: KnowledgeHealthBucket
  neverReviewed: KnowledgeHealthBucket
  possibleDuplicates: KnowledgeHealthBucket
  possibleConflicts: KnowledgeHealthBucket
  sourceLinkIssues: KnowledgeHealthBucket
  staleKnowledge: KnowledgeHealthBucket
}

export function captureKnowledge(input: SourceCaptureInput) {
  return invoke<KnowledgeCaptureResult>('knowledge_capture', { input })
}

export function listKnowledge(limit = 20) {
  return invoke<KnowledgeListItem[]>('knowledge_list', { limit })
}

export function listKnowledgeInbox(limit = 100) {
  return invoke<InboxItemRecord[]>('knowledge_inbox_list', { limit })
}

export function getKnowledgeInbox(inboxId: string) {
  return invoke<InboxDetailRecord>('knowledge_inbox_get', { inboxId })
}

export interface InboxDraftUpdateInput {
  coreClaim: string
  concepts: string[]
  prerequisites: string[]
  importantDetails: string[]
  limitations: string[]
  classification: KnowledgeDraftClassification
  relatedKnowledgeId?: string | null
}

export function updateKnowledgeInboxDraft(draftId: string, input: InboxDraftUpdateInput) {
  return invoke<KnowledgeDraftRecord>('knowledge_inbox_draft_update', {
    draftId,
    coreClaim: input.coreClaim,
    concepts: input.concepts,
    prerequisites: input.prerequisites,
    importantDetails: input.importantDetails,
    limitations: input.limitations,
    classification: input.classification,
    relatedKnowledgeId: input.relatedKnowledgeId || undefined
  })
}

export function acceptKnowledgeInbox(inboxId: string, draftIds: string[]) {
  return invoke<InboxAcceptResult>('knowledge_inbox_accept', { inboxId, draftIds })
}

export function ignoreKnowledgeInbox(inboxId: string) {
  return invoke<void>('knowledge_inbox_ignore', { inboxId })
}

export function restoreKnowledgeInbox(inboxId: string) {
  return invoke<InboxItemRecord>('knowledge_inbox_restore', { inboxId })
}

export function getKnowledge(knowledgeUnitId: string) {
  return invoke<KnowledgeCaptureResult>('knowledge_get', { knowledgeUnitId })
}

export function getKnowledgeDetail(knowledgeUnitId: string) {
  return invoke<KnowledgeDetail>('knowledge_get_detail', { knowledgeUnitId })
}

export function listSources(query?: string, platform?: SourcePlatform, limit = 100) {
  return invoke<SourceLibraryItem[]>('source_library_list', { query, platform, limit })
}

export function getSourceDetail(sourceId: string) {
  return invoke<SourceDetailRecord>('source_get_detail', { sourceId })
}

export function getKnowledgeHealth() {
  return invoke<KnowledgeHealthReport>('knowledge_health')
}

export function getKnowledgeGraph(centerKnowledgeId?: string | null, depth = 2, limit = 48) {
  return invoke<KnowledgeGraphSnapshot>('knowledge_graph', {
    centerKnowledgeId: centerKnowledgeId || undefined,
    depth,
    limit
  })
}

export function listKnowledgeLibrary(filters: KnowledgeLibraryFilters = {}) {
  return invoke<KnowledgeLibraryItem[]>('knowledge_library_list', {
    query: filters.query,
    status: filters.status,
    qualityStatus: filters.qualityStatus,
    topicId: filters.topicId,
    tagId: filters.tagId,
    includeArchived: filters.includeArchived,
    limit: filters.limit ?? 100
  })
}

export function expandKnowledgeQuery(query: string) {
  return invoke<string[]>('knowledge_expand_query', { query })
}

export function semanticSearchKnowledge(query: string, filters: Omit<KnowledgeLibraryFilters, 'query'> = {}) {
  return invoke<SemanticSearchResult>('knowledge_semantic_search', {
    query,
    status: filters.status,
    qualityStatus: filters.qualityStatus,
    topicId: filters.topicId,
    tagId: filters.tagId,
    includeArchived: filters.includeArchived,
    limit: filters.limit ?? 12
  })
}

export function getKnowledgeManagementDetail(knowledgeUnitId: string) {
  return invoke<KnowledgeManagementDetail>('knowledge_management_detail', { knowledgeUnitId })
}

export function updateKnowledgeNote(knowledgeUnitId: string, note: string) {
  return invoke<string | null>('knowledge_note_update', { knowledgeUnitId, note })
}

export function createTopic(name: string, description?: string) {
  return invoke<TopicRecord>('topic_create', { name, description })
}

export function listTopics(includeArchived = false) {
  return invoke<TopicRecord[]>('topic_list', { includeArchived })
}

export function updateTopic(topicId: string, name: string, description?: string) {
  return invoke<TopicRecord>('topic_update', { topicId, name, description })
}

export function setTopicArchived(topicId: string, archived: boolean) {
  return invoke<TopicRecord>('topic_set_archived', { topicId, archived })
}

export function assignTopic(knowledgeUnitId: string, topicId: string) {
  return invoke<void>('topic_assign', { knowledgeUnitId, topicId })
}

export function unassignTopic(knowledgeUnitId: string, topicId: string) {
  return invoke<void>('topic_unassign', { knowledgeUnitId, topicId })
}

export function addKnowledgeTag(knowledgeUnitId: string, name: string) {
  return invoke<TagRecord>('tag_add', { knowledgeUnitId, name })
}

export function removeKnowledgeTag(knowledgeUnitId: string, tagId: string) {
  return invoke<void>('tag_remove', { knowledgeUnitId, tagId })
}

export function listTags() {
  return invoke<TagRecord[]>('tag_list')
}

export function updateTag(tagId: string, name: string) {
  return invoke<TagRecord>('tag_update', { tagId, name })
}

export function deleteEmptyTag(tagId: string) {
  return invoke<void>('tag_delete_empty', { tagId })
}

export function createKnowledgeRelation(
  sourceKnowledgeId: string,
  targetKnowledgeId: string,
  relationType: KnowledgeRelationType
) {
  return invoke<KnowledgeRelationRecord>('knowledge_relation_create', {
    sourceKnowledgeId,
    targetKnowledgeId,
    relationType
  })
}

export function listKnowledgeRelations(knowledgeUnitId: string) {
  return invoke<KnowledgeRelationRecord[]>('knowledge_relation_list', { knowledgeUnitId })
}

export function removeKnowledgeRelation(relationId: string) {
  return invoke<void>('knowledge_relation_remove', { relationId })
}

export function archiveKnowledge(knowledgeUnitId: string, archived: boolean) {
  return invoke<void>('knowledge_archive', { knowledgeUnitId, archived })
}

export function trashKnowledge(knowledgeUnitId: string) {
  return invoke<void>('knowledge_trash', { knowledgeUnitId })
}

export function restoreKnowledge(knowledgeUnitId: string) {
  return invoke<void>('knowledge_restore', { knowledgeUnitId })
}

export function listKnowledgeTrash() {
  return invoke<TrashKnowledgeItem[]>('knowledge_trash_list')
}

export function internalizeKnowledge(knowledgeUnitId: string) {
  return invoke<KnowledgeJobStart>('knowledge_internalize', { knowledgeUnitId })
}

export function generateKnowledgeQuestions(knowledgeUnitId: string) {
  return invoke<KnowledgeJobStart>('knowledge_generate_questions', { knowledgeUnitId })
}

export function createKnowledgeAttempt(questionId: string, answer: string) {
  return invoke<KnowledgeAttemptRecord>('knowledge_attempt_create', { questionId, answer })
}

export function listKnowledgeAttempts(knowledgeUnitId: string) {
  return invoke<KnowledgeAttemptRecord[]>('knowledge_attempt_list', { knowledgeUnitId })
}

export function judgeKnowledgeAttempt(attemptId: string) {
  return invoke<JudgedAttemptResult>('knowledge_attempt_judge', { attemptId })
}

export function listKnowledgeReviewQueue(dueBefore: number, limit = 20) {
  return invoke<ReviewQueueItem[]>('knowledge_review_queue', { dueBefore, limit })
}

export function startReviewSession(dueBefore: number, limit = 20) {
  return invoke<ReviewSessionDetail>('knowledge_review_session_start', { dueBefore, limit })
}

export function getCurrentReviewSession() {
  return invoke<ReviewSessionDetail | null>('knowledge_review_session_current')
}

export function abandonReviewSession(sessionId: string) {
  return invoke<ReviewSessionDetail>('knowledge_review_session_abandon', { sessionId })
}

export function listReviewSessionHistory(limit = 7) {
  return invoke<ReviewSessionSummary[]>('knowledge_review_session_history', { limit })
}

export function previewReviewPlan(dueBefore: number, sessionLimit = 20) {
  return invoke<ReviewPlanPreview>('knowledge_review_plan_preview', { dueBefore, sessionLimit })
}

export function applyReviewPlan(plan: ReviewPlanPreview) {
  return invoke<ReviewSessionDetail>('knowledge_review_plan_apply', {
    dueBefore: plan.dueBefore,
    queueRevision: plan.queueRevision,
    selectedKnowledgeIds: plan.selected.map((item) => item.knowledgeUnitId)
  })
}

export function getTodayOverview() {
  return invoke<TodayOverview>('knowledge_today')
}

export function listKnowledgeTasks(limit = 40) {
  return invoke<AiTaskRecord[]>('knowledge_task_list', { limit })
}

export function analyzeResearchPlan(topic: string) {
  return invoke<ResearchPlanDetail>('research_plan_analyze', { topic })
}

export function getLatestResearchPlan(topic?: string) {
  return invoke<ResearchPlanDetail | null>('research_plan_latest', { topic })
}

export function getResearchPlan(planId: string) {
  return invoke<ResearchPlanDetail>('research_plan_get', { planId })
}

export function listResearchPlans(limit = 20) {
  return invoke<ResearchPlanSummary[]>('research_plan_list', { limit })
}

export function listResearchGapSources(gapId: string) {
  return invoke<SourceLibraryItem[]>('research_gap_source_list', { gapId })
}

export function setResearchGapStatus(gapId: string, status: ResearchGapStatus) {
  return invoke<ResearchGapRecord>('research_gap_set_status', { gapId, status })
}

export function linkResearchGapSource(gapId: string, sourceId: string) {
  return invoke<ResearchGapRecord>('research_gap_link_source', { gapId, sourceId })
}

export function openSourceUrl(url: string) {
  return invoke<void>('open_source_url', { url })
}
