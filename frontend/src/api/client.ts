// ══════════════════════════════════════════
//  CodeWatch PRO — API Client
// ══════════════════════════════════════════
// api/client.ts
import { getApiBase } from '../store/state'
import type {
  ApiResult, AnalysisResult, MLAnalysisResult, Capabilities
} from '../types'

const TIMEOUT = 60_000

async function post<T>(path: string, body: unknown): Promise<T> {
  const controller = new AbortController()
  const tid = setTimeout(() => controller.abort(), TIMEOUT)
  try {
    const res = await fetch(getApiBase() + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: controller.signal,
    })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    return res.json() as Promise<T>
  } finally {
    clearTimeout(tid)
  }
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(getApiBase() + path, {
    signal: AbortSignal.timeout(8000),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json() as Promise<T>
}

// ── Endpoints
export const api = {
  capabilities: () => get<Capabilities>('/capabilities'),

  checkUrls: (urls: string[], timeout = 10) =>
    post<{ results: ApiResult[] }>('/check/api', { urls, timeout }),

  analyzeCode: (filename: string, content: string) =>
    post<AnalysisResult>('/analyze/code', {
      filename, content,
      tools: ['ast', 'flake8', 'pylint', 'radon'],
    }),

  analyzeML: (filename: string, content: string) =>
    post<MLAnalysisResult>('/analyze/ml', { filename, content }),

  analyzeLogs: (files: { name: string; content: string }[]) =>
    post<{ errors: unknown[]; warnings: unknown[] }>('/analyze/logs', { files }),

  generateDiagram: (filename: string, content: string, diagram_type: string) =>
    post<{ mermaid: string }>('/analyze/diagram', { filename, content, diagram_type }),

  getLogs: (limit = 50) => get<{ logs: unknown[] }>(`/logs?limit=${limit}`),

  /** Fallback: browser fetch sin CORS */
  browserPing: async (url: string): Promise<ApiResult> => {
    const t0 = Date.now()
    const r: ApiResult = {
      url, status: 'unknown', code: null, ms: null,
      error: null, ts: new Date().toLocaleTimeString(), history: [],
    }
    try {
      await fetch(url, { mode: 'no-cors', signal: AbortSignal.timeout(6000) })
      r.ms = Date.now() - t0
      r.status = 'ok'
      r.code = '2xx'
    } catch (e: unknown) {
      r.ms = Date.now() - t0
      const err = e as Error
      r.status  = err.name === 'AbortError' ? 'down' : 'warning'
      r.error   = err.name === 'AbortError' ? 'Timeout' : err.message
    }
    return r
  },
}