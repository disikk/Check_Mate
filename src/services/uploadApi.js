const API_BASE = (import.meta.env.VITE_API_BASE_URL ?? '').replace(/\/$/, '')

function apiUrl(path) {
  return API_BASE ? `${API_BASE}${path}` : path
}

function websocketUrl(path) {
  const baseUrl = API_BASE
    ? new URL(API_BASE)
    : new URL(window.location.origin)

  baseUrl.protocol = baseUrl.protocol === 'https:' ? 'wss:' : 'ws:'
  baseUrl.pathname = path
  baseUrl.search = ''
  baseUrl.hash = ''

  return baseUrl.toString()
}

async function parseJsonResponse(response) {
  const text = await response.text()
  const payload = text ? JSON.parse(text) : {}

  if (!response.ok) {
    throw new Error(payload.error ?? `HTTP ${response.status}`)
  }

  return payload
}

export async function fetchSessionContext() {
  const response = await fetch(apiUrl('/api/session'))
  return parseJsonResponse(response)
}

export async function createBundleUpload(selectedFiles) {
  const formData = new FormData()
  selectedFiles.forEach((file) => {
    formData.append('files', file)
  })

  const response = await fetch(apiUrl('/api/ingest/bundles'), {
    method: 'POST',
    body: formData,
  })

  return parseJsonResponse(response)
}

export function subscribeToBundle(bundleId, handlers = {}) {
  const socket = new WebSocket(websocketUrl(`/api/ingest/bundles/${bundleId}/ws`))

  socket.onmessage = (event) => {
    try {
      const payload = JSON.parse(event.data)
      handlers.onMessage?.(payload)
    } catch (error) {
      handlers.onError?.(error)
    }
  }

  socket.onerror = () => {
    handlers.onError?.(new Error('WebSocket connection failed'))
  }

  socket.onclose = () => {
    handlers.onClose?.()
  }

  return () => {
    if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) {
      socket.close()
    }
  }
}
