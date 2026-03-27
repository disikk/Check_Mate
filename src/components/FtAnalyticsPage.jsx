import {
  startTransition,
  useDeferredValue,
  useEffect,
  useState,
} from 'react'

import FtChartPanel from './FtChartPanel'
import FtStatCard from './FtStatCard'
import { ftCardRows } from '../data/ftAnalyticsConfig'
import { fetchFtDashboardSnapshot } from '../services/ftDashboardApi'
import {
  adaptFtDashboardSnapshot,
  createDefaultFtFilters,
  createInitialFtDashboardViewModel,
  getFtDashboardStateLabel,
} from '../services/ftDashboardState'

export default function FtAnalyticsPage() {
  const [filters, setFilters] = useState(() => createDefaultFtFilters())
  const [viewModel, setViewModel] = useState(() => createInitialFtDashboardViewModel())
  const [isLoading, setIsLoading] = useState(true)
  const [errorMessage, setErrorMessage] = useState(null)
  const deferredFilters = useDeferredValue(filters)

  useEffect(() => {
    const abortController = new AbortController()
    let cancelled = false

    setIsLoading(true)
    setErrorMessage(null)

    fetchFtDashboardSnapshot(deferredFilters, {
      signal: abortController.signal,
    })
      .then((snapshot) => {
        if (cancelled) {
          return
        }

        const nextViewModel = adaptFtDashboardSnapshot(snapshot)
        startTransition(() => {
          setViewModel(nextViewModel)
          setIsLoading(false)
          setErrorMessage(null)
        })
      })
      .catch((error) => {
        if (cancelled || error?.name === 'AbortError') {
          return
        }

        setErrorMessage(error.message || 'Не удалось загрузить FT dashboard')
        setIsLoading(false)
      })

    return () => {
      cancelled = true
      abortController.abort()
    }
  }, [deferredFilters])

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
            <div className="ft-section-status" aria-live="polite">
              <span>
                Состояние данных: {getFtDashboardStateLabel(viewModel.dataState)}
              </span>
              {viewModel.coverage ? (
                <span>
                  {' '}· турнирный срез {viewModel.coverage.summaryTournamentCount}/{viewModel.coverage.tournamentCount}
                </span>
              ) : null}
              {isLoading ? <span>{' '}· обновляем данные...</span> : null}
            </div>
            {errorMessage ? (
              <p className="ft-section-error" role="alert">
                {errorMessage}
              </p>
            ) : null}
          </div>
        </div>

        <div className="ft-filter-bar">
          <label className="ft-filter-field">
            <span>Бай-ин</span>
            <select
              aria-label="Бай-ин"
              value={filters.buyinFilter}
              onChange={(event) => updateFilter('buyinFilter', event.target.value ? Number(event.target.value) : '')}
            >
              <option value="">Все</option>
              {viewModel.filterOptions.buyins.map((buyin) => (
                <option key={buyin.value} value={buyin.value}>
                  {buyin.label}
                </option>
              ))}
            </select>
          </label>

          <label className="ft-filter-field">
            <span>Сессия</span>
            <select
              aria-label="Сессия"
              value={filters.sessionId}
              onChange={(event) => updateFilter('sessionId', event.target.value)}
            >
              <option value="">Все</option>
              {viewModel.filterOptions.sessions.map((session) => (
                <option key={session.id} value={session.id}>
                  {session.label}
                </option>
              ))}
            </select>
          </label>

          <label className="ft-filter-field">
            <span>С</span>
            <input
              aria-label="С"
              type="datetime-local"
              value={filters.dateFrom}
              min={viewModel.filterOptions.minDate || undefined}
              max={viewModel.filterOptions.maxDate || undefined}
              onChange={(event) => updateFilter('dateFrom', event.target.value)}
            />
          </label>

          <label className="ft-filter-field">
            <span>По</span>
            <input
              aria-label="По"
              type="datetime-local"
              value={filters.dateTo}
              min={viewModel.filterOptions.minDate || undefined}
              max={viewModel.filterOptions.maxDate || undefined}
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
                  card={cardKey ? viewModel.statCards[cardKey] : null}
                />
              ))}
            </div>
          ))}
        </div>
      </section>

      <FtChartPanel
        charts={viewModel.charts}
        bigKoCards={viewModel.bigKoCards}
        inlineStats={viewModel.inlineStats}
      />
    </div>
  )
}
