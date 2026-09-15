(() => {
  'use strict'

  const BASE_URL = 'http://127.0.0.1:17832'
  const CLIENT_HEADER = { 'X-ZhiForge-Client': 'browser-helper' }
  let token = null

  async function pair() {
    const response = await fetch(`${BASE_URL}/pair`, {
      method: 'GET',
      headers: CLIENT_HEADER,
      cache: 'no-store'
    })
    if (!response.ok) {
      throw new Error(`ZhiForge pair failed: HTTP ${response.status}`)
    }
    const payload = await response.json()
    if (!payload?.success || typeof payload.token !== 'string' || !payload.token) {
      throw new Error('ZhiForge pair returned an invalid token')
    }
    token = payload.token
    return token
  }

  async function postCapture(payload, retry = true) {
    const bridgeToken = token || await pair()
    const response = await fetch(`${BASE_URL}/capture`, {
      method: 'POST',
      headers: {
        ...CLIENT_HEADER,
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${bridgeToken}`
      },
      body: JSON.stringify(payload),
      cache: 'no-store'
    })

    if (response.status === 401 && retry) {
      token = null
      return postCapture(payload, false)
    }
    if (!response.ok) {
      throw new Error(`ZhiForge capture bridge failed: HTTP ${response.status}`)
    }
    const result = await response.json()
    if (!result?.success) {
      throw new Error('ZhiForge capture bridge rejected the context')
    }
    return result
  }

  globalThis.ZhiForgeDesktopBridge = Object.freeze({
    sendCapture: postCapture
  })
})()
