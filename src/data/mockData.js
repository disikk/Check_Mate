// Моковые данные для студенческого дашборда

// 5 типов ошибок с примерами рук
export const errorTypes = [
  {
    id: 'no_vpip',
    name: 'No VPIP',
    description: 'Рука из чарта — не вошёл в игру',
    count: 47,
    totalHands: 1200,
    hands: [
      { cards: 'AQs', position: 'CO', action: 'Fold', session: '2026-03-15' },
      { cards: 'KJs', position: 'BTN', action: 'Fold', session: '2026-03-15' },
      { cards: 'TT', position: 'MP', action: 'Fold', session: '2026-03-14' },
      { cards: 'A5s', position: 'CO', action: 'Fold', session: '2026-03-14' },
      { cards: 'KQo', position: 'BTN', action: 'Fold', session: '2026-03-13' },
      { cards: '99', position: 'HJ', action: 'Fold', session: '2026-03-12' },
    ],
  },
  {
    id: 'vpip_off_chart',
    name: 'VPIP вне чарта',
    description: 'Рука НЕ из чарта — вошёл в игру',
    count: 31,
    totalHands: 1200,
    hands: [
      { cards: 'J4o', position: 'MP', action: 'Call', session: '2026-03-15' },
      { cards: '72s', position: 'CO', action: 'Raise', session: '2026-03-14' },
      { cards: 'T5o', position: 'HJ', action: 'Call', session: '2026-03-14' },
      { cards: '83o', position: 'MP', action: 'Call', session: '2026-03-13' },
      { cards: 'Q3o', position: 'BTN', action: 'Raise', session: '2026-03-12' },
    ],
  },
  {
    id: '3bet_defense',
    name: 'Защита на 3-бет',
    description: 'Ошибки при столкновении с 3-бетом',
    count: 22,
    totalHands: 1200,
    hands: [
      { cards: 'AJs', position: 'CO', action: 'Fold vs 3bet', session: '2026-03-15' },
      { cards: 'QQ', position: 'MP', action: 'Fold vs 3bet', session: '2026-03-14' },
      { cards: 'T6o', position: 'BTN', action: 'Call 3bet', session: '2026-03-14' },
      { cards: '85s', position: 'CO', action: 'Call 3bet', session: '2026-03-13' },
    ],
  },
  {
    id: 'double_check',
    name: 'Двойной чек',
    description: 'Чек флопа + чек тёрна / чек тёрна + чек ривера',
    count: 38,
    totalHands: 1200,
    hands: [
      { cards: 'A2o', position: 'BB', action: 'Check/Check flop-turn', session: '2026-03-15' },
      { cards: 'K8o', position: 'SB', action: 'Check/Check turn-river', session: '2026-03-15' },
      { cards: 'J3s', position: 'BB', action: 'Check/Check flop-turn', session: '2026-03-14' },
      { cards: 'Q5o', position: 'BB', action: 'Check/Check turn-river', session: '2026-03-13' },
      { cards: '94o', position: 'SB', action: 'Check/Check flop-turn', session: '2026-03-13' },
      { cards: 'T2o', position: 'BB', action: 'Check/Check turn-river', session: '2026-03-12' },
    ],
  },
  {
    id: 'river_check_ip',
    name: 'Чек ривера IP',
    description: 'Чек на ривере в позиции',
    count: 15,
    totalHands: 1200,
    hands: [
      { cards: 'KK', position: 'BTN', action: 'Check river IP', session: '2026-03-15' },
      { cards: 'ATo', position: 'CO', action: 'Check river IP', session: '2026-03-14' },
      { cards: 'QJs', position: 'BTN', action: 'Check river IP', session: '2026-03-13' },
    ],
  },
]

