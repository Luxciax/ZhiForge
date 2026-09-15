(() => {
  'use strict'

  if (typeof importScripts === 'function' && !globalThis.ZhiForgeDesktopBridge) {
    importScripts('desktop-bridge.js')
  }

  function isZhihuSender(sender) {
    try {
      const url = new URL(sender?.url || '')
      return url.protocol === 'https:' && (url.hostname === 'zhihu.com' || url.hostname.endsWith('.zhihu.com'))
    } catch {
      return false
    }
  }

  chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message?.type !== 'zhiforge:zhihu-selection' || !isZhihuSender(sender)) {
      return false
    }

    globalThis.ZhiForgeDesktopBridge
      .sendCapture(message.payload)
      .then((result) => sendResponse({ ok: true, captureId: result.captureId || null }))
      .catch((error) => {
        console.debug('[ZhiForge] Desktop bridge unavailable:', error?.message || error)
        sendResponse({ ok: false, error: String(error?.message || error) })
      })
    return true
  })
})()
