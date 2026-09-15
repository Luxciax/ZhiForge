import { invoke } from '@tauri-apps/api/core'
import type { LibrarianProposalRecord, LibrarianRunDetail } from './librarian-types'

export function analyzeKnowledgeLibrary() {
  return invoke<LibrarianRunDetail>('librarian_analyze')
}

export function getLatestLibrarianRun() {
  return invoke<LibrarianRunDetail | null>('librarian_latest')
}

export function applyLibrarianProposal(proposalId: string) {
  return invoke<LibrarianProposalRecord>('librarian_proposal_apply', { proposalId })
}

export function dismissLibrarianProposal(proposalId: string) {
  return invoke<LibrarianProposalRecord>('librarian_proposal_dismiss', { proposalId })
}