// Данные для графика динамики ошибок — помесячно, понедельно, подневно
export const trendData = {
  day: [
    { date: '10.03', no_vpip: 8, vpip_off_chart: 5, '3bet_defense': 3, double_check: 6, river_check_ip: 2 },
    { date: '11.03', no_vpip: 6, vpip_off_chart: 4, '3bet_defense': 4, double_check: 7, river_check_ip: 3 },
    { date: '12.03', no_vpip: 9, vpip_off_chart: 6, '3bet_defense': 2, double_check: 5, river_check_ip: 1 },
    { date: '13.03', no_vpip: 5, vpip_off_chart: 3, '3bet_defense': 5, double_check: 8, river_check_ip: 4 },
    { date: '14.03', no_vpip: 7, vpip_off_chart: 7, '3bet_defense': 3, double_check: 4, river_check_ip: 2 },
    { date: '15.03', no_vpip: 4, vpip_off_chart: 2, '3bet_defense': 2, double_check: 6, river_check_ip: 1 },
    { date: '16.03', no_vpip: 6, vpip_off_chart: 5, '3bet_defense': 4, double_check: 3, river_check_ip: 3 },
    { date: '17.03', no_vpip: 3, vpip_off_chart: 3, '3bet_defense': 1, double_check: 5, river_check_ip: 2 },
    { date: '18.03', no_vpip: 5, vpip_off_chart: 4, '3bet_defense': 3, double_check: 4, river_check_ip: 1 },
    { date: '19.03', no_vpip: 7, vpip_off_chart: 2, '3bet_defense': 2, double_check: 7, river_check_ip: 0 },
    { date: '20.03', no_vpip: 4, vpip_off_chart: 3, '3bet_defense': 1, double_check: 3, river_check_ip: 2 },
    { date: '21.03', no_vpip: 3, vpip_off_chart: 1, '3bet_defense': 2, double_check: 4, river_check_ip: 1 },
    { date: '22.03', no_vpip: 2, vpip_off_chart: 2, '3bet_defense': 1, double_check: 2, river_check_ip: 1 },
  ],
  week: [
    { date: 'Нед 1', no_vpip: 24, vpip_off_chart: 15, '3bet_defense': 10, double_check: 18, river_check_ip: 7 },
    { date: 'Нед 2', no_vpip: 20, vpip_off_chart: 12, '3bet_defense': 8, double_check: 15, river_check_ip: 5 },
    { date: 'Нед 3', no_vpip: 18, vpip_off_chart: 10, '3bet_defense': 11, double_check: 20, river_check_ip: 8 },
    { date: 'Нед 4', no_vpip: 15, vpip_off_chart: 14, '3bet_defense': 7, double_check: 12, river_check_ip: 4 },
    { date: 'Нед 5', no_vpip: 12, vpip_off_chart: 8, '3bet_defense': 5, double_check: 14, river_check_ip: 6 },
    { date: 'Нед 6', no_vpip: 10, vpip_off_chart: 9, '3bet_defense': 6, double_check: 10, river_check_ip: 3 },
    { date: 'Нед 7', no_vpip: 8, vpip_off_chart: 5, '3bet_defense': 4, double_check: 8, river_check_ip: 2 },
    { date: 'Нед 8', no_vpip: 5, vpip_off_chart: 3, '3bet_defense': 3, double_check: 6, river_check_ip: 2 },
  ],
  month: [
    { date: 'Янв', no_vpip: 85, vpip_off_chart: 60, '3bet_defense': 40, double_check: 70, river_check_ip: 25 },
    { date: 'Фев', no_vpip: 65, vpip_off_chart: 45, '3bet_defense': 30, double_check: 55, river_check_ip: 18 },
    { date: 'Мар', no_vpip: 47, vpip_off_chart: 31, '3bet_defense': 22, double_check: 38, river_check_ip: 15 },
  ],
}

// Цвета для линий графика (по типу ошибки)
export const errorColors = {
  no_vpip: '#f43f5e',
  vpip_off_chart: '#f59e0b',
  '3bet_defense': '#6366f1',
  double_check: '#3b82f6',
  river_check_ip: '#10b981',
}

export const errorLabels = {
  no_vpip: 'No VPIP',
  vpip_off_chart: 'VPIP вне чарта',
  '3bet_defense': 'Защита 3-бет',
  double_check: 'Двойной чек',
  river_check_ip: 'Чек ривера IP',
}

