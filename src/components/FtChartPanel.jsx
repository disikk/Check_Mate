import { useEffect, useState } from 'react'
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
import { ftChartConfig, ftChartOptions, getFtChartData } from '../data/ftAnalyticsMock'

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
      {lines.map((line, index) => (
        <tspan key={`${point.label}-${index}`} x={x + width / 2} dy={index === 0 ? 0 : 12}>
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

export default function FtChartPanel({ filters, bigKoCards, inlineStats }) {
  const [chartType, setChartType] = useState('ft')
  const [densityStep, setDensityStep] = useState('')

  useEffect(() => {
    const config = ftChartConfig[chartType]
    if (config.densityOptions?.length) {
      setDensityStep((currentStep) => (
        config.densityOptions.includes(Number(currentStep))
          ? currentStep
          : String(config.densityOptions[0])
      ))
    } else {
      setDensityStep('')
    }
  }, [chartType])

  const chartData = getFtChartData(chartType, filters, densityStep)
  const yAxisProps = getYAxisProps(chartData.metric, chartData.bars)
  const showLabels = chartData.bars.length <= 18
  const xAxisAngle = chartData.bars.length > 14 ? -35 : 0

  return (
    <section className="bento-card ft-chart-card">
      <div className="ft-chart-toolbar">
        <div className="ft-chart-header">
          <span>{chartData.header}</span>
          {chartData.tooltip && (
            <span className="chart-tooltip-icon ft-tooltip-anchor" data-tooltip={chartData.tooltip}>
              ?
            </span>
          )}
        </div>

        <div className="ft-chart-selectors">
          <select value={chartType} onChange={(event) => setChartType(event.target.value)}>
            {ftChartOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>

          {chartData.densityOptions?.length ? (
            <select value={densityStep} onChange={(event) => setDensityStep(event.target.value)}>
              {chartData.densityOptions.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          ) : null}
        </div>
      </div>

      <div className="ft-chart-container">
        <ResponsiveContainer width="100%" height={440}>
          <BarChart data={chartData.bars} margin={{ top: 40, right: 16, left: 8, bottom: xAxisAngle ? 76 : 28 }}>
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
                value: chartData.xAxisLabel,
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
                  metric={chartData.metric}
                  xAxisLabel={chartData.xAxisLabel}
                />
              )}
            />
            {chartData.medianLabel ? (
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
              {chartData.bars.map((bar) => (
                <Cell key={`${chartType}-${bar.label}`} fill={bar.color} />
              ))}
              {showLabels ? (
                <LabelList content={(props) => <FtBarLabel {...props} bars={chartData.bars} />} />
              ) : null}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
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
