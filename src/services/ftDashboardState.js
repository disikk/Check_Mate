import {
  ftChartConfig,
  ftChartPalettes,
  ftInlineStatMeta,
  ftStatCardMeta,
} from '../data/ftAnalyticsConfig'

export function createDefaultFtFilters() {
  return {
    sessionId: '',
    buyinFilter: '',
    dateFrom: '',
    dateTo: '',
  }
}

export function createInitialFtDashboardViewModel() {
  return {
    dataState: 'loading',
    filterOptions: {
      buyins: [],
      sessions: [],
      minDate: '',
      maxDate: '',
    },
    coverage: null,
    statCards: {},
    bigKoCards: [],
    inlineStats: {},
    charts: {},
  }
}

export function adaptFtDashboardSnapshot(snapshot) {
  return {
    dataState: snapshot?.data_state || 'empty',
    filterOptions: adaptFilterOptions(snapshot?.filter_options),
    coverage: adaptCoverage(snapshot?.coverage),
    statCards: adaptStatCards(snapshot?.stat_cards),
    bigKoCards: adaptBigKoCards(snapshot?.big_ko_cards),
    inlineStats: adaptInlineStats(snapshot?.inline_stats),
    charts: adaptCharts(snapshot?.charts),
  }
}

export function getFtDashboardStateLabel(dataState) {
  switch (dataState) {
    case 'ready':
      return 'Готово'
    case 'partial':
      return 'Частичное покрытие'
    case 'blocked':
      return 'Недостаточно данных'
    case 'loading':
      return 'Загрузка'
    default:
      return 'Нет данных'
  }
}

function adaptFilterOptions(filterOptions) {
  return {
    buyins: (filterOptions?.buyin_total_cents || []).map((buyinTotalCents) => ({
      value: buyinTotalCents,
      label: formatCurrencyFromCents(buyinTotalCents),
    })),
    sessions: (filterOptions?.bundle_options || []).map((session) => ({
      id: session.bundle_id,
      label: session.label,
    })),
    minDate: filterOptions?.min_date_local || '',
    maxDate: filterOptions?.max_date_local || '',
  }
}

function adaptCoverage(coverage) {
  if (!coverage) {
    return null
  }

  return {
    tournamentCount: coverage.tournament_count,
    summaryTournamentCount: coverage.summary_tournament_count,
    handTournamentCount: coverage.hand_tournament_count,
    bundleCount: coverage.bundle_count,
    minStartedAtLocal: coverage.min_started_at_local,
    maxStartedAtLocal: coverage.max_started_at_local,
  }
}

function adaptStatCards(statCards = {}) {
  return Object.fromEntries(
    Object.entries(ftStatCardMeta).map(([cardKey, meta]) => [
      cardKey,
      buildStatCard(cardKey, meta, statCards[cardKey]),
    ]),
  )
}

function buildStatCard(cardKey, meta, sourceCard) {
  if (!sourceCard) {
    return buildUnavailableCard(meta, 'blocked')
  }

  const state = sourceCard.state || 'blocked'
  if (state !== 'ready') {
    return buildUnavailableCard(meta, state)
  }

  switch (cardKey) {
    case 'roi':
    case 'roiOnFt':
    case 'deepFtRoi':
      return createCard(meta, formatSignedPercent(sourceCard.value), {
        valueColor: getSignedColor(sourceCard.value),
      })
    case 'ftReach':
    case 'itm':
    case 'deepFtReach':
    case 'winningsFromKo':
    case 'winningsFromItm':
      return createCard(meta, formatPercent(sourceCard.value))
    case 'avgKo':
      return createCard(meta, formatDecimal(sourceCard.value, 2), {
        subtitle: sourceCard.aux_value == null
          ? null
          : `${formatDecimal(sourceCard.aux_value, 2)} за турнир с FT`,
      })
    case 'avgFtStack':
      return createCard(meta, formatInteger(sourceCard.value), {
        subtitle: sourceCard.aux_value == null
          ? null
          : `${formatInteger(sourceCard.value)} фишек / ${formatDecimal(sourceCard.aux_value, 1)} BB`,
      })
    case 'avgPlaceFt':
    case 'avgPlaceAll':
      return createCard(meta, formatDecimal(sourceCard.value, 2))
    case 'deepFtStack':
      return createCard(
        meta,
        sourceCard.aux_value == null
          ? formatInteger(sourceCard.value)
          : `${formatInteger(sourceCard.value)}/${formatDecimal(sourceCard.aux_value, 1)}`,
      )
    case 'ftStackConv79':
    case 'ftStackConv56':
    case 'ftStackConv34':
      return createCard(meta, formatDecimal(sourceCard.value, 2), {
        subtitle: sourceCard.aux_value == null
          ? null
          : `${formatDecimal(sourceCard.aux_value, 2)} попыток за турнир с FT`,
      })
    case 'koAttempts1':
    case 'koAttempts2':
    case 'koAttempts3p':
      return buildUnavailableCard(meta, 'blocked')
    default:
      return createCard(meta, formatDecimal(sourceCard.value, 2))
  }
}

function buildUnavailableCard(meta, state) {
  return createCard(meta, formatUnavailableValue(state), {
    valueColor: 'var(--text-muted)',
  })
}

function createCard(meta, value, overrides = {}) {
  return {
    label: meta.label,
    tooltip: meta.tooltip || null,
    value,
    subtitle: overrides.subtitle || null,
    valueColor: overrides.valueColor || null,
  }
}

function adaptBigKoCards(bigKoCards = []) {
  return bigKoCards.map((card) => ({
    tier: card.tier,
    count: card.state === 'ready' && card.count != null ? formatInteger(card.count) : formatUnavailableValue(card.state),
    subtitle: card.state === 'ready' && card.occurs_once_every_kos != null
      ? `1 на ${Math.max(1, Math.round(card.occurs_once_every_kos))} нокаутов`
      : unavailableSubtitle(card.state),
    valueColor: getBigKoColor(card),
  }))
}

