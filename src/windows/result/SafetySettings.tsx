import { invoke } from '@tauri-apps/api/core'
import { ArchiveRestore, Check, Copy, DatabaseBackup, Download, FileJson, FileText, History, LoaderCircle, ShieldCheck } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import {
  createDatabaseBackup,
  exportKnowledge,
  getDatabaseSafetyStatus,
  listAuditLog,
  restoreDatabaseBackup
} from '../../services/safety-service'
import type { AuditRecord, BackupInfo, DatabaseSafetyStatus, KnowledgeExportInfo } from '../../types'

const ACTION_LABELS: Record<string, string> = {
  'backup.create': '创建手动备份',
  'backup.restore': '恢复数据库',
  'knowledge.export': '导出知识库',
  'knowledge.note.update': '更新我的理解',
  'topic.create': '创建主题',
  'knowledge.topic.assign': '加入主题',
  'knowledge.topic.unassign': '移出主题',
  'knowledge.tag.add': '添加标签',
  'knowledge.tag.remove': '移除标签',
  'knowledge.relation.create': '建立关系',
  'knowledge.relation.remove': '移除关系',
  'knowledge.archive': '归档知识',
  'knowledge.unarchive': '取消归档',
  'knowledge.trash': '移到回收站',
  'knowledge.restore': '从回收站恢复',
  'librarian.run.ready': '生成整理建议',
  'librarian.run.failed': '知识整理失败',
  'librarian.proposal.apply': '应用整理建议',
  'librarian.proposal.dismiss': '忽略整理建议'
}

