(() => {
  'use strict'

  let timer = null
  let lastSignature = ''
  let lastSentAt = 0

  function scheduleCapture() {
    if (timer) clearTimeout(timer)
    timer = setTimeout(sendSelectionContext, 90)
  }

  function sendSelectionContext() {
    timer = null
    const payload = globalThis.ZhiForgeZhihuAdapter?.captureSelection?.()
    if (!payload) return

    const signature = `${payload.url}\n${payload.selectedText}`
    const now = Date.now()
    if (signature === lastSignature && now - lastSentAt < 1500) return
    lastSignature = signature
    lastSentAt = now

    try {
      const result = chrome.runtime.sendMessage({
        type: 'zhiforge:zhihu-selection',
        payload
      })
      if (result?.catch) result.catch(() => undefined)
    } catch {
      // The desktop app remains fully usable through generic Windows selection.
    }
  }

  document.addEventListener('selectionchange', scheduleCapture, true)
  document.addEventListener('mouseup', scheduleCapture, true)
  document.addEventListener('keyup', scheduleCapture, true)
})()
