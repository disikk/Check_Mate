import { useEffect, useState } from 'react'
import Sidebar from './components/Sidebar'
import StudentDashboard from './components/StudentDashboard'
import FtAnalyticsPage from './components/FtAnalyticsPage'
import SettingsPage from './components/SettingsPage'
import UploadHandsPage from './components/UploadHandsPage'
import SectionPlaceholderPage from './components/SectionPlaceholderPage'
import { sectionById } from './navigation/sections'
import {
  readMockUserTimezone,
  writeMockUserTimezone,
} from './services/mockUserTimezone'

export default function App() {
  const [theme, setTheme] = useState(
    () => document.documentElement.getAttribute('data-theme') || 'dark',
  )
  const [activeSection, setActiveSection] = useState('dashboard')
  const [mockTimezone, setMockTimezone] = useState(() => readMockUserTimezone())

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme)
  }, [theme])

  const handleTimezoneSave = (timezoneName) => {
    writeMockUserTimezone(timezoneName)
    setMockTimezone(timezoneName)
  }

  const toggleTheme = () => {
    setTheme((currentTheme) => (currentTheme === 'dark' ? 'light' : 'dark'))
  }

  const activeSectionMeta = sectionById[activeSection] ?? sectionById.dashboard
  let activeSectionContent = <StudentDashboard />

  if (activeSection === 'ftAnalytics') {
    activeSectionContent = <FtAnalyticsPage />
  } else if (activeSection === 'upload') {
    activeSectionContent = (
      <UploadHandsPage
        timezoneName={mockTimezone}
        onOpenSettings={() => setActiveSection('settings')}
      />
    )
  } else if (activeSection === 'errors') {
    activeSectionContent = (
      <SectionPlaceholderPage
        eyebrow="Errors"
        title="Журнал ошибок"
        description="Следующим шагом сюда можно вынести разбор проблемных рук и поиск по конкретным ситуациям."
      />
    )
  } else if (activeSection === 'settings') {
    activeSectionContent = (
      <SettingsPage
        timezoneName={mockTimezone}
        onTimezoneSave={handleTimezoneSave}
      />
    )
  }

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
        {activeSectionContent}
      </main>
    </div>
  )
}
