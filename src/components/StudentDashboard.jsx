import ErrorSummary from './ErrorSummary'
import ErrorTrendChart from './ErrorTrendChart'
import MbrStatsPanel from './MbrStatsPanel'
import FtDistribution from './FtDistribution'

export default function StudentDashboard() {
  return (
    <div className="bento-grid">
      <ErrorSummary />
      <ErrorTrendChart />
      <MbrStatsPanel />
      <FtDistribution />
    </div>
  )
}
