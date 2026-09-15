import { invoke } from '@tauri-apps/api/core'
import type { CurationCandidateRecord, CurationRunDetail } from './curation-types'

export function analyzeKnowledgeCuration() {
  return invoke<CurationRunDetail>('curation_analyze')
}

export function getLatestCurationRun() {
  return invoke<CurationRunDetail | null>('curation_latest')
}

export function applyCurationCandidate(candidateId: string) {
  return invoke<CurationCandidateRecord>('curation_candidate_apply', { candidateId })
}

export function dismissCurationCandidate(candidateId: string) {
  return invoke<CurationCandidateRecord>('curation_candidate_dismiss', { candidateId })
}
