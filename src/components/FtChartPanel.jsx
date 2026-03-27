import { useEffect, useMemo, useState } from 'react'
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  LabelList,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { ftChartOptions } from '../data/ftAnalyticsConfig'

function formatChartNumber(value, decimals = 1) {
  return value.toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  })
}

function FtBarLabel({ x, y, width, index, bars }) {
  const point = typeof index === 'number' ? bars[index] : null
  const lines = point ? [point.topLabel, ...(point.secondaryLabels || [])].filter(Boolean) : []

  if (lines.length === 0) {
    return null
  }

  return (
    <text
      x={x + width / 2}
      y={y - 10}
      fill="var(--text-soft)"
      textAnchor="middle"
      fontSize={11}
    >
      {lines.map((line, lineIndex) => (
        <tspan key={`${point.label}-${lineIndex}`} x={x + width / 2} dy={lineIndex === 0 ? 0 : 12}>
          {line}
        </tspan>
      ))}
    </text>
  )
}

function FtChartTooltip({ active, payload, label, metric, xAxisLabel }) {
  if (!active || !payload || payload.length === 0) {
    return null
  }

  const point = payload[0].payload
  const lines = []

  if (metric === 'count') {
    lines.push(`Финишей: ${Math.round(point.value).toLocaleString('en-US')}`)
    if (point.topLabel) {
      lines.push(`Доля: ${point.topLabel}`)
    }
  } else if (metric === 'roi') {
    lines.push(`ROI: ${formatChartNumber(point.value, 1)}%`)
  } else if (metric === 'conv') {
    lines.push(`Конверсия: ${formatChartNumber(point.value, 2)}`)
  } else if (metric === 'avgKo') {
    lines.push(`Среднее KO: ${formatChartNumber(point.value, 2)}`)
  }

  if (point.sampleSize) {
    lines.push(`Выборка: n=${point.sampleSize}`)
  }

  if (metric === 'conv' && point.attempts) {
    lines.push(`Попыток/FT: ${formatChartNumber(point.attempts, 1)}`)
  }

  return (
    <div className="ft-chart-tooltip">
      <div className="ft-chart-tooltip-title">
        {xAxisLabel}: {label}
      </div>
      {lines.map((line) => (
        <div key={line} className="ft-chart-tooltip-line">
          {line}
        </div>
      ))}
    </div>
  )
}

function getYAxisProps(metric, bars) {
  if (metric === 'roi') {
    const values = bars.map((item) => item.value)
    const minValue = Math.min(...values, 0)
    const maxValue = Math.max(...values, 0)
    const padding = Math.max(10, (maxValue - minValue) * 0.15)

    return {
      domain: [Math.floor((minValue - padding) / 10) * 10, Math.ceil((maxValue + padding) / 10) * 10],
      tickFormatter: (value) => `${value}%`,
    }
  }

  if (metric === 'conv' || metric === 'avgKo') {
    const values = bars.map((item) => item.value)
    const minValue = Math.min(...values, 0)
    const maxValue = Math.max(...values, 0)
    const padding = Math.max(metric === 'conv' ? 0.08 : 0.12, (maxValue - minValue) * 0.15)

    return {
      domain: [Math.max(0, roundTick(minValue - padding)), roundTick(maxValue + padding)],
      tickFormatter: (value) => formatChartNumber(value, 2),
    }
  }

  return {
    domain: [0, 'auto'],
    tickFormatter: (value) => (Number.isInteger(value) ? value.toLocaleString('en-US') : ''),
  }
}

function roundTick(value) {
  return Math.round(value * 10) / 10
}

function resolveChartVariant(chart, densityStep) {
  if (!chart) {
    return null
  }

  if (!chart.densityOptions?.length) {
    return chart.variants?.default || Object.values(chart.variants || {})[0] || null
  }

  const resolvedKey = densityStep || String(chart.defaultDensityStep || chart.densityOptions[0])
  return chart.variants?.[resolvedKey] || Object.values(chart.variants || {})[0] || null
}

function getChartEmptyMessage(chart) {
  if (!chart) {
    return 'График появится после первой загрузки реальных данных.'
  }

  if (chart.state === 'empty') {
    return 'По текущим фильтрам для этого графика пока нет данных.'
  }

  if (chart.state === 'blocked') {
    return 'Недостаточно покрытия, чтобы честно построить этот график.'
  }

  return 'График обновляется.'
}