function adaptInlineStats(inlineStats = {}) {
  return Object.fromEntries(
    Object.entries(ftInlineStatMeta).map(([statKey, meta]) => [
      statKey,
      buildInlineStat(meta, inlineStats[statKey]),
    ]),
  )
}

function buildInlineStat(meta, sourceStat) {
  const state = sourceStat?.state || 'blocked'
  if (state !== 'ready') {
    return {
      label: meta.label,
      tooltip: meta.tooltip || null,
      value: formatUnavailableValue(state),
      valueColor: 'var(--text-muted)',
    }
  }

  if (meta.label === 'KO Luck') {
    return {
      label: meta.label,
      tooltip: meta.tooltip || null,
      value: formatSignedMoney(sourceStat.value),
      valueColor: getSignedColor(sourceStat.value),
    }
  }

  return {
    label: meta.label,
    tooltip: meta.tooltip || null,
    value: formatSignedPercent(sourceStat.value),
    valueColor: getSignedColor(sourceStat.value),
  }
}

function adaptCharts(charts = {}) {
  return Object.fromEntries(
    Object.entries(ftChartConfig).map(([chartKey, chartMeta]) => [
      chartKey,
      buildChart(chartKey, chartMeta, charts[chartKey]),
    ]),
  )
}

function buildChart(chartKey, chartMeta, sourceChart) {
  const state = sourceChart?.state || 'blocked'
  const metric = normalizeChartMetric(sourceChart?.metric || 'count')
  const densityOptions = sourceChart?.density_options || []
  const defaultDensityStep = sourceChart?.default_density_step
  const variants = sourceChart?.variants || {}

  return {
    key: chartKey,
    state,
    metric,
    header: chartMeta.header,
    tooltip: chartMeta.tooltip,
    xAxisLabel: chartMeta.xAxisLabel,
    yAxisLabel: chartMeta.yAxisLabel,
    densityOptions,
    defaultDensityStep,
    variants: Object.fromEntries(
      Object.entries(variants).map(([variantKey, variant]) => [
        variantKey,
        adaptChartVariant(metric, chartMeta.palette, variant),
      ]),
    ),
  }
}

function adaptChartVariant(metric, paletteName, variant) {
  const rawBars = variant?.bars || []
  const colors = getPalette(paletteName, rawBars.length)
  const total = metric === 'count'
    ? rawBars.reduce((sum, bar) => sum + Math.max(0, Number(bar.value) || 0), 0) || 1
    : 1

  return {
    medianLabel: variant?.median_label || null,
    bars: rawBars.map((bar, index) => ({
      label: bar.label,
      value: Number(bar.value) || 0,
      sampleSize: bar.sample_size || 0,
      attempts: bar.attempts ?? 0,
      color: colors[index],
      topLabel: buildChartTopLabel(metric, bar, total),
      secondaryLabels: buildChartSecondaryLabels(metric, bar),
    })),
  }
}

function buildChartTopLabel(metric, bar, total) {
  if (!bar.sample_size) {
    return ''
  }

  if (metric === 'count') {
    return `${formatDecimal((bar.value / total) * 100, 1)}%`
  }

  if (metric === 'roi') {
    return `${formatDecimal(bar.value, 0)}%`
  }

  return formatDecimal(bar.value, 2)
}

function buildChartSecondaryLabels(metric, bar) {
  if (!bar.sample_size) {
    return []
  }

  if (metric === 'conv' && bar.attempts) {
    return [`${formatDecimal(bar.attempts, 1)} попыток/FT`, formatSampleSize(bar.sample_size)]
  }

  return [formatSampleSize(bar.sample_size)]
}

function normalizeChartMetric(metric) {
  if (metric === 'avg_ko' || metric === 'early_avg_ko') {
    return 'avgKo'
  }
  return metric
}

function getPalette(name, length) {
  const palette = ftChartPalettes[name] || ftChartPalettes.ft
  return Array.from({ length }, (_, index) => palette[index % palette.length])
}

function formatCurrencyFromCents(cents) {
  return `$${(Number(cents) / 100).toFixed(2)}`
}

function formatInteger(value) {
  return Math.round(Number(value)).toLocaleString('en-US')
}

function formatDecimal(value, decimals = 1) {
  return Number(value).toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  })
}

function formatPercent(value, decimals = 1) {
  return `${formatDecimal(value, decimals)}%`
}

function formatSignedPercent(value, decimals = 1) {
  return `${Number(value) >= 0 ? '+' : ''}${formatDecimal(value, decimals)}%`
}

function formatSignedMoney(value) {
  const numericValue = Number(value)
  return `${numericValue >= 0 ? '+' : '-'}$${Math.abs(numericValue).toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`
}

function formatSampleSize(value) {
  return `n=${Math.round(Number(value)).toLocaleString('en-US')}`
}

function getSignedColor(value) {
  if (value > 0) {
    return 'var(--success)'
  }
  if (value < 0) {
    return 'var(--danger)'
  }
  return null
}

function getBigKoColor(card) {
  if (card.state !== 'ready') {
    return null
  }

  if (['x100', 'x1000', 'x10000'].includes(card.tier) && Number(card.count) > 0) {
    return 'var(--success)'
  }

  return null
}

function formatUnavailableValue(state) {
  return state === 'empty' ? 'Нет данных' : 'Недостаточно данных'
}

function unavailableSubtitle(state) {
  return state === 'empty' ? 'По этим фильтрам пока пусто' : 'Покрытие пока недостаточно'
}
