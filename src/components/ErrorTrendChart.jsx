import { useState } from 'react'
import {
  LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer,
} from 'recharts'
import { trendData, errorColors, errorLabels } from '../data/mockData'

const periods = [
  { key: 'day', label: 'День' },
  { key: 'week', label: 'Неделя' },
  { key: 'month', label: 'Месяц' },
]

export default function ErrorTrendChart() {
  const [period, setPeriod] = useState('week')
  const data = trendData[period]

  return (
    <div className="bento-card span-full">
      <div className="card-header">
        <span className="card-title">Динамика ошибок</span>
        <div className="chart-controls">
          {periods.map((p) => (
            <button
              key={p.key}
              className={`chart-btn ${period === p.key ? 'active' : ''}`}
              onClick={() => setPeriod(p.key)}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>
      <ResponsiveContainer width="100%" height={320}>
        <LineChart data={data} margin={{ top: 5, right: 20, left: 0, bottom: 5 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="var(--border-subtle)" />
          <XAxis
            dataKey="date"
            tick={{ fill: 'var(--text-muted)', fontSize: 12 }}
            axisLine={{ stroke: 'var(--border-subtle)' }}
          />
          <YAxis
            tick={{ fill: 'var(--text-muted)', fontSize: 12 }}
            axisLine={{ stroke: 'var(--border-subtle)' }}
          />
          <Tooltip
            contentStyle={{
              background: 'var(--surface)',
              border: '1px solid var(--border)',
              borderRadius: 'var(--radius-xs)',
              backdropFilter: 'blur(var(--blur))',
              color: 'var(--text-primary)',
            }}
          />
          <Legend
            formatter={(value) => (
              <span style={{ color: 'var(--text-soft)', fontSize: '12px' }}>
                {errorLabels[value] || value}
              </span>
            )}
          />
          {Object.entries(errorColors).map(([key, color]) => (
            <Line
              key={key}
              type="monotone"
              dataKey={key}
              stroke={color}
              strokeWidth={2}
              dot={{ r: 3, fill: color }}
              activeDot={{ r: 5 }}
              name={key}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  )
}
