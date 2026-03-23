import ErrorSummary from './ErrorSummary'
import ErrorTrendChart from './ErrorTrendChart'

export default function StudentDashboard() {
  return (
    <div className="page-shell">
      <section className="bento-card page-intro-card">
        <div>
          <div className="page-eyebrow">Overview</div>
          <h1 className="page-heading">Основной dashboard теперь сфокусирован на ошибках</h1>
          <p className="page-description">
            FT-аналитика вынесена в отдельный раздел, поэтому здесь остались только
            сводка ошибок и их динамика.
          </p>
        </div>
      </section>

      <div className="bento-grid">
        <ErrorSummary />
        <ErrorTrendChart />
      </div>
    </div>
  )
}
