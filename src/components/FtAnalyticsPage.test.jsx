import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import FtAnalyticsPage from './FtAnalyticsPage'
import { MOCK_USER_TIMEZONE_STORAGE_KEY } from '../services/mockUserTimezone'

const firstSnapshot = {
  data_state: 'partial',
  filter_options: {
    buyin_total_cents: [2500],
    bundle_options: [{ bundle_id: 'bundle-1', label: 'Bundle A' }],
    min_date_local: '2026-03-16T10:44',
    max_date_local: '2026-03-16T10:44',
  },
  coverage: {
    tournament_count: 1,
    summary_tournament_count: 1,
    hand_tournament_count: 0,
    bundle_count: 1,
    min_started_at_local: '2026-03-16T10:44',
    max_started_at_local: '2026-03-16T10:44',
  },
  stat_cards: {
    roi: { state: 'ready', value: 42.4, aux_value: null },
    ftReach: { state: 'ready', value: 50.0, aux_value: null },
    itm: { state: 'ready', value: 20.0, aux_value: null },
    avgKo: { state: 'ready', value: 0.51, aux_value: 0.97 },
    koAttempts1: { state: 'blocked', value: null, aux_value: null },
    roiOnFt: { state: 'ready', value: 103.1, aux_value: null },
    avgFtStack: { state: 'blocked', value: null, aux_value: null },
    deepFtReach: { state: 'ready', value: 61.0, aux_value: null },
    ftStackConv79: { state: 'ready', value: 1.15, aux_value: 1.75 },
    koAttempts2: { state: 'blocked', value: null, aux_value: null },
    winningsFromKo: { state: 'ready', value: 44.1, aux_value: null },
    avgPlaceFt: { state: 'ready', value: 4.56, aux_value: null },
    deepFtRoi: { state: 'ready', value: 231.8, aux_value: null },
    ftStackConv56: { state: 'ready', value: 1.12, aux_value: 1.94 },
    koAttempts3p: { state: 'blocked', value: null, aux_value: null },
    winningsFromItm: { state: 'ready', value: 55.9, aux_value: null },
    avgPlaceAll: { state: 'ready', value: 8.64, aux_value: null },
    deepFtStack: { state: 'ready', value: 3551, aux_value: 19.2 },
    ftStackConv34: { state: 'ready', value: 1.07, aux_value: 2.9 },
  },
  big_ko_cards: [
    { state: 'ready', tier: 'x10', count: 7, occurs_once_every_kos: 19.3 },
  ],
  inline_stats: {
    koLuck: { state: 'ready', value: 186.4 },
    roiAdj: { state: 'ready', value: 7.9 },
  },
  charts: {
    ft: {
      state: 'ready',
      metric: 'count',
      density_options: [],
      default_density_step: null,
      variants: {
        default: {
          bars: [
            { label: '1', value: 2, sample_size: 2, attempts: null },
            { label: '2', value: 1, sample_size: 1, attempts: null },
          ],
          median_label: null,
        },
      },
    },
  },
}

const secondSnapshot = {
  ...firstSnapshot,
  data_state: 'ready',
  stat_cards: {
    ...firstSnapshot.stat_cards,
    roi: { state: 'ready', value: 11.1, aux_value: null },
    avgFtStack: { state: 'ready', value: 2057, aux_value: 20.1 },
  },
}

describe('FtAnalyticsPage', () => {
  beforeEach(() => {
    window.localStorage.clear()
    window.localStorage.setItem(
      MOCK_USER_TIMEZONE_STORAGE_KEY,
      'Asia/Krasnoyarsk',
    )
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('loads FT dashboard from the real API contract and refetches on filter changes', async () => {
    const user = userEvent.setup()
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => firstSnapshot,
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => secondSnapshot,
      })

    vi.stubGlobal('fetch', fetchMock)

    render(<FtAnalyticsPage />)

    expect(await screen.findByText('+42.4%')).toBeInTheDocument()
    expect(screen.getAllByText('Недостаточно данных').length).toBeGreaterThan(0)
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining('/api/ft/dashboard?'),
      expect.objectContaining({ method: 'GET' }),
    )
    expect(fetchMock.mock.calls[0][0]).toContain('timezone=Asia%2FKrasnoyarsk')

    await user.selectOptions(screen.getByLabelText('Бай-ин'), '2500')

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(2)
    })
    expect(fetchMock.mock.calls[1][0]).toContain('buyin=2500')
    expect(await screen.findByText('+11.1%')).toBeInTheDocument()
    expect(screen.getByText('2,057')).toBeInTheDocument()
  })
})
