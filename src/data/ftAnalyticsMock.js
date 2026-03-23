const TOTAL_RANGE_START = new Date('2026-01-01T00:00:00')
const TOTAL_RANGE_END = new Date('2026-03-23T23:59:00')
const TOTAL_RANGE_MS = TOTAL_RANGE_END.getTime() - TOTAL_RANGE_START.getTime()

const SESSION_PROFILES = {
  all: {
    countScale: 1,
    roiDelta: 0,
    ftReachDelta: 0,
    itmDelta: 0,
    topHeavy: 0,
    stackShift: 0,
    aggression: 0,
    placeDelta: 0,
    phase: 0,
  },
  feb_grind: {
    countScale: 0.82,
    roiDelta: 0.5,
    ftReachDelta: 0.7,
    itmDelta: 0.3,
    topHeavy: 0.08,
    stackShift: 0.04,
    aggression: 0.03,
    placeDelta: -0.1,
    phase: 0.7,
  },
  sunday_ko: {
    countScale: 0.56,
    roiDelta: 1.7,
    ftReachDelta: 1.2,
    itmDelta: 0.1,
    topHeavy: 0.14,
    stackShift: 0.08,
    aggression: 0.08,
    placeDelta: -0.22,
    phase: 1.4,
  },
  recovery: {
    countScale: 0.64,
    roiDelta: -0.9,
    ftReachDelta: -1.1,
    itmDelta: -0.4,
    topHeavy: -0.07,
    stackShift: -0.05,
    aggression: -0.04,
    placeDelta: 0.18,
    phase: 2.1,
  },
}

const BUYIN_PROFILES = {
  all: {
    countScale: 1,
    roiDelta: 0,
    ftReachDelta: 0,
    itmDelta: 0,
    topHeavy: 0,
    stackShift: 0,
    aggression: 0,
    placeDelta: 0,
    phase: 0,
  },
  '2.5': {
    countScale: 1.16,
    roiDelta: -1.1,
    ftReachDelta: -0.4,
    itmDelta: 0.4,
    topHeavy: -0.02,
    stackShift: -0.04,
    aggression: -0.03,
    placeDelta: 0.1,
    phase: 0.45,
  },
  '5': {
    countScale: 0.95,
    roiDelta: 0,
    ftReachDelta: 0,
    itmDelta: 0,
    topHeavy: 0,
    stackShift: 0,
    aggression: 0,
    placeDelta: 0,
    phase: 0.9,
  },
  '10': {
    countScale: 0.76,
    roiDelta: 0.7,
    ftReachDelta: 0.5,
    itmDelta: -0.1,
    topHeavy: 0.04,
    stackShift: 0.03,
    aggression: 0.02,
    placeDelta: -0.06,
    phase: 1.35,
  },
  '25': {
    countScale: 0.58,
    roiDelta: 1.5,
    ftReachDelta: 0.9,
    itmDelta: -0.3,
    topHeavy: 0.08,
    stackShift: 0.06,
    aggression: 0.05,
    placeDelta: -0.14,
    phase: 1.8,
  },
}

const FT_BASE_STATS = {
  roi: 6.7,
  ftReach: 50.0,
  itm: 20.0,
  avgKoTournament: 0.51,
  avgKoFtTournament: 0.97,
  roiOnFt: 103.1,
  avgFtStackChips: 2057,
  avgFtStackBb: 20.1,
  deepFtReach: 61.0,
  ftStackConv79: 1.15,
  ftStackConv79Attempts: 1.75,
  winningsFromKo: 44.1,
  avgPlaceFt: 4.56,
  deepFtRoi: 231.8,
  ftStackConv56: 1.12,
  ftStackConv56Attempts: 1.94,
  attemptsPct1: 37.1,
  attemptsSuccess1: 29.0,
  attemptsPct2: 55.8,
  attemptsSuccess2: 17.9,
  attemptsPct3p: 83.1,
  attemptsSuccess3p: 12.1,
  winningsFromItm: 55.9,
  avgPlaceAll: 8.64,
  deepFtStackChips: 3551,
  deepFtStackBb: 19.2,
  ftStackConv34: 1.07,
  ftStackConv34Attempts: 2.90,
  koLuck: 186.4,
  roiAdj: 7.9,
  totalKnockouts: 4090.3,
}

const FT_BENCHMARKS = {
  roi: 8.0,
  ftReach: 52.1,
  itm: 18.8,
  avgKoTournament: 0.53,
  avgKoFtTournament: 1.01,
  roiOnFt: 107.2,
  avgFtStackChips: 2067,
  avgFtStackBb: 20.3,
  deepFtReach: 55.7,
  ftStackConv79: 1.31,
  ftStackConv79Attempts: 2.20,
  winningsFromKo: 47.4,
  avgPlaceFt: 4.76,
  deepFtRoi: 264.6,
  ftStackConv56: 1.10,
  ftStackConv56Attempts: 1.97,
  attemptsPct1: 40.6,
  attemptsSuccess1: 27.7,
  attemptsPct2: 60.4,
  attemptsSuccess2: 17.3,
  attemptsPct3p: 86.9,
  attemptsSuccess3p: 12.2,
  winningsFromItm: 52.6,
  avgPlaceAll: 8.91,
  deepFtStackChips: 3788,
  deepFtStackBb: 20.7,
  ftStackConv34: 1.14,
  ftStackConv34Attempts: 3.51,
}

const BASE_FT_COUNTS = [520, 578, 549, 532, 575, 494, 452, 395, 430]
const BASE_PRE_FT_COUNTS = [612, 587, 563, 541, 518, 497, 472, 449, 426]
const BASE_KO_ATTEMPTS = [
  { label: '1', value: 904, weight: -0.24 },
  { label: '2', value: 612, weight: -0.05 },
  { label: '3', value: 322, weight: 0.12 },
  { label: '4', value: 128, weight: 0.24 },
  { label: '5+', value: 54, weight: 0.38 },
]
const BASE_AVG_KO_BY_POSITION = [
  { label: '1', value: 2.63, sampleSize: 520 },
  { label: '2', value: 2.28, sampleSize: 578 },
  { label: '3', value: 2.07, sampleSize: 549 },
  { label: '4', value: 1.78, sampleSize: 532 },
  { label: '5', value: 1.55, sampleSize: 575 },
  { label: '6', value: 1.37, sampleSize: 494 },
  { label: '7', value: 1.14, sampleSize: 452 },
  { label: '8', value: 0.98, sampleSize: 395 },
]
const BASE_STAGE_CONV_79 = [
  { label: '500-1200', value: 0.89, attempts: 1.42, sampleSize: 214 },
  { label: '1200-1800', value: 1.03, attempts: 1.71, sampleSize: 301 },
  { label: '1800-3000', value: 1.27, attempts: 2.03, sampleSize: 284 },
  { label: '3000+', value: 1.46, attempts: 2.28, sampleSize: 162 },
]
const BASE_STAGE_CONV_56 = [
  { label: '500-1200', value: 0.82, attempts: 1.27, sampleSize: 188 },
  { label: '1200-1800', value: 0.94, attempts: 1.54, sampleSize: 272 },
  { label: '1800-3000', value: 1.11, attempts: 1.86, sampleSize: 236 },
  { label: '3000+', value: 1.25, attempts: 2.08, sampleSize: 141 },
]
const BASE_BIG_KO_COUNTS = {
  'x1.5': 1870,
  x2: 1104,
  x10: 162,
  x100: 24,
  x1000: 4,
  x10000: 1,
}

