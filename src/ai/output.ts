const HIDDEN_TAGS = ['think', 'analysis', 'reasoning'] as const

export function sanitizeModelOutput(raw: string) {
  let visible = raw

  for (const tag of HIDDEN_TAGS) {
    const complete = new RegExp(`<${tag}\\b[^>]*>[\\s\\S]*?<\\/${tag}\\s*>`, 'gi')
    const unfinished = new RegExp(`<${tag}\\b[^>]*>[\\s\\S]*$`, 'gi')
    const orphanClose = new RegExp(`<\\/${tag}\\s*>`, 'gi')
    visible = visible.replace(complete, '').replace(unfinished, '').replace(orphanClose, '')
  }

  return visible
}
