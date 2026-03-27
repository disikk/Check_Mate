export const ftCardRows = [
  ['roi', 'ftReach', 'itm', 'avgKo', 'koAttempts1'],
  ['roiOnFt', 'avgFtStack', 'deepFtReach', 'ftStackConv79', 'koAttempts2'],
  ['winningsFromKo', 'avgPlaceFt', 'deepFtRoi', 'ftStackConv56', 'koAttempts3p'],
  ['winningsFromItm', 'avgPlaceAll', 'deepFtStack', 'ftStackConv34', null],
]

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
    xAxisLabel: 'Место',
    yAxisLabel: 'Количество финишей',
    palette: 'ft',
  },
  pre_ft: {
    header: 'Распределение мест до финального стола (10-18)',
    tooltip: 'Показывает, где Hero чаще всего выбывает до финального стола.',
    xAxisLabel: 'Место',
    yAxisLabel: 'Количество финишей',
    palette: 'pre_ft',
  },
  all: {
    header: 'Распределение финишных мест (1-18)',
    tooltip: 'Полное распределение финишей Hero по всем местам.',
    xAxisLabel: 'Место',
    yAxisLabel: 'Количество финишей',
    palette: 'all',
  },
  ft_stack: {
    header: 'Распределение стеков выхода на FT (в фишках)',
    tooltip: 'С каким стартовым стеком Hero чаще всего выходит на FT.',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Количество выходов на FT',
    palette: 'ft_stack',
  },
  ft_stack_roi: {
    header: 'Средний ROI по стекам выхода на FT',
    tooltip: 'Средний ROI турниров в зависимости от стартового стека Hero при выходе на FT.',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Средний ROI (%)',
    palette: 'ft_stack',
  },
  ft_stack_roi_0_800: {
    header: 'Средний ROI по стекам 0-1500 фишек на FT',
    tooltip: 'Детализация коротких стеков Hero при выходе на FT в диапазоне 0-1500 фишек.',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Средний ROI (%)',
    palette: 'ft_stack',
  },
  ft_stack_conv: {
    header: 'Конверсия по стекам выхода на FT',
    tooltip: 'Эффективность конвертации стартового стека Hero в нокауты.',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Конверсия стека',
    palette: 'ft_stack',
  },
  ft_stack_conv_7_9: {
    header: 'Конверсия стека на стадии 7-9 игроков по диапазонам',
    tooltip: 'Конверсия стека Hero в KO на стадии 7-9 игроков.',
    xAxisLabel: 'Диапазон стека (фишки)',
    yAxisLabel: 'Конверсия стека',
    palette: 'stage_conv',
  },
  ft_stack_conv_5_6: {
    header: 'Конверсия стека на стадии 5-6 игроков по диапазонам',
    tooltip: 'Конверсия стека Hero в KO на стадии 5-6 игроков.',
    xAxisLabel: 'Диапазон стека (фишки)',
    yAxisLabel: 'Конверсия стека',
    palette: 'stage_conv',
  },
  ko_attempts: {
    header: 'Попытки KO за раздачу',
    tooltip: 'Сколько раз в одной раздаче у Hero возникало 1, 2, 3, 4 или 5+ попыток.',
    xAxisLabel: 'Попытки KO',
    yAxisLabel: 'Количество рук',
    palette: 'ft',
  },
  avg_ko_by_position: {
    header: 'Среднее количество KO по финишным позициям',
    tooltip: 'Сколько нокаутов в среднем делает Hero с разной итоговой позицией.',
    xAxisLabel: 'Финишная позиция',
    yAxisLabel: 'Среднее количество KO',
    palette: 'avg_ko',
  },
  avg_ko_by_ft_stack: {
    header: 'Среднее количество KO по стартовому стеку FT',
    tooltip: 'KO в среднем за турнир по стартовому стеку на FT.',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Среднее количество KO',
    palette: 'ft_stack',
  },
  avg_ko_by_early_ft_stack: {
    header: 'Среднее KO в ранней FT по стартовому стеку',
    tooltip: 'Среднее количество KO Hero только в ранней стадии FT (9-6).',
    xAxisLabel: 'Стек (фишки)',
    yAxisLabel: 'Среднее количество KO (ранняя FT)',
    palette: 'ft_stack',
  },
}

