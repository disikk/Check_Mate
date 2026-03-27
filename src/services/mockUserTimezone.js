export const MOCK_USER_TIMEZONE_STORAGE_KEY = 'check-mate.mock-user-timezone'

const FALLBACK_TIMEZONE_OPTIONS = [
  'UTC',
  'Europe/Moscow',
  'Europe/London',
  'America/New_York',
  'America/Los_Angeles',
  'Asia/Krasnoyarsk',
  'Asia/Almaty',
  'Asia/Tbilisi',
  'Asia/Tokyo',
  'Australia/Sydney',
]

export const QUICK_PICK_TIMEZONES = [
  'Asia/Krasnoyarsk',
  'Europe/Moscow',
  'UTC',
  'America/New_York',
]

export function normalizeTimezoneName(value) {
  const normalized = value?.trim()
  return normalized ? normalized : null
}

export function isValidTimezoneName(timezoneName) {
  const normalized = normalizeTimezoneName(timezoneName)
  if (!normalized) {
    return false
  }

  try {
    new Intl.DateTimeFormat('ru-RU', { timeZone: normalized }).format(new Date())
    return true
  } catch {
    return false
  }
}

export function readMockUserTimezone(storage = globalThis?.localStorage) {
  if (!storage) {
    return null
  }

  try {
    const normalized = normalizeTimezoneName(
      storage.getItem(MOCK_USER_TIMEZONE_STORAGE_KEY),
    )

    return normalized && isValidTimezoneName(normalized) ? normalized : null
  } catch {
    return null
  }
}

export function writeMockUserTimezone(timezoneName, storage = globalThis?.localStorage) {
  if (!storage) {
    return
  }

  const normalized = normalizeTimezoneName(timezoneName)

  try {
    if (!normalized) {
      storage.removeItem(MOCK_USER_TIMEZONE_STORAGE_KEY)
      return
    }

    storage.setItem(MOCK_USER_TIMEZONE_STORAGE_KEY, normalized)
  } catch {
    // Mock UX should stay resilient even if storage is unavailable.
  }
}

export function getTimezoneOptions() {
  const supportedValuesOf = Intl.supportedValuesOf

  if (typeof supportedValuesOf === 'function') {
    return Array.from(
      new Set([
        ...QUICK_PICK_TIMEZONES,
        ...supportedValuesOf.call(Intl, 'timeZone'),
      ]),
    )
  }

  return FALLBACK_TIMEZONE_OPTIONS
}

export function formatTimezonePreview(timezoneName) {
  if (!isValidTimezoneName(timezoneName)) {
    return null
  }

  return new Intl.DateTimeFormat('ru-RU', {
    timeZone: timezoneName,
    dateStyle: 'full',
    timeStyle: 'short',
  }).format(new Date())
}