// MBR / FT статистика — раскладка 5 колонок как в ROYAL_Stats
// Каждая строка = 5 ячеек (null = пустая ячейка)
export const mbrStatsGrid = [
  // Ряд 1
  [
    { label: 'ROI', value: '+6.7%', benchmark: '+8.0%', delta: -1.3, unit: '%' },
    { label: '% достижения ФТ', value: '50.0%', benchmark: '52.1%', delta: -2.1, unit: '%' },
    { label: 'ITM', value: '20.0%', benchmark: '18.8%', delta: +1.2, unit: '%' },
    { label: 'Среднее КО за турнир', value: '0.51', subtitle: '0.97 за турнир с FT', benchmark: '0.53 / 1.01 за FT', delta: -0.02, unit: '' },
    { label: 'Попытки и успешность\nпри 1 возможном КО', value: '37.1%', subtitle: 'успешность 29.0%', benchmark: '40.6% / 27.7%', deltas: [-3.5, +1.3], unit: '%' },
  ],
  // Ряд 2
  [
    { label: 'ROI на FT', value: '+103.1%', benchmark: '+107.2%', delta: -4.1, unit: '%' },
    { label: 'Средний стек проходки FT', value: '2,057', subtitle: '2,057 фишек / 20.1 BB', benchmark: '2,067 / 20.3 BB', deltas: [-10, -0.2], unit: '' },
    { label: '% в 5MAX', value: '61.0%', benchmark: '55.7%', delta: +5.2, unit: '%' },
    { label: 'Конверсия стека в КО 7-9', value: '1.15', subtitle: '1.75 попыток за турнир с FT', benchmark: '1.31 / 2.20', deltas: [-0.16, -0.45], unit: '', subtitleLabel: 'попыток' },
    { label: 'Попытки и успешность\nпри 2 возможных КО', value: '55.8%', subtitle: 'успешность 17.9%', benchmark: '60.4% / 17.3%', deltas: [-4.6, +0.6], unit: '%' },
  ],
  // Ряд 3
  [
    { label: 'Выигрыш от КО', value: '44.1%', benchmark: '47.4%', delta: -3.3, unit: '%' },
    { label: 'Среднее место FT', value: '4.56', benchmark: '4.76', delta: -0.20, unit: '' },
    { label: 'ROI при проходке 5MAX', value: '+231.8%', benchmark: '+264.6%', delta: -32.8, unit: '%' },
    { label: 'Конверсия стека в КО 5-6', value: '1.12', subtitle: '1.94 попытки за турнир с FT', benchmark: '1.10 / 1.97', deltas: [+0.02, -0.03], unit: '', subtitleLabel: 'попыток' },
    { label: 'Попытки и успешность\nпри 3+ возможных КО', value: '83.1%', subtitle: 'успешность 12.1%', benchmark: '86.9% / 12.2%', deltas: [-3.8, -0.1], unit: '%' },
  ],
  // Ряд 4
  [
    { label: 'Выигрыш от ITM', value: '55.9%', benchmark: '52.6%', delta: +3.3, unit: '%' },
    { label: 'Среднее место', value: '8.64', benchmark: '8.91', delta: -0.27, unit: '' },
    { label: 'Стек проходки 5MAX', value: '3,551/19.2', subtitle: '3,551 фишек / 19.2 BB', benchmark: '3,788 / 20.7 BB', deltas: [-237, -1.5], unit: '' },
    { label: 'Конверсия стека в КО 3-4', value: '1.07', subtitle: '2.90 попыток за турнир с FT', benchmark: '1.14 / 3.51', deltas: [-0.07, -0.61], unit: '', subtitleLabel: 'попыток' },
    null,
  ],
]

// Данные для гистограммы распределения мест на ФТ
export const ftDistribution = [
  { place: 1, count: 520, percent: 11.5 },
  { place: 2, count: 578, percent: 12.8 },
  { place: 3, count: 549, percent: 12.1 },
  { place: 4, count: 532, percent: 11.8 },
  { place: 5, count: 575, percent: 12.7 },
  { place: 6, count: 494, percent: 10.9 },
  { place: 7, count: 452, percent: 10.0 },
  { place: 8, count: 395, percent: 8.7 },
  { place: 9, count: 430, percent: 9.5 },
]

// Общие итоги (шапка)
export const summaryStats = {
  tournaments: 8456,
  profit: '$6,056.00',
  ko: 4090.3,
  rushChips: 80,
}
