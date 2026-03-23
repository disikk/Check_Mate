export const studentSections = [
  {
    id: 'dashboard',
    icon: 'OV',
    navLabel: 'Обзор',
    title: 'Личный кабинет',
    subtitle: 'Ошибки и динамика игры',
  },
  {
    id: 'ftAnalytics',
    icon: 'FT',
    navLabel: 'FT аналитика',
    title: 'FT аналитика',
    subtitle: 'MBR / FT статистика и распределение мест',
  },
  {
    id: 'upload',
    icon: 'HH',
    navLabel: 'Загрузка рук',
    title: 'Импорт рук',
    subtitle: 'Загрузка и парсинг hand history',
  },
  {
    id: 'errors',
    icon: 'ER',
    navLabel: 'Журнал ошибок',
    title: 'Журнал ошибок',
    subtitle: 'Детализация проблемных раздач',
  },
  {
    id: 'settings',
    icon: 'ST',
    navLabel: 'Настройки',
    title: 'Настройки',
    subtitle: 'Параметры профиля и импорта',
  },
]

export const sectionById = Object.fromEntries(
  studentSections.map((section) => [section.id, section]),
)
