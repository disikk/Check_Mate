import { useEffect, useState } from 'react'
import Sidebar from './components/Sidebar'
import StudentDashboard from './components/StudentDashboard'
import FtAnalyticsPage from './components/FtAnalyticsPage'
import UploadHandsPage from './components/UploadHandsPage'
import SectionPlaceholderPage from './components/SectionPlaceholderPage'
import { sectionById } from './navigation/sections'

const sectionComponents = {
  dashboard: StudentDashboard,
  ftAnalytics: FtAnalyticsPage,
  upload: UploadHandsPage,
  errors: () => (
    <SectionPlaceholderPage
      eyebrow="Errors"
      title="Журнал ошибок"
      description="Следующим шагом сюда можно вынести разбор проблемных рук и поиск по конкретным ситуациям."
    />
  ),
  settings: () => (
    <SectionPlaceholderPage
      eyebrow="Settings"
      title="Настройки"
      description="Раздел оставлен под настройки профиля, импорта и будущих интеграций."
    />
  ),
}

export default function App() {
  const [theme, setTheme] = useState(
    () => document.documentElement.getAttribute('data-theme') || 'dark',
  )
  const [activeSection, setActiveSection] = useState('dashboard')

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme)
  }, [theme])

  const toggleTheme = () => {
    setTheme((currentTheme) => (currentTheme === 'dark' ? 'light' : 'dark'))
  }

  const activeSectionMeta = sectionById[activeSection] ?? sectionById.dashboard
  const ActiveSectionComponent = sectionComponents[activeSection] ?? StudentDashboard

  return (
    <div className="app-layout">
      <Sidebar activeSection={activeSection} onNavigate={setActiveSection} />

      <div className="topbar">
        <div className="topbar-left">
          <span className="topbar-title">{activeSectionMeta.title}</span>
          <span className="topbar-subtitle">Студент / {activeSectionMeta.subtitle}</span>
        </div>
        <div className="topbar-right">
          <button className="theme-toggle" onClick={toggleTheme}>
            {theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'}
          </button>
        </div>
      </div>

      <main className="main-content">
        <ActiveSectionComponent />
      </main>
    </div>
  )
}
