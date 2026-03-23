import { useState } from 'react'
import Sidebar from './components/Sidebar'
import StudentDashboard from './components/StudentDashboard'

export default function App() {
  const [theme, setTheme] = useState('dark')
  const [activeSection, setActiveSection] = useState('dashboard')

  const toggleTheme = () => {
    const next = theme === 'dark' ? 'light' : 'dark'
    setTheme(next)
    document.documentElement.setAttribute('data-theme', next)
  }

  // При старте ставим тёмную тему
  if (!document.documentElement.getAttribute('data-theme')) {
    document.documentElement.setAttribute('data-theme', 'dark')
  }

  return (
    <div className="app-layout">
      <Sidebar activeSection={activeSection} onNavigate={setActiveSection} />

      <div className="topbar">
        <div className="topbar-left">
          <span className="topbar-title">Личный кабинет</span>
          <span className="topbar-subtitle">Студент</span>
        </div>
        <div className="topbar-right">
          <button className="theme-toggle" onClick={toggleTheme}>
            {theme === 'dark' ? '☀️' : '🌙'} {theme === 'dark' ? 'Светлая' : 'Тёмная'}
          </button>
        </div>
      </div>

      <main className="main-content">
        {activeSection === 'dashboard' && <StudentDashboard />}
        {activeSection === 'upload' && (
          <div className="bento-card" style={{ padding: '40px', textAlign: 'center' }}>
            <h2 style={{ marginBottom: '8px' }}>Загрузка Hand History</h2>
            <p style={{ color: 'var(--text-soft)' }}>Скоро здесь будет загрузка файлов</p>
          </div>
        )}
        {activeSection === 'errors' && (
          <div className="bento-card" style={{ padding: '40px', textAlign: 'center' }}>
            <h2 style={{ marginBottom: '8px' }}>Журнал ошибок</h2>
            <p style={{ color: 'var(--text-soft)' }}>Детализированный журнал — в разработке</p>
          </div>
        )}
        {activeSection === 'settings' && (
          <div className="bento-card" style={{ padding: '40px', textAlign: 'center' }}>
            <h2 style={{ marginBottom: '8px' }}>Настройки</h2>
            <p style={{ color: 'var(--text-soft)' }}>Настройки профиля — в разработке</p>
          </div>
        )}
      </main>
    </div>
  )
}
