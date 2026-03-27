import { describe, expect, it } from 'vitest'

import {
  adaptFtDashboardSnapshot,
  createDefaultFtFilters,
  createInitialFtDashboardViewModel,
} from './ftDashboardState'

describe('ftDashboardState adapter', () => {
  it('maps backend FT snapshot to stable UI cards, filters and charts', () => {
    const viewModel = adaptFtDashboardSnapshot({
      data_state: 'partial',
      filter_options: {
        buyin_total_cents: [250, 2500],
        bundle_options: [{ bundle_id: 'bundle-1', label: 'Bundle A' }],
        min_date_local: '2026-03-16T10:44',
        max_date_local: '2026-03-17T11:00',
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
        roi: { state: 'ready', value: 12.3, aux_value: null },
        avgKo: { state: 'ready', value: 0.51, aux_value: 0.97 },
        avgFtStack: { state: 'blocked', value: null, aux_value: null },
        koAttempts1: { state: 'blocked', value: null, aux_value: null },
      },
      big_ko_cards: [
        {
          state: 'ready',
          tier: 'x100',
          count: 3,
          occurs_once_every_kos: 41.4,
        },
      ],
      inline_stats: {
        koLuck: { state: 'ready', value: 186.4 },
        roiAdj: { state: 'blocked', value: null },
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
        ft_stack: {
          state: 'blocked',
          metric: 'count',
          density_options: [100, 200],
          default_density_step: 100,
          variants: {
            '100': { bars: [], median_label: null },
          },
        },
      },
    })

    expect(viewModel.dataState).toBe('partial')
    expect(viewModel.filterOptions).toMatchObject({
      buyins: [
        { value: 250, label: '$2.50' },
        { value: 2500, label: '$25.00' },
      ],
      sessions: [{ id: 'bundle-1', label: 'Bundle A' }],
      minDate: '2026-03-16T10:44',
      maxDate: '2026-03-17T11:00',
    })
    expect(viewModel.statCards.roi).toMatchObject({
      label: 'ROI',
      value: '+12.3%',
    })
    expect(viewModel.statCards.avgKo).toMatchObject({
      value: '0.51',
      subtitle: '0.97 за турнир с FT',
    })
    expect(viewModel.statCards.avgFtStack.value).toBe('Недостаточно данных')
    expect(viewModel.bigKoCards[0]).toMatchObject({
      count: '3',
      subtitle: '1 на 41 нокаутов',
      valueColor: 'var(--success)',
    })
    expect(viewModel.inlineStats.koLuck).toMatchObject({
      label: 'KO Luck',
      value: '+$186.40',
    })
    expect(viewModel.inlineStats.roiAdj.value).toBe('Недостаточно данных')
    expect(viewModel.charts.ft.variants.default.bars[0]).toMatchObject({
      color: '#10B981',
      topLabel: '66.7%',
      secondaryLabels: ['n=2'],
    })
    expect(viewModel.charts.ft_stack.state).toBe('blocked')
  })

  it('exposes safe initial filters and empty initial view model', () => {
    expect(createDefaultFtFilters()).toEqual({
      sessionId: '',
      buyinFilter: '',
      dateFrom: '',
      dateTo: '',
    })
    expect(createInitialFtDashboardViewModel()).toMatchObject({
      dataState: 'loading',
      filterOptions: {
        buyins: [],
        sessions: [],
      },
    })
  })
})