export default function FtChartPanel({ charts, bigKoCards, inlineStats }) {
  const [chartType, setChartType] = useState('ft')
  const [densityStep, setDensityStep] = useState('')

  const chart = useMemo(
    () => charts?.[chartType] || charts?.ft || null,
    [chartType, charts],
  )

  useEffect(() => {
    if (!charts?.[chartType] && charts?.ft) {
      setChartType('ft')
    }
  }, [chartType, charts])

  useEffect(() => {
    if (!chart) {
      setDensityStep('')
      return
    }

    if (chart.densityOptions?.length) {
      const normalizedOptions = chart.densityOptions.map((option) => String(option))
      setDensityStep((currentStep) => (
        normalizedOptions.includes(currentStep)
          ? currentStep
          : String(chart.defaultDensityStep || chart.densityOptions[0])
      ))
    } else {
      setDensityStep('')
    }
  }, [chart])

  const chartData = resolveChartVariant(chart, densityStep)
  const bars = chartData?.bars || []
  const yAxisProps = getYAxisProps(chart?.metric || 'count', bars)
  const showLabels = bars.length <= 18
  const xAxisAngle = bars.length > 14 ? -35 : 0

  return (
    <section className="bento-card ft-chart-card">
      <div className="ft-chart-toolbar">
        <div className="ft-chart-header">
          <span>{chart?.header || 'FT chart'}</span>
          {chart?.tooltip ? (
            <span className="chart-tooltip-icon ft-tooltip-anchor" data-tooltip={chart.tooltip}>
              ?
            </span>
          ) : null}
        </div>

        <div className="ft-chart-selectors">
          <select value={chartType} onChange={(event) => setChartType(event.target.value)}>
            {ftChartOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>

          {chart?.densityOptions?.length ? (
            <select value={densityStep} onChange={(event) => setDensityStep(event.target.value)}>
              {chart.densityOptions.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          ) : null}
        </div>
      </div>

      <div className="ft-chart-container">
        {chart?.state === 'ready' && bars.length > 0 ? (
          <ResponsiveContainer width="100%" height={440}>
            <BarChart data={bars} margin={{ top: 40, right: 16, left: 8, bottom: xAxisAngle ? 76 : 28 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border-subtle)" />
              <XAxis
                dataKey="label"
                tick={{ fill: 'var(--text-muted)', fontSize: 11 }}
                axisLine={{ stroke: 'var(--border-subtle)' }}
                tickLine={{ stroke: 'var(--border-subtle)' }}
                height={xAxisAngle ? 70 : 34}
                interval={0}
                angle={xAxisAngle}
                textAnchor={xAxisAngle ? 'end' : 'middle'}
                label={{
                  value: chart.xAxisLabel,
                  position: 'insideBottom',
                  offset: xAxisAngle ? -52 : -8,
                  fill: 'var(--text-muted)',
                  fontSize: 12,
                }}
              />
              <YAxis
                tick={{ fill: 'var(--text-muted)', fontSize: 11 }}
                axisLine={{ stroke: 'var(--border-subtle)' }}
                tickLine={{ stroke: 'var(--border-subtle)' }}
                width={72}
                {...yAxisProps}
              />
              <Tooltip
                cursor={{ fill: 'rgba(99, 102, 241, 0.08)' }}
                content={(
                  <FtChartTooltip
                    metric={chart.metric}
                    xAxisLabel={chart.xAxisLabel}
                  />
                )}
              />
              {chartData?.medianLabel ? (
                <ReferenceLine
                  x={chartData.medianLabel}
                  stroke="var(--warning)"
                  strokeDasharray="6 4"
                  label={{
                    value: `Медиана: ${chartData.medianLabel}`,
                    fill: 'var(--warning)',
                    fontSize: 11,
                    position: 'top',
                  }}
                />
              ) : null}
              <Bar dataKey="value" radius={[5, 5, 0, 0]} isAnimationActive={false}>
                {bars.map((bar) => (
                  <Cell key={`${chartType}-${bar.label}`} fill={bar.color} />
                ))}
                {showLabels ? (
                  <LabelList content={(props) => <FtBarLabel {...props} bars={bars} />} />
                ) : null}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        ) : (
          <div className="ft-chart-empty-state">
            {getChartEmptyMessage(chart)}
          </div>
        )}
      </div>

      <div className="ft-big-ko-grid">
        {bigKoCards.map((card) => (
          <div key={card.tier} className="ft-big-ko-card">
            <span className="ft-big-ko-tier">{card.tier}</span>
            <span
              className="ft-big-ko-value"
              style={card.valueColor ? { color: card.valueColor } : undefined}
            >
              {card.count}
            </span>
            <span className="ft-big-ko-subtitle">{card.subtitle}</span>
          </div>
        ))}
      </div>

      <div className="ft-inline-stat-row">
        {Object.values(inlineStats).map((stat) => (
          <div
            key={stat.label}
            className="ft-inline-stat ft-tooltip-anchor"
            data-tooltip={stat.tooltip || undefined}
          >
            <span className="ft-inline-stat-label">{stat.label}</span>
            <strong style={stat.valueColor ? { color: stat.valueColor } : undefined}>
              {stat.value}
            </strong>
          </div>
        ))}
      </div>
    </section>
  )
}