const FT_PALETTES = {
  ft: ['#10B981', '#34D399', '#6EE7B7', '#FCD34D', '#F59E0B', '#EF4444', '#DC2626', '#B91C1C', '#991B1B'],
  pre_ft: ['#6366F1', '#3B82F6', '#0EA5E9', '#06B6D4', '#0891B2', '#14B8A6', '#0D9488', '#0F766E', '#134E4A'],
  all: ['#10B981', '#34D399', '#6EE7B7', '#14B8A6', '#0D9488', '#0F766E', '#134E4A', '#0891B2', '#06B6D4', '#0EA5E9', '#3B82F6', '#6366F1', '#FCD34D', '#F59E0B', '#FB923C', '#EF4444', '#DC2626', '#991B1B'],
  ft_stack: ['#EF4444', '#F87171', '#FB923C', '#FDBA74', '#FCD34D', '#FDE047', '#FDE68A', '#FBBF24', '#A3E635', '#84CC16', '#65A30D', '#4ADE80', '#34D399', '#10B981', '#14B8A6', '#0D9488', '#0F766E', '#134E4A', '#0891B2', '#06B6D4', '#0EA5E9', '#3B82F6', '#6366F1'],
  avg_ko: ['#10B981', '#34D399', '#6EE7B7', '#84CC16', '#FCD34D', '#F59E0B', '#FB923C', '#EF4444'],
  stage_conv: ['#EF4444', '#F59E0B', '#10B981', '#3B82F6'],
}

export const ftCardRows = [
  ['roi', 'ftReach', 'itm', 'avgKo', 'koAttempts1'],
  ['roiOnFt', 'avgFtStack', 'deepFtReach', 'ftStackConv79', 'koAttempts2'],
  ['winningsFromKo', 'avgPlaceFt', 'deepFtRoi', 'ftStackConv56', 'koAttempts3p'],
  ['winningsFromItm', 'avgPlaceAll', 'deepFtStack', 'ftStackConv34', null],
]

export const ftFilterOptions = {
  buyins: [2.5, 5, 10, 25],
  sessions: [
    { id: 'feb_grind', label: 'Февральский гринд' },
    { id: 'sunday_ko', label: 'Sunday KO' },
    { id: 'recovery', label: 'Late Reg Recovery' },
  ],
  minDate: '2026-01-01T00:00',
  maxDate: '2026-03-23T23:59',
}

export const ftChartOptions = [
  { label: 'Финальный стол', value: 'ft' },
  { label: 'До финального стола', value: 'pre_ft' },
  { label: 'Все места', value: 'all' },
  { label: 'Стек FT (фишки)', value: 'ft_stack' },
  { label: 'ROI по стекам FT', value: 'ft_stack_roi' },
  { label: 'ROI по стекам 0-1500', value: 'ft_stack_roi_0_800' },
  { label: 'Конверсия по стекам FT', value: 'ft_stack_conv' },
  { label: 'Конверсия 7-9 игроков по стекам', value: 'ft_stack_conv_7_9' },
  { label: 'Конверсия 5-6 игроков по стекам', value: 'ft_stack_conv_5_6' },
  { label: 'Попытки KO', value: 'ko_attempts' },
  { label: 'Среднее KO по позициям', value: 'avg_ko_by_position' },
  { label: 'Среднее KO по стекам FT', value: 'avg_ko_by_ft_stack' },
  { label: 'Среднее KO ранней FT по стекам', value: 'avg_ko_by_early_ft_stack' },
]

