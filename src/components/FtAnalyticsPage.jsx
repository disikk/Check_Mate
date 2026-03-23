import { useState } from 'react'
import FtChartPanel from './FtChartPanel'
import FtStatCard from './FtStatCard'
import {
  createDefaultFtFilters,
  ftCardRows,
  ftFilterOptions,
  getFtAnalyticsDashboard,
} from '../data/ftAnalyticsMock'

export default function FtAnalyticsPage() {
  const [filters, setFilters] = useState(() => createDefaultFtFilters())

  const { statCards, bigKoCards, inlineStats } = getFtAnalyticsDashboard(filters)

  const updateFilter = (field, value) => {
    setFilters((currentFilters) => ({
      ...currentFilters,
      [field]: value,
    }))
  }

  return (
    <div className="page-shell ft-analytics-page">
      <section className="bento-card ft-stats-section">
        <div className="ft-section-heading">
          <div>
            <div className="page-eyebrow">FT analytics</div>
            <h1 className="ft-section-title">MBR / FT статистика</h1>
            <p className="ft-section-description">
              Student view: только свои руки, свои статы и свои фильтры. Выбор игрока и агрегирование
              по ученикам здесь не используются.
            </p>
          </div>
        </div>

        <div className="ft-filter-bar">
          <label className="ft-filter-field">
            <span>Бай-ин</span>
            <select
              value={filters.buyinFilter}
              onChange={(event) => updateFilter('buyinFilter', event.target.value ? Number(event.target.value) : '')}
            >
              <option value="">Все</option>
              {ftFilterOptions.buyins.map((buyin) => (
                <option key={buyin} value={buyin}>
                  ${buyin.toFixed(2)}
                </option>
              ))}
            </select>
          </label>

          <label className="ft-filter-field">
            <span>Сессия</span>
            <select
              value={filters.sessionId}
              onChange={(event) => updateFilter('sessionId', event.target.value)}
            >
              <option value="">Все</option>
              {ftFilterOptions.sessions.map((session) => (
                <option key={session.id} value={session.id}>
                  {session.label}
                </option>
              ))}
            </select>
          </label>

          <label className="ft-filter-field">
            <span>С</span>
            <input
              type="datetime-local"
              value={filters.dateFrom}
              min={ftFilterOptions.minDate}
              max={ftFilterOptions.maxDate}
              onChange={(event) => updateFilter('dateFrom', event.target.value)}
            />
          </label>

          <label className="ft-filter-field">
            <span>По</span>
            <input
              type="datetime-local"
              value={filters.dateTo}
              min={ftFilterOptions.minDate}
              max={ftFilterOptions.maxDate}
              onChange={(event) => updateFilter('dateTo', event.target.value)}
            />
          </label>
        </div>

        <div className="ft-stats-grid">
          {ftCardRows.map((row, rowIndex) => (
            <div key={`ft-row-${rowIndex}`} className="ft-stats-row">
              {row.map((cardKey, columnIndex) => (
                <FtStatCard
                  key={cardKey || `empty-${rowIndex}-${columnIndex}`}
                  card={cardKey ? statCards[cardKey] : null}
                />
              ))}
            </div>
          ))}
        </div>
      </section>

      <FtChartPanel
        filters={filters}
        bigKoCards={bigKoCards}
        inlineStats={inlineStats}
      />
    </div>
  )
}