export const ftChartPalettes = {
  ft: ['#10B981', '#34D399', '#6EE7B7', '#FCD34D', '#F59E0B', '#EF4444', '#DC2626', '#B91C1C', '#991B1B'],
  pre_ft: ['#6366F1', '#3B82F6', '#0EA5E9', '#06B6D4', '#0891B2', '#14B8A6', '#0D9488', '#0F766E', '#134E4A'],
  all: ['#10B981', '#34D399', '#6EE7B7', '#14B8A6', '#0D9488', '#0F766E', '#134E4A', '#0891B2', '#06B6D4', '#0EA5E9', '#3B82F6', '#6366F1', '#FCD34D', '#F59E0B', '#FB923C', '#EF4444', '#DC2626', '#991B1B'],
  ft_stack: ['#EF4444', '#F87171', '#FB923C', '#FDBA74', '#FCD34D', '#FDE047', '#FDE68A', '#FBBF24', '#A3E635', '#84CC16', '#65A30D', '#4ADE80', '#34D399', '#10B981', '#14B8A6', '#0D9488', '#0F766E', '#134E4A', '#0891B2', '#06B6D4', '#0EA5E9', '#3B82F6', '#6366F1'],
  avg_ko: ['#10B981', '#34D399', '#6EE7B7', '#84CC16', '#FCD34D', '#F59E0B', '#FB923C', '#EF4444'],
  stage_conv: ['#EF4444', '#F59E0B', '#10B981', '#3B82F6'],
}

export const ftStatCardMeta = {
  roi: {
    label: 'ROI',
    tooltip: 'Return On Investment - средний возврат на вложенный бай-ин',
  },
  ftReach: {
    label: '% достижения FT',
  },
  itm: {
    label: 'ITM',
  },
  avgKo: {
    label: 'Среднее KO за турнир',
  },
  koAttempts1: {
    label: 'Попытки и успешность\nпри 1 возможном КО',
    tooltip: 'Метрика будет подключена после отдельного KO-attempts слоя.',
  },
  roiOnFt: {
    label: 'ROI на FT',
    tooltip: 'Средний ROI в турнирах с достижением финального стола',
  },
  avgFtStack: {
    label: 'Средний стек проходки на FT',
    tooltip: 'Средний стек Hero на старте финального стола',
  },
  deepFtReach: {
    label: '% в 5max',
    tooltip: 'Процент финалок, где Hero дошел до <=5 игроков',
  },
  ftStackConv79: {
    label: 'Конверсия стека в KO 7-9',
  },
  koAttempts2: {
    label: 'Попытки и успешность\nпри 2 возможных КО',
    tooltip: 'Метрика будет подключена после отдельного KO-attempts слоя.',
  },
  winningsFromKo: {
    label: 'Выигрыш от KO',
  },
  avgPlaceFt: {
    label: 'Среднее место FT',
  },
  deepFtRoi: {
    label: 'ROI при проходке в 5max',
  },
  ftStackConv56: {
    label: 'Конверсия стека в KO 5-6',
  },
  koAttempts3p: {
    label: 'Попытки и успешность\nпри 3+ возможных КО',
    tooltip: 'Метрика будет подключена после отдельного KO-attempts слоя.',
  },
  winningsFromItm: {
    label: 'Выигрыш от ITM',
  },
  avgPlaceAll: {
    label: 'Среднее место',
  },
  deepFtStack: {
    label: 'Стек проходки 5max',
  },
  ftStackConv34: {
    label: 'Конверсия стека в KO 3-4',
  },
}

export const ftInlineStatMeta = {
  koLuck: {
    label: 'KO Luck',
    tooltip: 'Отклонение полученных денег от нокаутов относительно среднего',
  },
  roiAdj: {
    label: 'ROI adj',
    tooltip: 'ROI с поправкой на удачу в нокаутах',
  },
}
