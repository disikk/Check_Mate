import { mbrStatsGrid, summaryStats } from '../data/mockData'

function StatCell({ stat }) {
  if (!stat) return <div className="stat-cell stat-cell-empty" />

  // Поддержка двойных дельт (deltas) и одиночной дельты (delta)
  const renderDelta = () => {
    if (stat.deltas) {
      return stat.deltas.map((d, i) => {
        const isPos = d >= 0
        return (
          <span key={i} className={`stat-delta ${isPos ? 'positive' : 'negative'}`}>
            {isPos ? '+' : ''}{d}{stat.unit}
          </span>
        )
      })
    }
    if (stat.delta !== undefined) {
      const isPos = stat.delta >= 0
      return (
        <span className={`stat-delta ${isPos ? 'positive' : 'negative'}`}>
          {isPos ? '+' : ''}{stat.delta}{stat.unit}
        </span>
      )
    }
    return null
  }

  return (
    <div className="stat-cell">
      <div className="stat-cell-top">
        <span className="stat-label">{stat.label}</span>
        <span className="stat-value">{stat.value}</span>
      </div>
      {stat.subtitle && (
        <div className="stat-subtitle">{stat.subtitle}</div>
      )}
      {stat.benchmark && (
        <div className="stat-benchmark">
          <span className="stat-benchmark-label">Ориентир:</span>{' '}
          <span className="stat-benchmark-value">{stat.benchmark}</span>{' '}
          {renderDelta()}
        </div>
      )}
    </div>
  )
}

export default function MbrStatsPanel() {
  return (
    <div className="bento-card span-full">
      <div className="card-header">
        <span className="card-title">MBR / FT Статистика</span>
        <div style={{ display: 'flex', gap: '16px', fontSize: '13px', color: 'var(--text-soft)' }}>
          <span>Турниров: <strong style={{ color: 'var(--text-primary)' }}>{summaryStats.tournaments}</strong></span>
          <span>Прибыль: <strong style={{ color: 'var(--success)' }}>{summaryStats.profit}</strong></span>
          <span>КО: <strong style={{ color: 'var(--text-primary)' }}>{summaryStats.ko}</strong></span>
          <span>Rush chips/T: <strong style={{ color: 'var(--text-primary)' }}>{summaryStats.rushChips}</strong></span>
        </div>
      </div>
      <div className="mbr-grid">
        {mbrStatsGrid.map((row, rowIdx) => (
          <div key={rowIdx} className="mbr-grid-row">
            {row.map((stat, colIdx) => (
              <StatCell key={colIdx} stat={stat} />
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}
