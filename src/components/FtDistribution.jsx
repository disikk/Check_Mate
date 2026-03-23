import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Cell, LabelList,
} from 'recharts'
import { ftDistribution } from '../data/mockData'

// Цвета столбцов — ITM позиции зелёные, остальные по убыванию
const barColors = [
  '#10b981', // 1 - зелёный (ITM)
  '#34d399', // 2
  '#6ee7b7', // 3
  '#fbbf24', // 4
  '#f59e0b', // 5
  '#fb923c', // 6
  '#f87171', // 7
  '#ef4444', // 8
  '#dc2626', // 9
]

function CustomLabel({ x, y, width, value, index }) {
  const item = ftDistribution[index]
  return (
    <text
      x={x + width / 2}
      y={y - 8}
      fill="var(--text-soft)"
      textAnchor="middle"
      fontSize={11}
    >
      {item.percent}% n={item.count}
    </text>
  )
}

export default function FtDistribution() {
  return (
    <div className="bento-card span-full">
      <div className="card-header">
        <span className="card-title">Распределение мест на финальном столе</span>
      </div>
      <ResponsiveContainer width="100%" height={300}>
        <BarChart data={ftDistribution} margin={{ top: 25, right: 20, left: 0, bottom: 5 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="var(--border-subtle)" />
          <XAxis
            dataKey="place"
            tick={{ fill: 'var(--text-muted)', fontSize: 12 }}
            axisLine={{ stroke: 'var(--border-subtle)' }}
            label={{ value: 'Место', position: 'bottom', offset: -5, fill: 'var(--text-muted)', fontSize: 12 }}
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
              color: 'var(--text-primary)',
            }}
            formatter={(value, name) => [value, 'Финишей']}
            labelFormatter={(label) => `Место: ${label}`}
          />
          <Bar dataKey="count" radius={[4, 4, 0, 0]}>
            {ftDistribution.map((entry, index) => (
              <Cell key={index} fill={barColors[index]} />
            ))}
            <LabelList content={<CustomLabel />} />
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}
