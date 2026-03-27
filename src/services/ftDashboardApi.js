import { readMockUserTimezone } from './mockUserTimezone'

function resolveDashboardTimezone() {
  return (
    readMockUserTimezone() ||
    globalThis?.Intl?.DateTimeFormat?.().resolvedOptions?.().timeZone ||
    'UTC'
  )
}

export function buildFtDashboardRequestPath(filters, timezoneName = resolveDashboardTimezone()) {
  const params = new URLSearchParams()
  params.set('timezone', timezoneName)

  if (filters?.buyinFilter !== '' && filters?.buyinFilter != null) {
    params.set('buyin', String(Number(filters.buyinFilter)))
  }

  if (filters?.sessionId) {
    params.set('bundle_id', filters.sessionId)
  }

  if (filters?.dateFrom) {
    params.set('date_from', filters.dateFrom)
  }

  if (filters?.dateTo) {
    params.set('date_to', filters.dateTo)
  }

  return `/api/ft/dashboard?${params.toString()}`
}

export async function fetchFtDashboardSnapshot(filters, options = {}) {
  const {
    fetchImpl = globalThis.fetch,
    signal,
    timezoneName = resolveDashboardTimezone(),
  } = options

  if (typeof fetchImpl !== 'function') {
    throw new Error('Fetch API is unavailable in the current environment')
  }

  const response = await fetchImpl(buildFtDashboardRequestPath(filters, timezoneName), {
    method: 'GET',
    signal,
  })

  if (!response.ok) {
    let message = `FT dashboard request failed with status ${response.status}`
    try {
      const payload = await response.json()
      if (typeof payload?.error === 'string' && payload.error.trim()) {
        message = payload.error
      }
    } catch {
      // Keep the fallback message when backend error payload is unavailable.
    }

    throw new Error(message)
  }

  return response.json()
}
