export default function FtStatCard({ card }) {
  if (!card) {
    return <div className="ft-stat-card ft-stat-card-empty" />
  }

  const tooltip = card.tooltip || undefined
  const deltas = Array.isArray(card.benchmarkDeltas)
    ? card.benchmarkDeltas.filter(Boolean)
    : card.benchmarkDeltas
      ? [card.benchmarkDeltas]
      : []

  return (
    <article className="ft-stat-card ft-tooltip-anchor" data-tooltip={tooltip}>
      <div className="ft-stat-card-top">
        <span className="ft-stat-card-label">{card.label}</span>
        <span
          className="ft-stat-card-value"
          style={card.valueColor ? { color: card.valueColor } : undefined}
        >
          {card.value}
        </span>
      </div>

      {card.subtitle && (
        <div className="ft-stat-card-subtitle">{card.subtitle}</div>
      )}

      {card.benchmark && (
        <div className="ft-stat-card-benchmark">
          <span className="ft-stat-card-benchmark-label">Ориентир:</span>
          <span className="ft-stat-card-benchmark-value">{card.benchmark}</span>
          {deltas.map((delta, index) => {
            const isPositive = delta.startsWith('+')
            const isNegative = delta.startsWith('-')

            return (
              <span
                key={`${card.label}-${index}`}
                className={`ft-stat-card-delta ${isPositive ? 'positive' : ''} ${isNegative ? 'negative' : ''}`}
              >
                {delta}
              </span>
            )
          })}
        </div>
      )}
    </article>
  )
}