export const ftChartConfig = {
  ft: {
    header: 'Распределение финишных мест на финальном столе',
    tooltip: 'Показывает, как часто Hero занимает каждое место на финальном столе.',
    densityOptions: null,
    metric: 'count',
    xAxisLabel: 'Место',
    yAxisLabel: 'Количество финишей',
    palette: 'ft',
  },
  pre_ft: {
    header: 'Распределение мест до финального стола (10-18)',
    tooltip: 'Показывает, где Hero чаще всего выбывает до финального стола.',
    densityOptions: null,
    metric: 'count',
    xAxisLabel: 'Место',
    yAxisLabel: 'Количество финишей',
    palette: 'pre_ft',
  },
  all: {
    header: 'Распределение финишных мест (1-18)',
    tooltip: 'Полное распределение финишей Hero по всем местам.',
    densityOptions: null,
    metric: 'count',
    xAxisLabel: 'Место',
    yAxisLabel: 'Количество финишей',
    palette: 'all',
  },
  ft_stack: {
    header: 'Распределение стеков выхода на FT (в фишках)',
    tooltip: 'С каким стартовым стеком Hero чаще всего выходит на FT.',
    densityOptions: [100, 200, 400, 1000],
    metric: 'count',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Количество выходов на FT',
    palette: 'ft_stack',
    showMedian: true,
  },
  ft_stack_roi: {
    header: 'Средний ROI по стекам выхода на FT',
    tooltip: 'Средний ROI турниров в зависимости от стартового стека Hero при выходе на FT.',
    densityOptions: [100, 200, 400, 1000],
    metric: 'roi',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Средний ROI (%)',
    palette: 'ft_stack',
  },
  ft_stack_roi_0_800: {
    header: 'Средний ROI по стекам 0-1500 фишек на FT',
    tooltip: 'Детализация коротких стеков Hero при выходе на FT в диапазоне 0-1500 фишек.',
    densityOptions: [50, 100],
    metric: 'roi',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Средний ROI (%)',
    palette: 'ft_stack',
  },
  ft_stack_conv: {
    header: 'Конверсия по стекам выхода на FT',
    tooltip: 'Эффективность конвертации стартового стека Hero в нокауты.',
    densityOptions: [100, 200, 400, 1000],
    metric: 'conv',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Конверсия стека',
    palette: 'ft_stack',
  },
  ft_stack_conv_7_9: {
    header: 'Конверсия стека на стадии 7-9 игроков по диапазонам',
    tooltip: 'Конверсия стека Hero в KO на стадии 7-9 игроков.',
    densityOptions: null,
    metric: 'conv',
    xAxisLabel: 'Диапазон стека (фишки)',
    yAxisLabel: 'Конверсия стека',
    palette: 'stage_conv',
  },
  ft_stack_conv_5_6: {
    header: 'Конверсия стека на стадии 5-6 игроков по диапазонам',
    tooltip: 'Конверсия стека Hero в KO на стадии 5-6 игроков.',
    densityOptions: null,
    metric: 'conv',
    xAxisLabel: 'Диапазон стека (фишки)',
    yAxisLabel: 'Конверсия стека',
    palette: 'stage_conv',
  },
  ko_attempts: {
    header: 'Попытки KO за раздачу',
    tooltip: 'Сколько раз в одной раздаче у Hero возникало 1, 2, 3, 4 или 5+ попыток.',
    densityOptions: null,
    metric: 'count',
    xAxisLabel: 'Попытки KO',
    yAxisLabel: 'Количество рук',
    palette: 'ft',
  },
  avg_ko_by_position: {
    header: 'Среднее количество KO по финишным позициям',
    tooltip: 'Сколько нокаутов в среднем делает Hero с разной итоговой позицией.',
    densityOptions: null,
    metric: 'avgKo',
    xAxisLabel: 'Финишная позиция',
    yAxisLabel: 'Среднее количество KO',
    palette: 'avg_ko',
  },
  avg_ko_by_ft_stack: {
    header: 'Среднее количество KO по стартовому стеку FT',
    tooltip: 'KO в среднем за турнир по стартовому стеку на FT.',
    densityOptions: [100, 200, 400, 1000],
    metric: 'avgKo',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Среднее количество KO',
    palette: 'ft_stack',
    showMedian: true,
  },
  avg_ko_by_early_ft_stack: {
    header: 'Среднее KO в ранней FT по стартовому стеку',
    tooltip: 'Среднее количество KO Hero только в ранней стадии FT (9-6).',
    densityOptions: [100, 200, 400, 1000],
    metric: 'avgKo',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Среднее количество KO (ранняя FT)',
    palette: 'ft_stack',
    showMedian: true,
  },
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function roundTo(value, decimals = 1) {
  const factor = 10 ** decimals
  return Math.round(value * factor) / factor
}

function formatRange(start, end) {
  const startLabel = start >= 1000 ? `${(start / 1000).toFixed(1).replace(/\.0$/, '')}k` : `${start}`
  const endLabel = end >= 1000 ? `${(end / 1000).toFixed(1).replace(/\.0$/, '')}k` : `${end}`
  return `${startLabel}-${endLabel}`
}

function buildFtIntervals(step) {
  const intervals = [{ label: '≤800', min: 0, max: 800 }]
  let current = 800
  while (current < 4000) {
    const nextBoundary = current + step
    if (nextBoundary > 4000) {
      break
    }
    intervals.push({
      label: formatRange(current, nextBoundary),
      min: current,
      max: nextBoundary,
    })
    current = nextBoundary
  }
  intervals.push({ label: '≥4k', min: 4000, max: Number.POSITIVE_INFINITY })
  return intervals
}

function buildShortIntervals(step) {
  const intervals = []
  let current = 0
  while (current < 1500) {
    const nextBoundary = current + step
    intervals.push({
      label: `${current}-${nextBoundary}`,
      min: current,
      max: nextBoundary,
    })
    current = nextBoundary
  }
  return intervals
}

function formatInteger(value) {
  return Math.round(value).toLocaleString('en-US')
}

function formatDecimal(value, decimals = 1) {
  return value.toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  })
}

function formatPercent(value, decimals = 1) {
  return `${formatDecimal(value, decimals)}%`
}

function formatSignedPercent(value, decimals = 1) {
  return `${value >= 0 ? '+' : ''}${formatDecimal(value, decimals)}%`
}

function formatSignedMoney(value) {
  const sign = value >= 0 ? '+' : '-'
  return `${sign}$${Math.abs(value).toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`
}

function formatDelta(value, decimals = 1, suffix = '', useThousands = false) {
  const rounded = roundTo(value, decimals)
  const sign = rounded > 0 ? '+' : rounded < 0 ? '-' : ''
  const absolute = Math.abs(rounded).toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
    useGrouping: useThousands,
  })
  return `${sign}${absolute}${suffix}`
}