const BACKUP_KIND_LABELS: Record<BackupInfo['kind'], string> = {
  automatic: '自动',
  manual: '手动',
  'pre-restore': '恢复前'
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / 1024 / 1024).toFixed(1)} MB`
}

function formatTime(value: number) {
  return new Date(value).toLocaleString()
}

export function SafetySettings() {
  const [status, setStatus] = useState<DatabaseSafetyStatus | null>(null)
  const [audits, setAudits] = useState<AuditRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<string | null>(null)
  const [restoreConfirm, setRestoreConfirm] = useState<string | null>(null)
  const [copied, setCopied] = useState<string | null>(null)
  const [lastExport, setLastExport] = useState<KnowledgeExportInfo | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [nextStatus, nextAudits] = await Promise.all([
        getDatabaseSafetyStatus(),
        listAuditLog(80)
      ])
      setStatus(nextStatus)
      setAudits(nextAudits)
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const copyPath = async (key: string, value: string) => {
    try {
      await invoke('write_to_clipboard', { text: value })
      setCopied(key)
      window.setTimeout(() => setCopied(null), 1300)
    } catch (copyError) {
      setError(String(copyError))
    }
  }

  const createBackup = async () => {
    if (busy) return
    setBusy('backup')
    setNotice(null)
    setError(null)
    try {
      const backup = await createDatabaseBackup()
      setNotice(`已创建手动备份：${backup.fileName}`)
      await refresh()
    } catch (backupError) {
      setError(String(backupError))
    } finally {
      setBusy(null)
    }
  }

  const runExport = async (format: 'json' | 'markdown') => {
    if (busy) return
    setBusy(`export-${format}`)
    setNotice(null)
    setError(null)
    try {
      const result = await exportKnowledge(format)
      setLastExport(result)
      setNotice(`已导出 ${result.knowledgeCount} 条知识。`)
      await refresh()
    } catch (exportError) {
      setError(String(exportError))
    } finally {
      setBusy(null)
    }
  }

  const restoreBackup = async (backup: BackupInfo) => {
    if (busy || restoreConfirm !== backup.fileName) {
      setRestoreConfirm(backup.fileName)
      return
    }
    setBusy(`restore-${backup.fileName}`)
    setNotice(null)
    setError(null)
    try {
      const recovery = await restoreDatabaseBackup(backup.fileName)
      setRestoreConfirm(null)
      setNotice(`恢复完成。操作前状态已保存为 ${recovery.fileName}。`)
      await refresh()
    } catch (restoreError) {
      setError(String(restoreError))
    } finally {
      setBusy(null)
    }
  }

  if (loading && !status) {
    return <div className="safety-settings-loading"><LoaderCircle size={16} strokeWidth={1.8} className="spin" />正在读取本地数据状态…</div>
  }

  return (
    <div className="safety-settings-editor">
      <section className="safety-settings-card safety-overview-card">
        <div className="safety-card-heading">
          <span className="safety-card-icon"><ShieldCheck size={17} strokeWidth={1.8} /></span>
          <div>
            <strong>本地数据安全</strong>
            <span>启动时最多每 24 小时创建一次自动快照；不会自动删除旧备份。</span>
          </div>
        </div>
        {status && (
          <div className="safety-path-list">
            <button type="button" onClick={() => void copyPath('db', status.databasePath)}>
              <span><b>数据库</b><small>{status.databasePath}</small></span>
              {copied === 'db' ? <Check size={13} /> : <Copy size={13} />}
            </button>
            <button type="button" onClick={() => void copyPath('backup', status.backupDirectory)}>
              <span><b>备份目录</b><small>{status.backupDirectory}</small></span>
              {copied === 'backup' ? <Check size={13} /> : <Copy size={13} />}
            </button>
          </div>
        )}
      </section>

      <section className="safety-settings-card">
        <div className="safety-card-heading safety-heading-actions">
          <span className="safety-card-icon"><DatabaseBackup size={17} strokeWidth={1.8} /></span>
          <div>
            <strong>数据库备份</strong>
            <span>使用 SQLite Online Backup 创建一致快照。恢复前会先自动保存当前状态。</span>
          </div>
          <button type="button" disabled={Boolean(busy)} onClick={() => void createBackup()}>
            {busy === 'backup' ? <LoaderCircle size={13} className="spin" /> : <DatabaseBackup size={13} />}
            <span>立即备份</span>
          </button>
        </div>

        <div className="safety-backup-list">
          {status?.backups.length ? status.backups.slice(0, 12).map((backup) => (
            <div className="safety-backup-row" key={backup.fileName}>
              <span className={`safety-backup-kind ${backup.kind}`}>{BACKUP_KIND_LABELS[backup.kind]}</span>
              <span className="safety-backup-copy">
                <strong>{formatTime(backup.createdAt)}</strong>
                <small>{backup.fileName} · {formatBytes(backup.sizeBytes)}</small>
              </span>
              {restoreConfirm === backup.fileName ? (
                <div className="safety-restore-confirm">
                  <span>恢复会用该快照替换当前知识库。</span>
                  <button type="button" onClick={() => setRestoreConfirm(null)}>取消</button>
                  <button type="button" className="danger" disabled={Boolean(busy)} onClick={() => void restoreBackup(backup)}>
                    {busy === `restore-${backup.fileName}` ? '恢复中' : '确认恢复'}
                  </button>
                </div>
              ) : (
                <button type="button" className="safety-row-action" disabled={Boolean(busy)} onClick={() => void restoreBackup(backup)}>
                  <ArchiveRestore size={13} strokeWidth={1.8} />
                  <span>恢复</span>
                </button>
              )}
            </div>
          )) : <div className="safety-empty">还没有备份。已有数据库会在应用启动时自动建立每日快照。</div>}
        </div>
      </section>

      <section className="safety-settings-card">
        <div className="safety-card-heading">
          <span className="safety-card-icon"><Download size={17} strokeWidth={1.8} /></span>
          <div>
            <strong>可读导出</strong>
            <span>导出当前未删除知识、Source、Topic、Tag、Relation 与复习状态；备份仍是完整恢复的权威格式。</span>
          </div>
        </div>
        <div className="safety-export-actions">
          <button type="button" disabled={Boolean(busy)} onClick={() => void runExport('json')}>
            {busy === 'export-json' ? <LoaderCircle size={14} className="spin" /> : <FileJson size={14} />}
            <span>导出 JSON</span>
          </button>
          <button type="button" disabled={Boolean(busy)} onClick={() => void runExport('markdown')}>
            {busy === 'export-markdown' ? <LoaderCircle size={14} className="spin" /> : <FileText size={14} />}
            <span>导出 Markdown</span>
          </button>
          {lastExport && (
            <button type="button" className="safety-export-path" onClick={() => void copyPath('export', lastExport.path)} title={lastExport.path}>
              {copied === 'export' ? <Check size={13} /> : <Copy size={13} />}
              <span>复制最近导出路径</span>
            </button>
          )}
        </div>
      </section>

      <section className="safety-settings-card safety-audit-card">
        <div className="safety-card-heading">
          <span className="safety-card-icon"><History size={17} strokeWidth={1.8} /></span>
          <div>
            <strong>最近操作</strong>
            <span>只记录数据动作和必要元数据，不记录笔记正文、Source 正文或 API Key。</span>
          </div>
        </div>
        <div className="safety-audit-list">
          {audits.length ? audits.slice(0, 30).map((item) => (
            <div key={item.id}>
              <span>{ACTION_LABELS[item.action] || item.action}</span>
              <small>{item.entityId ? `${item.entityType} · ${item.entityId.slice(0, 8)}` : item.entityType}</small>
              <time>{formatTime(item.createdAt)}</time>
            </div>
          )) : <div className="safety-empty">还没有可显示的审计记录。</div>}
        </div>
      </section>

      {notice && <div className="safety-notice">{notice}</div>}
      {error && <div className="settings-error">{error}</div>}
    </div>
  )
}
