(() => {
  'use strict'

  const SELECTORS = Object.freeze({
    answerRoot: [
      '.AnswerItem',
      '[data-zop]',
      '.List-item',
      'article'
    ],
    questionTitle: [
      'h1.QuestionHeader-title',
      '.QuestionHeader-title',
      'h1.Post-Title'
    ],
    author: [
      '.AuthorInfo-name',
      '.AuthorInfo .UserLink-link',
      '.UserLink-link',
      '[data-za-detail-view-element_name="AuthorInfo"] a'
    ]
  })

  function first(selectors, root = document) {
    for (const selector of selectors) {
      const element = root?.querySelector?.(selector)
      if (element) return element
    }
    return null
  }

  function closestAny(element, selectors) {
    if (!element?.closest) return null
    for (const selector of selectors) {
      const match = element.closest(selector)
      if (match) return match
    }
    return null
  }

  function normalizeText(value) {
    return String(value || '')
      .replace(/\u00a0/g, ' ')
      .replace(/\s+/g, ' ')
      .trim()
  }

  function textOf(element) {
    return normalizeText(element?.innerText || element?.textContent || '')
  }

  function selectionElement(selection) {
    const node = selection?.anchorNode || selection?.focusNode
    if (!node) return null
    return node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement
  }

  function answerRoot(selection) {
    const element = selectionElement(selection)
    return closestAny(element, SELECTORS.answerRoot) || element?.parentElement || document.body
  }

  function parseDataZop(root) {
    const holder = root?.matches?.('[data-zop]') ? root : root?.querySelector?.('[data-zop]')
    const raw = holder?.getAttribute?.('data-zop')
    if (!raw) return null
    try {
      return JSON.parse(raw)
    } catch {
      return null
    }
  }

  function questionTitle(root) {
    const direct = first(SELECTORS.questionTitle)
    if (textOf(direct)) return textOf(direct)

    const zop = parseDataZop(root)
    const zopTitle = normalizeText(zop?.title || zop?.questionTitle || '')
    if (zopTitle) return zopTitle

    const ogTitle = document.querySelector('meta[property="og:title"]')?.getAttribute('content')
    return normalizeText(ogTitle || document.title).replace(/\s*[-–—]\s*知乎\s*$/, '')
  }

  function authorName(root) {
    const zop = parseDataZop(root)
    const fromZop = normalizeText(zop?.authorName || zop?.author?.name || '')
    if (fromZop) return fromZop

    const local = first(SELECTORS.author, root)
    if (textOf(local)) return textOf(local)

    const nearby = closestAny(root, ['.List-item', '.AnswerItem', 'article'])
    return textOf(first(SELECTORS.author, nearby || document)) || null
  }

  function contextAround(root, selectedText) {
    const body = normalizeText(root?.innerText || root?.textContent || '')
    const needle = normalizeText(selectedText)
    if (!body || !needle) return { before: null, after: null }
    const index = body.indexOf(needle)
    if (index < 0) return { before: null, after: null }

    const radius = 700
    const before = body.slice(Math.max(0, index - radius), index).trim()
    const after = body.slice(index + needle.length, index + needle.length + radius).trim()
    return {
      before: before || null,
      after: after || null
    }
  }

  function canonicalUrl() {
    const url = new URL(window.location.href)
    url.hash = ''
    return url.toString()
  }

  function captureSelection() {
    const hostname = window.location.hostname.toLowerCase()
    if (hostname !== 'zhihu.com' && !hostname.endsWith('.zhihu.com')) return null

    const selection = window.getSelection()
    if (!selection || selection.isCollapsed) return null
    const selectedText = selection.toString().trim()
    if (!selectedText) return null

    const root = answerRoot(selection)
    const context = contextAround(root, selectedText)
    return {
      platform: 'zhihu',
      url: canonicalUrl(),
      title: questionTitle(root) || null,
      pageTitle: normalizeText(document.title) || null,
      author: authorName(root),
      selectedText,
      contextBefore: context.before,
      contextAfter: context.after
    }
  }

  globalThis.ZhiForgeZhihuAdapter = Object.freeze({
    captureSelection
  })
})()
