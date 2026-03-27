import { useEffect, useMemo, useState } from 'react'
import {
  QUICK_PICK_TIMEZONES,
  formatTimezonePreview,
  getTimezoneOptions,
  isValidTimezoneName,
  normalizeTimezoneName,
} from '../services/mockUserTimezone'

const timezoneOptions = getTimezoneOptions()

export default function SettingsPage({ timezoneName, onTimezoneSave }) {
  const [draftTimezone, setDraftTimezone] = useState(timezoneName ?? '')

  useEffect(() => {
    setDraftTimezone(timezoneName ?? '')
  }, [timezoneName])

  const normalizedDraft = normalizeTimezoneName(draftTimezone)
  const hasDraft = Boolean(normalizedDraft)
  const isValidDraft = hasDraft ? isValidTimezoneName(normalizedDraft) : false
  const isDirty = (timezoneName ?? '') !== (normalizedDraft ?? '')

  const previewText = useMemo(
    () => formatTimezonePreview(normalizedDraft),
    [normalizedDraft],
  )

  return (
    <div className="page-shell settings-page">
      <section className="bento-card page-intro-card">
        <div>
          <div className="page-eyebrow">Timezone contract</div>
          <h1 className="page-heading">Настройки часового пояса</h1>
          <p className="page-description">
            Это mock-flow будущей настройки профиля. Таймзона хранится локально и
            показывает, как будет выглядеть product-сценарий до подключения реального auth/API.
          </p>
        </div>
        <div className="page-stat-list">
          <div className="page-stat-pill">
            <span>Хранение</span>
            <strong>localStorage</strong>
          </div>
          <div className="page-stat-pill">
            <span>Текущий статус</span>
            <strong>{timezoneName ?? 'Не выбрано'}</strong>
          </div>
        </div>
      </section>

      <div className="settings-grid">
        <section className="bento-card settings-form-card">
          <div className="card-header">
            <span className="card-title">Профиль импорта</span>
            <span className={`status-chip tone-${timezoneName ? 'success' : 'warning'}`}>
              {timezoneName ? 'Timezone ready' : 'Нужна настройка'}
            </span>
          </div>

          <div className="settings-field">
            <label htmlFor="timezone-name">Часовой пояс (IANA)</label>
            <input
              id="timezone-name"
              name="timezone-name"
              type="text"
              list="timezone-options"
              value={draftTimezone}
              placeholder="Например, Asia/Krasnoyarsk"
              onChange={(event) => setDraftTimezone(event.target.value)}
            />
            <datalist id="timezone-options">
              {timezoneOptions.map((option) => (
                <option key={option} value={option} />
              ))}
            </datalist>
            <p className="settings-field-hint">
              Нужен именно IANA identifier. Тогда backend сможет честно переводить GG local time в UTC.
            </p>
          </div>

          <div className="settings-quick-picks">
            {QUICK_PICK_TIMEZONES.map((option) => (
              <button
                key={option}
                type="button"
                className="stage-pill"
                onClick={() => setDraftTimezone(option)}
              >
                {option}
              </button>
            ))}
          </div>

          <div className="settings-actions">
            <button
              type="button"
              className="action-btn action-btn-primary"
              disabled={!isDirty || !isValidDraft}
              onClick={() => onTimezoneSave(normalizedDraft)}
            >
              Сохранить таймзону
            </button>
            <button
              type="button"
              className="action-btn action-btn-secondary"
              disabled={!timezoneName && !draftTimezone}
              onClick={() => {
                setDraftTimezone('')
                onTimezoneSave(null)
              }}
            >
              Очистить
            </button>
          </div>

          <div className={`settings-validation ${hasDraft && !isValidDraft ? 'invalid' : 'valid'}`}>
            {hasDraft && !isValidDraft && (
              <span>Такой IANA timezone сейчас не распознается. Проверь написание.</span>
            )}
            {!hasDraft && (
              <span>Без таймзоны импорт не блокируется, но точный UTC для GG-источников не гарантируется.</span>
            )}
            {isValidDraft && (
              <span>Формат подходит. После сохранения Upload перестанет предупреждать об отсутствии таймзоны.</span>
            )}
          </div>
        </section>

        <aside className="bento-card settings-preview-card">
          <div className="card-header">
            <span className="card-title">Что это меняет</span>
          </div>

          <div className="settings-preview-stack">
            <div className="settings-preview-item">
              <span className="summary-label">Статус UTC</span>
              <strong>{timezoneName ? 'Можно вычислять' : 'Останется неопределенным'}</strong>
            </div>
            <div className="settings-preview-item">
              <span className="summary-label">Часовые агрегаты</span>
              <strong>{timezoneName ? 'Доступны' : 'Лучше не показывать'}</strong>
            </div>
            <div className="settings-preview-item">
              <span className="summary-label">Превью локального времени</span>
              <strong>{previewText ?? 'Появится после выбора валидной таймзоны'}</strong>
            </div>
          </div>

          <div className="settings-note-list">
            <div className="callback-item">
              <code>gg_user_timezone</code>
              <span>Backend сможет выставлять canonical UTC для HH и TS на основе текущей настройки пользователя.</span>
            </div>
            <div className="callback-item">
              <code>gg_user_timezone_missing</code>
              <span>Импорт все равно пройдет, но `started_at` и `hand_started_at` останутся пустыми.</span>
            </div>
          </div>
        </aside>
      </div>
    </div>
  )
}
