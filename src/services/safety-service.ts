import { invoke } from '@tauri-apps/api/core'
import type { AuditRecord, BackupInfo, DatabaseSafetyStatus, KnowledgeExportInfo } from '../types'

export function getDatabaseSafetyStatus() {
  return invoke<DatabaseSafetyStatus>('database_safety_status')
}

export function createDatabaseBackup() {
  return invoke<BackupInfo>('database_backup_create')
}

export function listDatabaseBackups() {
  return invoke<BackupInfo[]>('database_backup_list')
}

export function restoreDatabaseBackup(fileName: string) {
  return invoke<BackupInfo>('database_backup_restore', { fileName })
}

export function exportKnowledge(format: 'json' | 'markdown') {
  return invoke<KnowledgeExportInfo>('knowledge_export', { format })
}

export function listAuditLog(limit = 80) {
  return invoke<AuditRecord[]>('audit_log_list', { limit })
}
