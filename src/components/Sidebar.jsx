import { useState } from 'react'

const navItems = [
  { id: 'dashboard', icon: '📊', label: 'Дашборд' },
  { id: 'upload', icon: '📁', label: 'Загрузка HH' },
  { id: 'errors', icon: '⚠️', label: 'Журнал ошибок' },
  { id: 'settings', icon: '⚙️', label: 'Настройки' },
]

export default function Sidebar({ activeSection, onNavigate }) {
  return (
    <aside className="sidebar">
      <div className="sidebar-logo">
        <div className="sidebar-logo-icon">CM</div>
        <span className="sidebar-logo-text">Check Mate</span>
      </div>

      <nav className="sidebar-nav">
        {navItems.map((item) => (
          <button
            key={item.id}
            className={`sidebar-item ${activeSection === item.id ? 'active' : ''}`}
            onClick={() => onNavigate(item.id)}
          >
            <span className="sidebar-item-icon">{item.icon}</span>
            {item.label}
          </button>
        ))}
      </nav>

      <div className="sidebar-user">
        <div className="sidebar-user-name">Борис</div>
        <div className="sidebar-user-role">Ученик</div>
      </div>
    </aside>
  )
}
