import { listen } from '@tauri-apps/api/event'
import {
  ExternalLink,
  Focus,
  LoaderCircle,
  Maximize2,
  RefreshCw,
  RotateCcw,
  Waypoints,
  ZoomIn,
  ZoomOut
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { getKnowledgeGraph } from '../../services/knowledge-service'
import type {
  KnowledgeGraphEdge,
  KnowledgeGraphEdgeKind,
  KnowledgeGraphNode,
  KnowledgeGraphSnapshot,
  KnowledgeQualityStatus,
  KnowledgeRelationType,
  KnowledgeStatus
} from '../../types'

interface KnowledgeGraphViewProps {
  initialCenterId?: string | null
  onCenterChange?: (knowledgeUnitId: string | null) => void
  onOpenKnowledge: (knowledgeUnitId: string) => void
}

interface Point {
  x: number
  y: number
}

const WIDTH = 1000
const HEIGHT = 660
const CX = WIDTH / 2
const CY = HEIGHT / 2

const STATUS_LABELS: Record<KnowledgeStatus, string> = {
  captured: '待提炼',
  processed: '已处理',
  weak: '薄弱',
  learning: '学习中',
  reviewing: '复习中',
  mastered: '已掌握'
}

const QUALITY_LABELS: Record<KnowledgeQualityStatus, string> = {
  unverified: '未验证',
  verified: '已验证',
  conflicted: '有冲突',
  stale: '可能过时',
  needs_expansion: '待扩展'
}

const RELATION_LABELS: Record<KnowledgeRelationType, string> = {
  related_to: '相关',
  supports: '支持',
  contradicts: '冲突',
  example_of: '示例',
  prerequisite_of: '前置',
  derived_from: '来源于',
  extends: '扩展'
}

const EDGE_LABELS: Record<KnowledgeGraphEdgeKind, string> = {
  relation: '关系',
  source: '来源',
  topic: '主题',
  tag: '标签'
}

const EDGE_FILTER_ORDER: KnowledgeGraphEdgeKind[] = ['relation', 'source', 'topic', 'tag']
const DIRECTIONAL_RELATIONS = new Set<KnowledgeRelationType>([
  'supports',
  'example_of',
  'prerequisite_of',
  'derived_from',
  'extends'
])

function shorten(value: string, max = 15) {
  const chars = Array.from(value.trim())
  return chars.length <= max ? chars.join('') : `${chars.slice(0, max).join('')}…`
}

function degreeMap(nodes: KnowledgeGraphNode[], edges: KnowledgeGraphEdge[]) {
  const map = new Map(nodes.map((node) => [node.id, 0]))
  for (const edge of edges) {
    map.set(edge.sourceKnowledgeId, (map.get(edge.sourceKnowledgeId) ?? 0) + 1)
    map.set(edge.targetKnowledgeId, (map.get(edge.targetKnowledgeId) ?? 0) + 1)
  }
  return map
}

function placeRing(ids: string[], radius: number, angleOffset: number, points: Map<string, Point>) {
  if (!ids.length) return
  ids.forEach((id, index) => {
    const angle = angleOffset + (Math.PI * 2 * index) / ids.length
    points.set(id, {
      x: CX + Math.cos(angle) * radius,
      y: CY + Math.sin(angle) * radius
    })
  })
}

function graphDistances(centerId: string, edges: KnowledgeGraphEdge[]) {
  const adjacency = new Map<string, Set<string>>()
  for (const edge of edges) {
    if (!adjacency.has(edge.sourceKnowledgeId)) adjacency.set(edge.sourceKnowledgeId, new Set())
    if (!adjacency.has(edge.targetKnowledgeId)) adjacency.set(edge.targetKnowledgeId, new Set())
    adjacency.get(edge.sourceKnowledgeId)?.add(edge.targetKnowledgeId)
    adjacency.get(edge.targetKnowledgeId)?.add(edge.sourceKnowledgeId)
  }
  const distances = new Map<string, number>([[centerId, 0]])
  const queue = [centerId]
  while (queue.length) {
    const current = queue.shift()!
    const distance = distances.get(current) ?? 0
    for (const neighbor of adjacency.get(current) ?? []) {
      if (distances.has(neighbor)) continue
      distances.set(neighbor, distance + 1)
      queue.push(neighbor)
    }
  }
  return distances
}

function graphLayout(snapshot: KnowledgeGraphSnapshot, edges: KnowledgeGraphEdge[]) {
  const points = new Map<string, Point>()
  if (!snapshot.nodes.length) return points

  const degrees = degreeMap(snapshot.nodes, edges)
  const sortByDegree = (left: KnowledgeGraphNode, right: KnowledgeGraphNode) =>
    (degrees.get(right.id) ?? 0) - (degrees.get(left.id) ?? 0) || right.updatedAt - left.updatedAt

  if (snapshot.centerKnowledgeId) {
    const centerId = snapshot.centerKnowledgeId
    points.set(centerId, { x: CX, y: CY })
    const distances = graphDistances(centerId, edges)
    const ringOne = snapshot.nodes.filter((node) => distances.get(node.id) === 1).sort(sortByDegree)
    const ringTwo = snapshot.nodes.filter((node) => (distances.get(node.id) ?? 99) === 2).sort(sortByDegree)
    const disconnected = snapshot.nodes
      .filter((node) => node.id !== centerId && !distances.has(node.id))
      .sort(sortByDegree)
    placeRing(ringOne.map((node) => node.id), 176, -Math.PI / 2, points)
    placeRing(ringTwo.map((node) => node.id), 286, -Math.PI / 2 + 0.12, points)
    placeRing(disconnected.map((node) => node.id), 310, Math.PI / 2, points)
    return points
  }

  const ordered = [...snapshot.nodes].sort(sortByDegree)
  const first = ordered.slice(0, 8)
  const second = ordered.slice(8, 24)
  const third = ordered.slice(24)
  placeRing(first.map((node) => node.id), 118, -Math.PI / 2, points)
  placeRing(second.map((node) => node.id), 220, -Math.PI / 2 + 0.1, points)
  placeRing(third.map((node) => node.id), 305, -Math.PI / 2 + 0.05, points)
  if (ordered.length === 1) points.set(ordered[0].id, { x: CX, y: CY })
  return points
}

function edgeClass(edge: KnowledgeGraphEdge) {
  return [
    'knowledge-map-edge',
    edge.kind,
    edge.relationType === 'contradicts' ? 'contradicts' : ''
  ].filter(Boolean).join(' ')
}

function nodeRadius(node: KnowledgeGraphNode, degree: number) {
  if (node.isCenter) return 27
  return Math.min(23, 15 + Math.sqrt(Math.max(0, degree)) * 2.1)
}

function edgeEndpoints(source: Point, target: Point, sourceRadius: number, targetRadius: number) {
  const dx = target.x - source.x
  const dy = target.y - source.y
  const length = Math.max(1, Math.hypot(dx, dy))
  const ux = dx / length
  const uy = dy / length
  return {
    x1: source.x + ux * (sourceRadius + 2),
    y1: source.y + uy * (sourceRadius + 2),
    x2: target.x - ux * (targetRadius + 6),
    y2: target.y - uy * (targetRadius + 6)
  }
}

export function KnowledgeGraphView({
  initialCenterId = null,
  onCenterChange,
  onOpenKnowledge
}: KnowledgeGraphViewProps) {
  const [centerId, setCenterId] = useState<string | null>(initialCenterId)
  const [depth, setDepth] = useState<1 | 2>(2)
  const [snapshot, setSnapshot] = useState<KnowledgeGraphSnapshot | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(initialCenterId)
  const [enabledKinds, setEnabledKinds] = useState<Record<KnowledgeGraphEdgeKind, boolean>>({
    relation: true,
    source: true,
    topic: true,
    tag: false
  })
  const [zoom, setZoom] = useState(1)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [refreshKey, setRefreshKey] = useState(0)

  useEffect(() => {
    setCenterId(initialCenterId)
    setSelectedId(initialCenterId)
  }, [initialCenterId])

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const next = await getKnowledgeGraph(centerId, depth, 48)
      setSnapshot(next)
      setSelectedId((current) => {
        if (current && next.nodes.some((node) => node.id === current)) return current
        return next.centerKnowledgeId ?? null
      })
      setError(null)
    } catch (loadError) {
      setError(String(loadError))
    } finally {
      setLoading(false)
    }
  }, [centerId, depth, refreshKey])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen('knowledge://changed', () => {
      if (!disposed) setRefreshKey((value) => value + 1)
    }).then((callback) => {
      if (disposed) callback()
      else unlisten = callback
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  const activeEdges = useMemo(
    () => snapshot?.edges.filter((edge) => enabledKinds[edge.kind]) ?? [],
    [enabledKinds, snapshot]
  )
  const degrees = useMemo(
    () => degreeMap(snapshot?.nodes ?? [], snapshot?.edges ?? []),
    [snapshot]
  )
  const points = useMemo(
    () => snapshot ? graphLayout(snapshot, snapshot.edges) : new Map<string, Point>(),
    [snapshot]
  )
  const selected = useMemo(
    () => snapshot?.nodes.find((node) => node.id === selectedId) ?? null,
    [selectedId, snapshot]
  )
  const connectedEdges = useMemo(
    () => selectedId
      ? activeEdges
          .filter((edge) => edge.sourceKnowledgeId === selectedId || edge.targetKnowledgeId === selectedId)
          .sort((left, right) => right.weight - left.weight)
      : [],
    [activeEdges, selectedId]
  )
  const connectedIds = useMemo(() => {
    const ids = new Set<string>()
    if (!selectedId) return ids
    ids.add(selectedId)
    for (const edge of connectedEdges) {
      ids.add(edge.sourceKnowledgeId)
      ids.add(edge.targetKnowledgeId)
    }
    return ids
  }, [connectedEdges, selectedId])
  const labelIds = useMemo(() => {
    if (!snapshot) return new Set<string>()
    if (snapshot.nodes.length <= 28) return new Set(snapshot.nodes.map((node) => node.id))
    const ranked = [...snapshot.nodes]
      .sort((left, right) => (degrees.get(right.id) ?? 0) - (degrees.get(left.id) ?? 0))
      .slice(0, 16)
      .map((node) => node.id)
    if (selectedId) ranked.push(selectedId)
    if (snapshot.centerKnowledgeId) ranked.push(snapshot.centerKnowledgeId)
    return new Set(ranked)
  }, [degrees, selectedId, snapshot])

  const changeCenter = (knowledgeUnitId: string | null) => {
    setCenterId(knowledgeUnitId)
    setSelectedId(knowledgeUnitId)
    setZoom(1)
    onCenterChange?.(knowledgeUnitId)
  }

  const toggleKind = (kind: KnowledgeGraphEdgeKind) => {
    setEnabledKinds((current) => ({ ...current, [kind]: !current[kind] }))
  }

  const scaleTransform = `translate(${CX * (1 - zoom)} ${CY * (1 - zoom)}) scale(${zoom})`

  return (
    <section className="knowledge-map-view" aria-label="知识地图">
      <header className="workspace-page-heading knowledge-map-heading">
        <div>
          <h1>知识地图</h1>
          <p>
            {loading && !snapshot
              ? '正在构建知识连接…'
              : snapshot?.centerKnowledgeId
                ? `${snapshot.nodes.length} 个节点 · ${activeEdges.length} 条当前连接 · ${depth} 跳视图`
                : `${snapshot?.nodes.length ?? 0} / ${snapshot?.totalActive ?? 0} 条活跃知识 · ${activeEdges.length} 条当前连接`}
          </p>
        </div>
        <div className="knowledge-map-heading-actions">
          {centerId && (
            <button type="button" onClick={() => changeCenter(null)}>
              <RotateCcw size={14} strokeWidth={1.8} />
              <span>返回全局</span>
            </button>
          )}
          <button type="button" disabled={loading} onClick={() => setRefreshKey((value) => value + 1)}>
            {loading ? <LoaderCircle size={14} className="spin" /> : <RefreshCw size={14} strokeWidth={1.8} />}
            <span>刷新</span>
          </button>
        </div>
      </header>

      {error && (
        <button type="button" className="workspace-error" onClick={() => void load()}>
          知识地图读取失败，点击重试
        </button>
      )}

      <div className="knowledge-map-toolbar">
        <div className="knowledge-map-filters" aria-label="连接类型">
          {EDGE_FILTER_ORDER.map((kind) => (
            <button
              type="button"
              key={kind}
              className={enabledKinds[kind] ? 'active' : undefined}
              onClick={() => toggleKind(kind)}>
              <span className={`knowledge-map-legend-dot ${kind}`} />
              {EDGE_LABELS[kind]}
            </button>
          ))}
        </div>
        <div className="knowledge-map-controls">
          {centerId && (
            <div className="knowledge-map-depth" aria-label="地图深度">
              <button type="button" className={depth === 1 ? 'active' : undefined} onClick={() => setDepth(1)}>1 跳</button>
              <button type="button" className={depth === 2 ? 'active' : undefined} onClick={() => setDepth(2)}>2 跳</button>
            </div>
          )}
          <button type="button" aria-label="缩小" disabled={zoom <= 0.75} onClick={() => setZoom((value) => Math.max(0.75, value - 0.15))}>
            <ZoomOut size={14} strokeWidth={1.8} />
          </button>
          <button type="button" aria-label="适合画布" onClick={() => setZoom(1)}>
            <Maximize2 size={14} strokeWidth={1.8} />
          </button>
          <button type="button" aria-label="放大" disabled={zoom >= 1.6} onClick={() => setZoom((value) => Math.min(1.6, value + 0.15))}>
            <ZoomIn size={14} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div className="knowledge-map-body">
        <div className="knowledge-map-canvas">
          {loading && !snapshot ? (
            <div className="workspace-empty">正在读取知识连接…</div>
          ) : !snapshot?.nodes.length ? (
            <div className="knowledge-map-empty">
              <Waypoints size={20} strokeWidth={1.7} />
              <strong>还没有可展示的知识</strong>
              <span>先保存并提炼几条内容，地图会从已有来源、主题与关系中自动形成。</span>
            </div>
          ) : (
            <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label="知识关系图">
              <defs>
                <marker id="knowledge-map-arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto" markerUnits="strokeWidth">
                  <path d="M0,0 L7,3.5 L0,7 Z" />
                </marker>
              </defs>
              <g transform={scaleTransform}>
                {activeEdges.map((edge) => {
                  const source = points.get(edge.sourceKnowledgeId)
                  const target = points.get(edge.targetKnowledgeId)
                  if (!source || !target) return null
                  const sourceNode = snapshot.nodes.find((node) => node.id === edge.sourceKnowledgeId)
                  const targetNode = snapshot.nodes.find((node) => node.id === edge.targetKnowledgeId)
                  if (!sourceNode || !targetNode) return null
                  const endpoints = edgeEndpoints(
                    source,
                    target,
                    nodeRadius(sourceNode, degrees.get(sourceNode.id) ?? 0),
                    nodeRadius(targetNode, degrees.get(targetNode.id) ?? 0)
                  )
                  const highlighted = !selectedId || edge.sourceKnowledgeId === selectedId || edge.targetKnowledgeId === selectedId
                  const directional = edge.kind === 'relation' && edge.relationType && DIRECTIONAL_RELATIONS.has(edge.relationType)
                  return (
                    <line
                      key={edge.id}
                      x1={endpoints.x1}
                      y1={endpoints.y1}
                      x2={endpoints.x2}
                      y2={endpoints.y2}
                      className={`${edgeClass(edge)}${highlighted ? ' highlighted' : ' muted'}`}
                      markerEnd={directional ? 'url(#knowledge-map-arrow)' : undefined}>
                      <title>{edge.reason}</title>
                    </line>
                  )
                })}

                {snapshot.nodes.map((node) => {
                  const point = points.get(node.id)
                  if (!point) return null
                  const degree = degrees.get(node.id) ?? 0
                  const selectedNode = node.id === selectedId
                  const dimmed = Boolean(selectedId && !connectedIds.has(node.id))
                  const radius = nodeRadius(node, degree)
                  return (
                    <g
                      key={node.id}
                      className={`knowledge-map-node status-${node.status}${node.isCenter ? ' center' : ''}${selectedNode ? ' selected' : ''}${dimmed ? ' dimmed' : ''}`}
                      transform={`translate(${point.x} ${point.y})`}
                      role="button"
                      tabIndex={0}
                      onClick={() => setSelectedId(node.id)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' || event.key === ' ') setSelectedId(node.id)
                      }}>
                      <circle r={radius} />
                      <circle className="knowledge-map-node-core" r={Math.max(4, radius * 0.24)} />
                      <title>{node.title}</title>
                      {labelIds.has(node.id) && (
                        <text y={radius + 17} textAnchor="middle">{shorten(node.title)}</text>
                      )}
                    </g>
                  )
                })}
              </g>
            </svg>
          )}
          {snapshot?.truncated && (
            <div className="knowledge-map-limit-note">为保持可读性，仅展示连接度较高的 {snapshot.nodes.length} 条知识。</div>
          )}
        </div>

        <aside className="knowledge-map-inspector" aria-label="地图详情">
          {!selected ? (
            <div className="knowledge-map-inspector-empty">
              <Focus size={18} strokeWidth={1.7} />
              <span>选择一个节点查看连接依据。</span>
            </div>
          ) : (
            <>
              <div className="knowledge-map-inspector-head">
                <span>{STATUS_LABELS[selected.status]} · {QUALITY_LABELS[selected.qualityStatus]}</span>
                <strong>{selected.title}</strong>
                <small>{selected.author || selected.platform}</small>
              </div>

              <div className="knowledge-map-metrics">
                <span><b>{selected.masteryScore}</b>掌握度</span>
                <span><b>{selected.reviewCount}</b>复习</span>
                <span><b>{selected.relationCount}</b>显式关系</span>
                <span><b>{selected.sourceCount}</b>来源</span>
              </div>

              {(selected.topicNames.length > 0 || selected.tagNames.length > 0) && (
                <div className="knowledge-map-inspector-chips">
                  {selected.topicNames.slice(0, 5).map((topic) => <span className="topic-chip" key={`topic:${topic}`}>{topic}</span>)}
                  {selected.tagNames.slice(0, 5).map((tag) => <span className="tag-chip" key={`tag:${tag}`}>{tag}</span>)}
                </div>
              )}

              <div className="knowledge-map-inspector-actions">
                <button type="button" disabled={selected.id === centerId} onClick={() => changeCenter(selected.id)}>
                  <Focus size={13} strokeWidth={1.8} />
                  <span>{selected.id === centerId ? '当前中心' : '以此为中心'}</span>
                </button>
                <button type="button" onClick={() => onOpenKnowledge(selected.id)}>
                  <ExternalLink size={13} strokeWidth={1.8} />
                  <span>打开知识</span>
                </button>
              </div>

              <section className="knowledge-map-connections">
                <div className="knowledge-map-inspector-section-title">
                  <strong>连接依据</strong>
                  <span>{connectedEdges.length}</span>
                </div>
                {connectedEdges.length === 0 ? (
                  <p>当前筛选下没有可见连接。可以开启更多连接类型。</p>
                ) : (
                  <div>
                    {connectedEdges.slice(0, 14).map((edge) => {
                      const otherId = edge.sourceKnowledgeId === selected.id ? edge.targetKnowledgeId : edge.sourceKnowledgeId
                      const other = snapshot?.nodes.find((node) => node.id === otherId)
                      return (
                        <button type="button" key={edge.id} onClick={() => setSelectedId(otherId)}>
                          <span className={`knowledge-map-legend-dot ${edge.kind}`} />
                          <span>
                            <strong>{other ? shorten(other.title, 22) : '相关知识'}</strong>
                            <small>{edge.kind === 'relation' && edge.relationType ? RELATION_LABELS[edge.relationType] : edge.reason}</small>
                          </span>
                        </button>
                      )
                    })}
                  </div>
                )}
              </section>
            </>
          )}
        </aside>
      </div>
    </section>
  )
}
