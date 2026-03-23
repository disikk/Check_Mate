import { useState } from 'react'
import { errorTypes } from '../data/mockData'

function ErrorCard({ error }) {
  const [expanded, setExpanded] = useState(false)
  const percent = ((error.count / error.totalHands) * 100).toFixed(1)

  return (
    <div
      className={`error-card ${expanded ? 'expanded' : ''}`}
      onClick={() => setExpanded(!expanded)}
    >
      <div className="error-card-type">{error.name}</div>
      <div className="error-card-count">{error.count}</div>
      <div className="error-card-percent">
        {percent}% от {error.totalHands} рук
      </div>
      <div className="expand-hint">{expanded ? '▲ свернуть' : '▼ подробнее'}</div>
      <div className="error-card-detail">
        {error.hands.map((hand, i) => (
          <div key={i} className="error-hand">
            <span className="error-hand-cards">{hand.cards}</span>
            <span style={{ color: 'var(--text-muted)', fontSize: '12px' }}>{hand.position}</span>
            <span className="error-hand-action">{hand.action}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

export default function ErrorSummary() {
  return (
    <div className="bento-card span-full">
      <div className="card-header">
        <span className="card-title">Сводка ошибок</span>
        <span className="card-badge badge-danger">
          {errorTypes.reduce((sum, e) => sum + e.count, 0)} всего
        </span>
      </div>
      <div className="error-cards">
        {errorTypes.map((error) => (
          <ErrorCard key={error.id} error={error} />
        ))}
      </div>
    </div>
  )
}
