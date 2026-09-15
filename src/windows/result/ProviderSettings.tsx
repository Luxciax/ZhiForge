import { listen } from '@tauri-apps/api/event'
import { X } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import type { AppConfig } from '../../domain/settings'
import { settingsPageRegistry } from '../../registries/settings-page-registry'
import { getAppConfig } from '../../services/settings-service'
import type { AiSettings } from '../../types'

interface ProviderSettingsProps {
  value: AiSettings
  onApply: (settings: AiSettings) => Promise<void>
  onClose: () => void
}

export function ProviderSettings({ value, onApply, onClose }: ProviderSettingsProps) {
  const pages = useMemo(() => settingsPageRegistry.list(), [])
  const [section, setSection] = useState(() => pages[0]?.id || 'providers')
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined

    void getAppConfig()
      .then((next) => {
        if (!disposed) {
          setConfig(next)
          setLoadError(null)
        }
      })
      .catch((error) => {
        if (!disposed) setLoadError(String(error))
      })

    void listen<AppConfig>('settings://changed', (event) => {
      if (!disposed) setConfig(event.payload)
    }).then((cleanup) => {
      if (disposed) cleanup()
      else unlisten = cleanup
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  const activeDefinition = settingsPageRegistry.get(section) || pages[0]
  const ActivePage = activeDefinition?.component

  return (
    <div className="settings-layer settings-center-layer">
      <div className="settings-panel provider-settings-panel settings-center">
        <aside className="settings-sidebar">
          <div className="settings-brand">
            <div>
              <strong>ZhiForge</strong>
              <span>设置中心</span>
            </div>
          </div>

          <nav className="settings-nav" aria-label="设置导航">
            {pages.map((page) => {
              const Icon = page.icon
              const active = activeDefinition?.id === page.id
              return (
                <button
                  type="button"
                  key={page.id}
                  className={active ? 'active' : undefined}
                  onClick={() => setSection(page.id)}>
                  <Icon size={16} strokeWidth={1.8} />
                  <span>
                    <strong>{page.title}</strong>
                    <small>{page.description}</small>
                  </span>
                </button>
              )
            })}
          </nav>

          {config && (
            <div className="settings-sidebar-summary">
              <span>{config.providers.length} 个渠道</span>
              <span>{config.actions.filter((action) => action.enabled).length} 个启用动作</span>
              <span>{config.appRules.length} 条应用规则</span>
            </div>
          )}
        </aside>

        <section className="settings-workspace">
          <header className="settings-heading settings-center-heading">
            <div className="settings-page-title">
              <strong>{activeDefinition?.title || '设置'}</strong>
              <span>{activeDefinition?.description}</span>
            </div>
            <button type="button" aria-label="关闭设置" title="关闭设置" onClick={onClose}>
              <X size={16} strokeWidth={1.8} />
            </button>
          </header>

          <div className="settings-page-body">
            {loadError ? (
              <div className="settings-error">{loadError}</div>
            ) : config && ActivePage ? (
              <ActivePage
                config={config}
                value={value}
                onConfigChange={setConfig}
                onApply={onApply}
              />
            ) : (
              <div className="provider-editor-empty">读取设置…</div>
            )}
          </div>
        </section>
      </div>
    </div>
  )
}
