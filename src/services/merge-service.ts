import { invoke } from '@tauri-apps/api/core'
import type { DuplicateMergePreview, DuplicateMergeRecord } from './merge-types'

export function previewDuplicateMerge(candidateId: string) {
  return invoke<DuplicateMergePreview>('duplicate_merge_preview', { candidateId })
}

export function applyDuplicateMerge(
  candidateId: string,
  targetKnowledgeId: string,
  expectedLeftUpdatedAt: number,
  expectedRightUpdatedAt: number
) {
  return invoke<DuplicateMergeRecord>('duplicate_merge_apply', {
    candidateId,
    targetKnowledgeId,
    expectedLeftUpdatedAt,
    expectedRightUpdatedAt
  })
}

export function revertDuplicateMerge(mergeId: string) {
  return invoke<DuplicateMergeRecord>('duplicate_merge_revert', { mergeId })
}