function formatSampleSize(value) {
  if (Number.isInteger(value)) {
    return `n=${value}`
  }
  return `n=${formatDecimal(value, 1)}`
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

function normalizeFilters(filters) {
  return {
    sessionId: filters?.sessionId || '',
    buyinFilter: filters?.buyinFilter === '' || filters?.buyinFilter == null
      ? ''
      : Number(filters.buyinFilter),
    dateFrom: filters?.dateFrom || ftFilterOptions.minDate,
    dateTo: filters?.dateTo || ftFilterOptions.maxDate,
  }
}

function resolveDateRange(filters) {
  const normalized = normalizeFilters(filters)
  const rawStart = new Date(normalized.dateFrom)
  const rawEnd = new Date(normalized.dateTo)
  const safeStart = Number.isNaN(rawStart.getTime()) ? TOTAL_RANGE_START : rawStart
  const safeEnd = Number.isNaN(rawEnd.getTime()) ? TOTAL_RANGE_END : rawEnd
  const start = safeStart.getTime() <= safeEnd.getTime() ? safeStart : safeEnd
  const end = safeStart.getTime() <= safeEnd.getTime() ? safeEnd : safeStart

  return {
    ...normalized,
    start,
    end,
  }
}

function buildProfile(filters) {
  const { sessionId, buyinFilter, start, end } = resolveDateRange(filters)
  const sessionProfile = SESSION_PROFILES[sessionId || 'all']
  const buyinProfile = BUYIN_PROFILES[buyinFilter ? String(buyinFilter) : 'all']
  const coverage = clamp((end.getTime() - start.getTime()) / TOTAL_RANGE_MS, 0.04, 1)
  const coverageFactor = 0.25 + 0.75 * Math.sqrt(coverage)
  const midpoint = start.getTime() + (end.getTime() - start.getTime()) / 2
  const midpointRatio = clamp((midpoint - TOTAL_RANGE_START.getTime()) / TOTAL_RANGE_MS, 0, 1)
  const recencyBias = (midpointRatio - 0.5) * 1.2

  return {
    countScale: sessionProfile.countScale * buyinProfile.countScale * coverageFactor,
    roiDelta: sessionProfile.roiDelta + buyinProfile.roiDelta + recencyBias,
    ftReachDelta: sessionProfile.ftReachDelta + buyinProfile.ftReachDelta + recencyBias * 0.55,
    itmDelta: sessionProfile.itmDelta + buyinProfile.itmDelta + recencyBias * 0.2,
    topHeavy: sessionProfile.topHeavy + buyinProfile.topHeavy + recencyBias * 0.03,
    stackShift: sessionProfile.stackShift + buyinProfile.stackShift + recencyBias * 0.02,
    aggression: sessionProfile.aggression + buyinProfile.aggression + recencyBias * 0.025,
    placeDelta: sessionProfile.placeDelta + buyinProfile.placeDelta - recencyBias * 0.08,
    phase: sessionProfile.phase + buyinProfile.phase + coverage * 2,
  }
}

function scaleCount(value, profile, weight = 0) {
  const multiplier = clamp(1 + weight, 0.58, 1.48)
  return Math.max(0, Math.round(value * profile.countScale * multiplier))
}

function createStackBuckets() {
  const buckets = [
    {
      label: '≤800',
      min: 0,
      max: 800,
      count: 116,
      roi: 84.2,
      conv: 0.82,
      attempts: 1.22,
      avgKo: 0.74,
      earlyAvgKo: 0.27,
    },
  ]

  for (let start = 800; start < 4000; start += 100) {
    const end = start + 100
    const center = start + 50
    const normalized = (center - 800) / 3200
    const gaussian = Math.exp(-((center - 2150) ** 2) / (2 * 860 ** 2))
    const shoulder = Math.exp(-((center - 1450) ** 2) / (2 * 1040 ** 2))

    buckets.push({
      label: formatRange(start, end),
      min: start,
      max: end,
      count: Math.round(18 + 86 * gaussian + 18 * shoulder + 6 * Math.sin(center / 260)),
      roi: roundTo(88 + normalized * 55 + 9 * Math.sin(center / 470), 1),
      conv: roundTo(0.84 + normalized * 0.7 + 0.06 * Math.cos(center / 390), 2),
      attempts: roundTo(1.24 + normalized * 0.96 + 0.07 * Math.sin(center / 330), 2),
      avgKo: roundTo(0.78 + normalized * 0.86 + 0.05 * Math.cos(center / 360), 2),
      earlyAvgKo: roundTo(0.29 + normalized * 0.47 + 0.04 * Math.sin(center / 340), 2),
    })
  }

  buckets.push({
    label: '≥4k',
    min: 4000,
    max: Number.POSITIVE_INFINITY,
    count: 72,
    roi: 152.8,
    conv: 1.69,
    attempts: 2.26,
    avgKo: 1.72,
    earlyAvgKo: 0.82,
  })

  return buckets
}

function createShortRoiBuckets() {
  const buckets = []
  for (let start = 0; start < 1500; start += 50) {
    const end = start + 50
    const center = start + 25
    const normalized = center / 1500
    const gaussian = Math.exp(-((center - 950) ** 2) / (2 * 280 ** 2))

    buckets.push({
      label: `${start}-${end}`,
      min: start,
      max: end,
      count: Math.round(5 + 10 * gaussian + 3 * Math.sin(center / 150)),
      roi: roundTo(28 + normalized * 105 + 8 * Math.sin(center / 170), 1),
    })
  }
  return buckets
}

const BASE_STACK_BUCKETS = createStackBuckets()
const BASE_SHORT_ROI_BUCKETS = createShortRoiBuckets()

function getBucketMidpoint(bucket) {
  if (bucket.max === Number.POSITIVE_INFINITY) {
    return 4300
  }
  if (bucket.min === 0 && bucket.max === 800) {
    return 600
  }
  return (bucket.min + bucket.max) / 2
}

function transformStackBucket(bucket, profile) {
  const midpoint = getBucketMidpoint(bucket)
  const normalized = clamp((midpoint - 2000) / 1800, -1, 1)
  const count = scaleCount(bucket.count, profile, normalized * profile.stackShift * 0.8 + profile.topHeavy * 0.08)

  return {
    ...bucket,
    count,
    roi: roundTo(bucket.roi + profile.roiDelta + normalized * profile.stackShift * 22, 1),
    conv: roundTo(bucket.conv * (1 + profile.aggression * 0.42 + normalized * profile.stackShift * 0.22), 2),
    attempts: roundTo(bucket.attempts * (1 + profile.aggression * 0.24 + normalized * profile.stackShift * 0.12), 2),
    avgKo: roundTo(bucket.avgKo * (1 + profile.aggression * 0.3 + normalized * profile.topHeavy * 0.15), 2),
    earlyAvgKo: roundTo(bucket.earlyAvgKo * (1 + profile.aggression * 0.24 + normalized * profile.topHeavy * 0.12), 2),
  }
}

function transformShortRoiBucket(bucket, profile) {
  const normalized = clamp((bucket.max - 750) / 750, -1, 1)
  return {
    ...bucket,
    count: scaleCount(bucket.count, profile, normalized * profile.stackShift * 0.4),
    roi: roundTo(bucket.roi + profile.roiDelta * 0.9 + normalized * profile.stackShift * 10, 1),
  }
}

function aggregateStackBuckets(profile, step, metricKey) {
  const buckets = BASE_STACK_BUCKETS.map((bucket) => transformStackBucket(bucket, profile))
  const intervals = buildFtIntervals(step)

  return intervals.map((interval) => {
    const matching = buckets.filter((bucket) => {
      if (interval.label === '≤800') {
        return bucket.min === 0 && bucket.max === 800
      }
      if (interval.label === '≥4k') {
        return bucket.min >= 4000
      }
      return bucket.min >= interval.min && bucket.max <= interval.max
    })

    const sampleSize = matching.reduce((sum, bucket) => sum + bucket.count, 0)
    const totalWeight = sampleSize || 1
    const weightedMetric = matching.reduce((sum, bucket) => sum + bucket[metricKey] * bucket.count, 0) / totalWeight
    const weightedAttempts = matching.reduce((sum, bucket) => sum + bucket.attempts * bucket.count, 0) / totalWeight

    return {
      label: interval.label,
      value: metricKey === 'count' ? sampleSize : roundTo(weightedMetric, metricKey === 'roi' ? 1 : 2),
      sampleSize,
      attempts: roundTo(weightedAttempts, 2),
    }
  })
}

function aggregateShortRoiBuckets(profile, step) {
  const buckets = BASE_SHORT_ROI_BUCKETS.map((bucket) => transformShortRoiBucket(bucket, profile))
  const intervals = buildShortIntervals(step)

  return intervals.map((interval) => {
    const matching = buckets.filter((bucket) => bucket.min >= interval.min && bucket.max <= interval.max)
    const sampleSize = matching.reduce((sum, bucket) => sum + bucket.count, 0)
    const totalWeight = sampleSize || 1
    const weightedRoi = matching.reduce((sum, bucket) => sum + bucket.roi * bucket.count, 0) / totalWeight

    return {
      label: interval.label,
      value: roundTo(weightedRoi, 1),
      sampleSize,
      attempts: 0,
    }
  })
}

function resolveMedianLabel(step, medianValue) {
  const intervals = buildFtIntervals(step)
  const match = intervals.find((interval) => medianValue >= interval.min && medianValue < interval.max)
  return match ? match.label : '≥4k'
}

function getPalette(name, length) {
  const palette = FT_PALETTES[name] || FT_PALETTES.ft
  return Array.from({ length }, (_, index) => palette[index % palette.length])
}

function buildCountBars(baseCounts, labels, profile, weightMapper, paletteName) {
  const bars = baseCounts.map((count, index) => ({
    label: labels[index],
    value: scaleCount(count, profile, weightMapper(index)),
    sampleSize: 0,
    attempts: 0,
  }))
  const colors = getPalette(paletteName, bars.length)
  const total = bars.reduce((sum, item) => sum + item.value, 0) || 1

  return bars.map((bar, index) => ({
    ...bar,
    color: colors[index],
    sampleSize: bar.value,
    topLabel: bar.value > 0 ? `${formatDecimal((bar.value / total) * 100, 1)}%` : '',
    secondaryLabels: bar.value > 0 ? [formatSampleSize(bar.value)] : [],
  }))
}

function finalizeMetricBars(bars, metric, paletteName) {
  const colors = getPalette(paletteName, bars.length)

  return bars.map((bar, index) => {
    const sampleSizeLabel = bar.sampleSize > 0 ? formatSampleSize(bar.sampleSize) : ''
    let topLabel = ''
    let secondaryLabels = []

    if (metric === 'roi') {
      topLabel = sampleSizeLabel ? `${formatDecimal(bar.value, 0)}%` : ''
      secondaryLabels = sampleSizeLabel ? [sampleSizeLabel] : []
    } else if (metric === 'conv') {
      topLabel = sampleSizeLabel ? formatDecimal(bar.value, 2) : ''
      secondaryLabels = sampleSizeLabel
        ? [
          bar.attempts > 0 ? `${formatDecimal(bar.attempts, 1)} попыток/FT` : '',
          sampleSizeLabel,
        ].filter(Boolean)
        : []
    } else if (metric === 'avgKo') {
      topLabel = sampleSizeLabel ? formatDecimal(bar.value, 2) : ''
      secondaryLabels = sampleSizeLabel ? [sampleSizeLabel] : []
    }

    return {
      ...bar,
      color: colors[index],
      topLabel,
      secondaryLabels,
    }
  })
}

function buildFtChartCountData(profile) {
  return buildCountBars(
    BASE_FT_COUNTS,
    Array.from({ length: 9 }, (_, index) => `${index + 1}`),
    profile,
    (index) => ((4 - index) / 5) * profile.topHeavy + 0.03 * Math.sin(index + profile.phase),
    'ft',
  )
}

function buildPreFtChartCountData(profile) {
  return buildCountBars(
    BASE_PRE_FT_COUNTS,
    Array.from({ length: 9 }, (_, index) => `${index + 10}`),
    profile,
    (index) => ((4 - index) / 5) * profile.topHeavy * 0.75 + 0.02 * Math.cos(index + profile.phase),
    'pre_ft',
  )
}

function buildAllPlacesChartData(profile) {
  const ftBars = buildFtChartCountData(profile)
  const preFtBars = buildPreFtChartCountData(profile)
  const combined = [...ftBars, ...preFtBars]
  const total = combined.reduce((sum, item) => sum + item.value, 0) || 1
  const colors = getPalette('all', combined.length)

  return combined.map((bar, index) => ({
    ...bar,
    color: colors[index],
    topLabel: bar.value > 0 ? `${formatDecimal((bar.value / total) * 100, 1)}%` : '',
    secondaryLabels: bar.value > 0 ? [formatSampleSize(bar.value)] : [],
  }))
}

function buildKoAttemptsChartData(profile) {
  const bars = BASE_KO_ATTEMPTS.map((item) => ({
    label: item.label,
    value: scaleCount(item.value, profile, item.weight * (0.8 + profile.aggression * 2.2)),
    sampleSize: 0,
    attempts: 0,
  }))
  const total = bars.reduce((sum, item) => sum + item.value, 0) || 1
  const colors = getPalette('ft', bars.length)

  return bars.map((bar, index) => ({
    ...bar,
    color: colors[index],
    sampleSize: bar.value,
    topLabel: bar.value > 0 ? `${formatDecimal((bar.value / total) * 100, 1)}%` : '',
    secondaryLabels: bar.value > 0 ? [formatSampleSize(bar.value)] : [],
  }))
}

function buildAvgKoByPositionChartData(profile) {
  const bars = BASE_AVG_KO_BY_POSITION.map((item, index) => {
    const positionWeight = (4 - index) / 5
    const value = roundTo(item.value * (1 + profile.aggression * 0.18 + positionWeight * profile.topHeavy * 0.22), 2)
    const sampleSize = scaleCount(item.sampleSize, profile, positionWeight * profile.topHeavy * 0.5)

    return {
      label: item.label,
      value,
      sampleSize,
      attempts: 0,
    }
  })

  return finalizeMetricBars(bars, 'avgKo', 'avg_ko')
}

function buildStageConvChartData(profile, baseData, paletteName) {
  const bars = baseData.map((item, index) => {
    const rangeWeight = (index - 1.5) / 2
    const value = roundTo(item.value * (1 + profile.aggression * 0.2 + rangeWeight * profile.stackShift * 0.35), 2)
    const attempts = roundTo(item.attempts * (1 + profile.aggression * 0.12 + rangeWeight * profile.stackShift * 0.16), 2)
    const sampleSize = scaleCount(item.sampleSize, profile, rangeWeight * profile.stackShift * 0.4)

    return {
      label: item.label,
      value,
      attempts,
      sampleSize,
    }
  })

  return finalizeMetricBars(bars, 'conv', paletteName)
}

function buildInlineStats(profile) {
  const koLuck = roundTo(FT_BASE_STATS.koLuck + profile.aggression * 210 + profile.roiDelta * 34, 2)
  const roiAdj = roundTo(FT_BASE_STATS.roiAdj + profile.roiDelta * 0.6 + profile.topHeavy * 5, 1)

  return {
    koLuck: {
      label: 'KO Luck',
      value: formatSignedMoney(koLuck),
      valueColor: getSignedColor(koLuck),
      tooltip: 'Отклонение полученных денег от нокаутов относительно среднего',
    },
    roiAdj: {
      label: 'ROI adj',
      value: formatSignedPercent(roiAdj),
      valueColor: getSignedColor(roiAdj),
      tooltip: 'ROI с поправкой на удачу в нокаутах',
    },
  }
}

function getBigKoColor(tier, totalKnockouts, count) {
  if (tier === 'x10') {
    if (count <= 0 || totalKnockouts <= 0) {
      return null
    }
    const ratio = totalKnockouts / count
    if (ratio <= 25) {
      return 'var(--success)'
    }
    if (ratio >= 34) {
      return 'var(--danger)'
    }
  }

  if (['x100', 'x1000', 'x10000'].includes(tier) && count > 0) {
    return 'var(--success)'
  }

  return null
}

function buildBigKoCards(profile) {
  const totalKnockouts = Math.max(1, FT_BASE_STATS.totalKnockouts * profile.countScale)

  return Object.entries(BASE_BIG_KO_COUNTS).map(([tier, count]) => {
    const tierMultiplier = tier === 'x1.5'
      ? 1 + profile.aggression * 0.05
      : tier === 'x2'
        ? 1 + profile.aggression * 0.08
        : tier === 'x10'
          ? 1 + profile.aggression * 0.25 + profile.topHeavy * 0.14
          : 1 + profile.topHeavy * 0.24 + profile.roiDelta * 0.03

    const nextCount = Math.max(0, Math.round(count * profile.countScale * clamp(tierMultiplier, 0.6, 1.7)))
    const ratio = nextCount > 0 ? totalKnockouts / nextCount : 0

    return {
      tier,
      count: nextCount,
      subtitle: nextCount > 0 ? `1 на ${Math.max(1, Math.round(ratio))} нокаутов` : 'пока нет',
      valueColor: getBigKoColor(tier, totalKnockouts, nextCount),
    }
  })
}

function createCard({ label, value, subtitle = null, tooltip = null, benchmark = null, benchmarkDeltas = [], valueColor = null }) {
  return {
    label,
    value,
    subtitle,
    tooltip,
    benchmark,
    benchmarkDeltas,
    valueColor,
  }
}

function buildFtStatCards(profile) {
  const roi = roundTo(FT_BASE_STATS.roi + profile.roiDelta, 1)
  const ftReach = roundTo(FT_BASE_STATS.ftReach + profile.ftReachDelta, 1)
  const itm = roundTo(FT_BASE_STATS.itm + profile.itmDelta, 1)
  const avgKoTournament = roundTo(FT_BASE_STATS.avgKoTournament * (1 + profile.aggression * 0.16), 2)
  const avgKoFtTournament = roundTo(FT_BASE_STATS.avgKoFtTournament * (1 + profile.aggression * 0.15 + profile.topHeavy * 0.05), 2)
  const roiOnFt = roundTo(FT_BASE_STATS.roiOnFt + profile.roiDelta * 2.4 + profile.topHeavy * 6, 1)
  const avgFtStackChips = Math.round(FT_BASE_STATS.avgFtStackChips * (1 + profile.stackShift * 0.12))
  const avgFtStackBb = roundTo(FT_BASE_STATS.avgFtStackBb * (1 + profile.stackShift * 0.08), 1)
  const deepFtReach = roundTo(FT_BASE_STATS.deepFtReach + profile.ftReachDelta * 0.85 + profile.topHeavy * 8, 1)
  const ftStackConv79 = roundTo(FT_BASE_STATS.ftStackConv79 * (1 + profile.aggression * 0.14 + profile.stackShift * 0.1), 2)
  const ftStackConv79Attempts = roundTo(FT_BASE_STATS.ftStackConv79Attempts * (1 + profile.aggression * 0.12 + profile.stackShift * 0.05), 2)
  const winningsFromKo = roundTo(clamp(FT_BASE_STATS.winningsFromKo + profile.topHeavy * 16 + profile.aggression * 7, 35, 65), 1)
  const winningsFromItm = roundTo(100 - winningsFromKo, 1)
  const avgPlaceFt = roundTo(FT_BASE_STATS.avgPlaceFt + profile.placeDelta, 2)
  const deepFtRoi = roundTo(FT_BASE_STATS.deepFtRoi + profile.roiDelta * 12 + profile.topHeavy * 38, 1)
  const ftStackConv56 = roundTo(FT_BASE_STATS.ftStackConv56 * (1 + profile.aggression * 0.12 + profile.stackShift * 0.08), 2)
  const ftStackConv56Attempts = roundTo(FT_BASE_STATS.ftStackConv56Attempts * (1 + profile.aggression * 0.1 + profile.stackShift * 0.04), 2)
  const attemptsPct1 = roundTo(clamp(FT_BASE_STATS.attemptsPct1 + profile.aggression * 14, 20, 70), 1)
  const attemptsSuccess1 = roundTo(clamp(FT_BASE_STATS.attemptsSuccess1 + profile.roiDelta * 0.45 + profile.topHeavy * 7, 15, 45), 1)
  const attemptsPct2 = roundTo(clamp(FT_BASE_STATS.attemptsPct2 + profile.aggression * 12, 35, 80), 1)
  const attemptsSuccess2 = roundTo(clamp(FT_BASE_STATS.attemptsSuccess2 + profile.roiDelta * 0.3 + profile.topHeavy * 4, 10, 28), 1)
  const attemptsPct3p = roundTo(clamp(FT_BASE_STATS.attemptsPct3p + profile.aggression * 8, 60, 95), 1)
  const attemptsSuccess3p = roundTo(clamp(FT_BASE_STATS.attemptsSuccess3p + profile.roiDelta * 0.2 + profile.topHeavy * 2, 7, 18), 1)
  const avgPlaceAll = roundTo(FT_BASE_STATS.avgPlaceAll + profile.placeDelta * 0.7, 2)
  const deepFtStackChips = Math.round(FT_BASE_STATS.deepFtStackChips * (1 + profile.stackShift * 0.1))
  const deepFtStackBb = roundTo(FT_BASE_STATS.deepFtStackBb * (1 + profile.stackShift * 0.08), 1)
  const ftStackConv34 = roundTo(FT_BASE_STATS.ftStackConv34 * (1 + profile.aggression * 0.1 + profile.stackShift * 0.06), 2)
  const ftStackConv34Attempts = roundTo(FT_BASE_STATS.ftStackConv34Attempts * (1 + profile.aggression * 0.08 + profile.stackShift * 0.03), 2)

  return {
    roi: createCard({
      label: 'ROI',
      value: formatSignedPercent(roi),
      tooltip: 'Return On Investment - средний возврат на вложенный бай-ин',
      benchmark: formatSignedPercent(FT_BENCHMARKS.roi),
      benchmarkDeltas: [formatDelta(roi - FT_BENCHMARKS.roi, 1, '%')],
      valueColor: getSignedColor(roi),
    }),
    ftReach: createCard({
      label: '% достижения FT',
      value: formatPercent(ftReach),
      benchmark: formatPercent(FT_BENCHMARKS.ftReach),
      benchmarkDeltas: [formatDelta(ftReach - FT_BENCHMARKS.ftReach, 1, '%')],
    }),
    itm: createCard({
      label: 'ITM',
      value: formatPercent(itm),
      benchmark: formatPercent(FT_BENCHMARKS.itm),
      benchmarkDeltas: [formatDelta(itm - FT_BENCHMARKS.itm, 1, '%')],
    }),
    avgKo: createCard({
      label: 'Среднее KO за турнир',
      value: formatDecimal(avgKoTournament, 2),
      subtitle: `${formatDecimal(avgKoFtTournament, 2)} за турнир с FT`,
      benchmark: `${formatDecimal(FT_BENCHMARKS.avgKoTournament, 2)} / ${formatDecimal(FT_BENCHMARKS.avgKoFtTournament, 2)} за FT`,
      benchmarkDeltas: [
        formatDelta(avgKoTournament - FT_BENCHMARKS.avgKoTournament, 2),
        formatDelta(avgKoFtTournament - FT_BENCHMARKS.avgKoFtTournament, 2, ' за FT'),
      ],
    }),
    koAttempts1: createCard({
      label: 'Попытки и успешность\nпри 1 возможном КО',
      value: formatPercent(attemptsPct1),
      subtitle: `успешность ${formatPercent(attemptsSuccess1)}`,
      tooltip: 'Попытки: доля раздач, где Hero использовал возможность взять 1 KO.\nУспешность: KO / попытки.',
      benchmark: `${formatPercent(FT_BENCHMARKS.attemptsPct1)} / ${formatPercent(FT_BENCHMARKS.attemptsSuccess1)}`,
      benchmarkDeltas: [
        formatDelta(attemptsPct1 - FT_BENCHMARKS.attemptsPct1, 1, '%'),
        formatDelta(attemptsSuccess1 - FT_BENCHMARKS.attemptsSuccess1, 1, '%'),
      ],
    }),
    roiOnFt: createCard({
      label: 'ROI на FT',
      value: formatSignedPercent(roiOnFt),
      tooltip: 'Средний ROI в турнирах с достижением финального стола',
      benchmark: formatSignedPercent(FT_BENCHMARKS.roiOnFt),
      benchmarkDeltas: [formatDelta(roiOnFt - FT_BENCHMARKS.roiOnFt, 1, '%')],
      valueColor: getSignedColor(roiOnFt),
    }),
    avgFtStack: createCard({
      label: 'Средний стек проходки на FT',
      value: formatInteger(avgFtStackChips),
      subtitle: `${formatInteger(avgFtStackChips)} фишек / ${formatDecimal(avgFtStackBb, 1)} BB`,
      tooltip: 'Средний стек Hero на старте финального стола',
      benchmark: `${formatInteger(FT_BENCHMARKS.avgFtStackChips)} / ${formatDecimal(FT_BENCHMARKS.avgFtStackBb, 1)} BB`,
      benchmarkDeltas: [
        formatDelta(avgFtStackChips - FT_BENCHMARKS.avgFtStackChips, 0, '', true),
        formatDelta(avgFtStackBb - FT_BENCHMARKS.avgFtStackBb, 1, ' BB'),
      ],
    }),
    deepFtReach: createCard({
      label: '% в 5max',
      value: formatPercent(deepFtReach),
      tooltip: 'Процент финалок, где Hero дошел до ≤5 игроков',
      benchmark: formatPercent(FT_BENCHMARKS.deepFtReach),
      benchmarkDeltas: [formatDelta(deepFtReach - FT_BENCHMARKS.deepFtReach, 1, '%')],
    }),
    ftStackConv79: createCard({
      label: 'Конверсия стека в KO 7-9',
      value: formatDecimal(ftStackConv79, 2),
      subtitle: `${formatDecimal(ftStackConv79Attempts, 2)} попыток за турнир с FT`,
      tooltip: 'Конверсия стека в KO на стадии 7-9 игроков',
      benchmark: `${formatDecimal(FT_BENCHMARKS.ftStackConv79, 2)} / ${formatDecimal(FT_BENCHMARKS.ftStackConv79Attempts, 2)} попыток`,
      benchmarkDeltas: [
        formatDelta(ftStackConv79 - FT_BENCHMARKS.ftStackConv79, 2),
        formatDelta(ftStackConv79Attempts - FT_BENCHMARKS.ftStackConv79Attempts, 2, ' попыток'),
      ],
    }),
    koAttempts2: createCard({
      label: 'Попытки и успешность\nпри 2 возможных КО',
      value: formatPercent(attemptsPct2),
      subtitle: `успешность ${formatPercent(attemptsSuccess2)}`,
      tooltip: 'Попытки: доля раздач, где Hero использовал возможность взять 2 KO.\nУспешность: KO / попытки.',
      benchmark: `${formatPercent(FT_BENCHMARKS.attemptsPct2)} / ${formatPercent(FT_BENCHMARKS.attemptsSuccess2)}`,
      benchmarkDeltas: [
        formatDelta(attemptsPct2 - FT_BENCHMARKS.attemptsPct2, 1, '%'),
        formatDelta(attemptsSuccess2 - FT_BENCHMARKS.attemptsSuccess2, 1, '%'),
      ],
    }),
    winningsFromKo: createCard({
      label: 'Выигрыш от KO',
      value: formatPercent(winningsFromKo),
      benchmark: formatPercent(FT_BENCHMARKS.winningsFromKo),
      benchmarkDeltas: [formatDelta(winningsFromKo - FT_BENCHMARKS.winningsFromKo, 1, '%')],
    }),
    avgPlaceFt: createCard({
      label: 'Среднее место FT',
      value: formatDecimal(avgPlaceFt, 2),
      benchmark: formatDecimal(FT_BENCHMARKS.avgPlaceFt, 2),
      benchmarkDeltas: [formatDelta(avgPlaceFt - FT_BENCHMARKS.avgPlaceFt, 2)],
    }),
    deepFtRoi: createCard({
      label: 'ROI при проходке в 5max',
      value: formatSignedPercent(deepFtRoi),
      tooltip: 'ROI в турнирах, где Hero дошел до стадии ≤5 игроков',
      benchmark: formatSignedPercent(FT_BENCHMARKS.deepFtRoi),
      benchmarkDeltas: [formatDelta(deepFtRoi - FT_BENCHMARKS.deepFtRoi, 1, '%')],
      valueColor: getSignedColor(deepFtRoi),
    }),
    ftStackConv56: createCard({
      label: 'Конверсия стека в KO 5-6',
      value: formatDecimal(ftStackConv56, 2),
      subtitle: `${formatDecimal(ftStackConv56Attempts, 2)} попыток за турнир с FT`,
      tooltip: 'Конверсия стека в KO на стадии 5-6 игроков',
      benchmark: `${formatDecimal(FT_BENCHMARKS.ftStackConv56, 2)} / ${formatDecimal(FT_BENCHMARKS.ftStackConv56Attempts, 2)} попыток`,
      benchmarkDeltas: [
        formatDelta(ftStackConv56 - FT_BENCHMARKS.ftStackConv56, 2),
        formatDelta(ftStackConv56Attempts - FT_BENCHMARKS.ftStackConv56Attempts, 2, ' попыток'),
      ],
    }),
    koAttempts3p: createCard({
      label: 'Попытки и успешность\nпри 3+ возможных КО',
      value: formatPercent(attemptsPct3p),
      subtitle: `успешность ${formatPercent(attemptsSuccess3p)}`,
      tooltip: 'Попытки: доля раздач, где Hero использовал возможность взять 3+ KO.\nУспешность: KO / попытки.',
      benchmark: `${formatPercent(FT_BENCHMARKS.attemptsPct3p)} / ${formatPercent(FT_BENCHMARKS.attemptsSuccess3p)}`,
      benchmarkDeltas: [
        formatDelta(attemptsPct3p - FT_BENCHMARKS.attemptsPct3p, 1, '%'),
        formatDelta(attemptsSuccess3p - FT_BENCHMARKS.attemptsSuccess3p, 1, '%'),
      ],
    }),
    winningsFromItm: createCard({
      label: 'Выигрыш от ITM',
      value: formatPercent(winningsFromItm),
      tooltip: 'Доля выигрыша от попадания в призы (места 1-3)',
      benchmark: formatPercent(FT_BENCHMARKS.winningsFromItm),
      benchmarkDeltas: [formatDelta(winningsFromItm - FT_BENCHMARKS.winningsFromItm, 1, '%')],
    }),
    avgPlaceAll: createCard({
      label: 'Среднее место',
      value: formatDecimal(avgPlaceAll, 2),
      benchmark: formatDecimal(FT_BENCHMARKS.avgPlaceAll, 2),
      benchmarkDeltas: [formatDelta(avgPlaceAll - FT_BENCHMARKS.avgPlaceAll, 2)],
    }),
    deepFtStack: createCard({
      label: 'Стек проходки 5max',
      value: `${formatInteger(deepFtStackChips)}/${formatDecimal(deepFtStackBb, 1)}`,
      tooltip: 'Средний стек проходки в стадию ≤5 игроков на финальном столе',
      benchmark: `${formatInteger(FT_BENCHMARKS.deepFtStackChips)} / ${formatDecimal(FT_BENCHMARKS.deepFtStackBb, 1)} BB`,
      benchmarkDeltas: [
        formatDelta(deepFtStackChips - FT_BENCHMARKS.deepFtStackChips, 0, '', true),
        formatDelta(deepFtStackBb - FT_BENCHMARKS.deepFtStackBb, 1, ' BB'),
      ],
    }),
    ftStackConv34: createCard({
      label: 'Конверсия стека в KO 3-4',
      value: formatDecimal(ftStackConv34, 2),
      subtitle: `${formatDecimal(ftStackConv34Attempts, 2)} попыток за турнир с FT`,
      tooltip: 'Конверсия стека в KO на стадии 3-4 игрока',
      benchmark: `${formatDecimal(FT_BENCHMARKS.ftStackConv34, 2)} / ${formatDecimal(FT_BENCHMARKS.ftStackConv34Attempts, 2)} попыток`,
      benchmarkDeltas: [
        formatDelta(ftStackConv34 - FT_BENCHMARKS.ftStackConv34, 2),
        formatDelta(ftStackConv34Attempts - FT_BENCHMARKS.ftStackConv34Attempts, 2, ' попыток'),
      ],
    }),
  }
}

export function createDefaultFtFilters() {
  return {
    sessionId: '',
    buyinFilter: '',
    dateFrom: ftFilterOptions.minDate,
    dateTo: ftFilterOptions.maxDate,
  }
}

export function getFtAnalyticsDashboard(filters) {
  const profile = buildProfile(filters)

  return {
    statCards: buildFtStatCards(profile),
    bigKoCards: buildBigKoCards(profile),
    inlineStats: buildInlineStats(profile),
  }
}

export function getFtChartData(chartType, filters, requestedStep) {
  const profile = buildProfile(filters)
  const config = ftChartConfig[chartType] || ftChartConfig.ft
  const densityOptions = config.densityOptions || []
  const step = densityOptions.length > 0 && densityOptions.includes(Number(requestedStep))
    ? Number(requestedStep)
    : densityOptions[0] || null

  if (chartType === 'ft') {
    return {
      ...config,
      densityStep: step,
      bars: buildFtChartCountData(profile),
    }
  }

  if (chartType === 'pre_ft') {
    return {
      ...config,
      densityStep: step,
      bars: buildPreFtChartCountData(profile),
    }
  }

  if (chartType === 'all') {
    return {
      ...config,
      densityStep: step,
      bars: buildAllPlacesChartData(profile),
    }
  }

  if (chartType === 'ft_stack') {
    const bars = finalizeMetricBars(aggregateStackBuckets(profile, step, 'count'), 'count', config.palette)
    const total = bars.reduce((sum, item) => sum + item.value, 0) || 1

    return {
      ...config,
      densityStep: step,
      medianLabel: resolveMedianLabel(step, 2057 * (1 + profile.stackShift * 0.1)),
      bars: bars.map((bar) => ({
        ...bar,
        topLabel: bar.value > 0 ? `${formatDecimal((bar.value / total) * 100, 1)}%` : '',
        secondaryLabels: bar.value > 0 ? [formatSampleSize(bar.sampleSize)] : [],
      })),
    }
  }

  if (chartType === 'ft_stack_roi') {
    return {
      ...config,
      densityStep: step,
      bars: finalizeMetricBars(aggregateStackBuckets(profile, step, 'roi'), 'roi', config.palette),
    }
  }

  if (chartType === 'ft_stack_roi_0_800') {
    return {
      ...config,
      densityStep: step,
      bars: finalizeMetricBars(aggregateShortRoiBuckets(profile, step), 'roi', config.palette),
    }
  }

  if (chartType === 'ft_stack_conv') {
    return {
      ...config,
      densityStep: step,
      bars: finalizeMetricBars(aggregateStackBuckets(profile, step, 'conv'), 'conv', config.palette),
    }
  }

  if (chartType === 'ft_stack_conv_7_9') {
    return {
      ...config,
      densityStep: step,
      bars: buildStageConvChartData(profile, BASE_STAGE_CONV_79, config.palette),
    }
  }

  if (chartType === 'ft_stack_conv_5_6') {
    return {
      ...config,
      densityStep: step,
      bars: buildStageConvChartData(profile, BASE_STAGE_CONV_56, config.palette),
    }
  }

  if (chartType === 'ko_attempts') {
    return {
      ...config,
      densityStep: step,
      bars: buildKoAttemptsChartData(profile),
    }
  }

  if (chartType === 'avg_ko_by_position') {
    return {
      ...config,
      densityStep: step,
      bars: buildAvgKoByPositionChartData(profile),
    }
  }

  if (chartType === 'avg_ko_by_ft_stack') {
    return {
      ...config,
      densityStep: step,
      medianLabel: resolveMedianLabel(step, 2057 * (1 + profile.stackShift * 0.1)),
      bars: finalizeMetricBars(aggregateStackBuckets(profile, step, 'avgKo'), 'avgKo', config.palette),
    }
  }

  if (chartType === 'avg_ko_by_early_ft_stack') {
    return {
      ...config,
      densityStep: step,
      medianLabel: resolveMedianLabel(step, 2057 * (1 + profile.stackShift * 0.1)),
      bars: finalizeMetricBars(aggregateStackBuckets(profile, step, 'earlyAvgKo'), 'avgKo', config.palette),
    }
  }

  return {
    ...ftChartConfig.ft,
    densityStep: null,
    bars: buildFtChartCountData(profile),
  }
}
