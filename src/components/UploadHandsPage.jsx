import { useEffect, useRef, useState } from 'react'
import { formatTimezonePreview } from '../services/mockUserTimezone'

import {
  createBundleUpload,
  fetchSessionContext,
  subscribeToBundle,
} from '../services/uploadApi'
import {
  applyBundleEvent,
  applyBundleSnapshot,
  createTransferViewState,
  initialUploadViewState,
} from '../services/uploadState'

const statusMeta = {
  idle: { label: 'Ожидание', tone: 'neutral' },
  queued: { label: 'В очереди', tone: 'neutral' },
  processing: { label: 'В работе', tone: 'info' },
  completed: { label: 'Готово', tone: 'success' },
  failed: { label: 'Ошибка', tone: 'warning' },
}

const REAL_UPLOAD_STAGES = [
  'Передача файла',
  'Проверка структуры',
  'Парсинг раздач',
  'Подготовка индекса',
]

const REAL_UPLOAD_EVENTS = [
  {
    name: 'bundle_snapshot',
    description: 'Первичный снимок партии для UI сразу после подключения WebSocket.',
  },
  {
    name: 'bundle_updated',
    description: 'Обновляет общий статус, прогресс и текущую стадию партии.',
  },
  {
    name: 'file_updated',
    description: 'Обновляет статус конкретного файла или member внутри ZIP.',
  },
  {
    name: 'diagnostic_logged',
    description: 'Показывает persisted diagnostics, например пропущенные ZIP members.',
  },
  {
    name: 'bundle_terminal',
    description: 'Фиксирует финальный статус партии после `bundle_finalize`.',
  },
]

export default function UploadHandsPage({ timezoneName, onOpenSettings }) {
  const inputRef = useRef(null)
  const socketDisposeRef = useRef(null)

  const [dragActive, setDragActive] = useState(false)
  const [viewState, setViewState] = useState(initialUploadViewState)
  const [sessionInfo, setSessionInfo] = useState(null)
  const [errorMessage, setErrorMessage] = useState('')

  const { batchState, files, activityLog } = viewState

  useEffect(() => {
    let active = true

    fetchSessionContext()
      .then((session) => {
        if (active) {
          setSessionInfo(session)
        }
      })
      .catch(() => {
        if (active) {
          setSessionInfo(null)
        }
      })

    return () => {
      active = false
      socketDisposeRef.current?.()
      socketDisposeRef.current = null
    }
  }, [])

  const resetQueue = () => {
    socketDisposeRef.current?.()
    socketDisposeRef.current = null
    setErrorMessage('')
    setViewState(initialUploadViewState)
  }

  const beginUpload = async (selectedFiles) => {
    if (!selectedFiles.length) {
      return
    }

    socketDisposeRef.current?.()
    socketDisposeRef.current = null
    setErrorMessage('')
    setViewState(createTransferViewState(selectedFiles))

    try {
      const response = await createBundleUpload(selectedFiles)
      setViewState((current) => applyBundleSnapshot(current, response.snapshot))

      socketDisposeRef.current = subscribeToBundle(response.bundle_id, {
        onMessage: (message) => {
          setViewState((current) => applyBundleEvent(current, message))

          if (message.type === 'bundle_terminal') {
            socketDisposeRef.current?.()
            socketDisposeRef.current = null
          }
        },
        onError: (error) => {
          setErrorMessage(error.message)
        },
        onClose: () => {
          socketDisposeRef.current = null
        },
      })
    } catch (error) {
      setErrorMessage(error.message)
      setViewState((current) => ({
        ...current,
        batchState: {
          ...current.batchState,
          status: 'failed',
          currentStage: 'Ошибка загрузки',
        },
      }))
    }
  }

  const handleInputChange = (event) => {
    void beginUpload(Array.from(event.target.files ?? []))
    event.target.value = ''
  }

  const handleDrop = (event) => {
    event.preventDefault()
    setDragActive(false)
    void beginUpload(Array.from(event.dataTransfer.files ?? []))
  }

  const timezonePreview = timezoneName ? formatTimezonePreview(timezoneName) : null

  return (
    <div className="page-shell">
      <section className="bento-card page-intro-card">
        <div>
          <div className="page-eyebrow">Hand history intake</div>
          <h1 className="page-heading">Загрузка рук через реальный backend ingest flow</h1>
          <p className="page-description">
            Страница больше не использует mocked pipeline. Файлы уходят в Rust backend,
            партия создаётся в PostgreSQL, а прогресс и события прилетают обратно через
            WebSocket поверх persisted ingest events.
          </p>
        </div>
        <div className="page-stat-list">
          <div className="page-stat-pill">
            <span>Текущий режим</span>
            <strong>Real backend</strong>
          </div>
          <div className="page-stat-pill">
            <span>Прогресс</span>
            <strong>{batchState.progress}%</strong>
          </div>
          <div className="page-stat-pill">
            <span>Сессия</span>
            <strong>{sessionInfo?.player_screen_name ?? 'Stub session'}</strong>
          </div>
          <div className="page-stat-pill">
            <span>GG timezone</span>
            <strong>{timezoneName ?? 'Не выбрано'}</strong>
          </div>
        </div>
      </section>

      {!timezoneName && (
        <section className="bento-card upload-timezone-banner warning" role="status">
          <div>
            <div className="upload-timezone-banner-title">
              Таймзона для GG импорта пока не выбрана
            </div>
            <p className="upload-timezone-banner-text">
              Загрузка не блокируется, но без настройки профиля backend не сможет честно
              посчитать canonical UTC и часовые агрегаты.
            </p>
          </div>
          <button
            type="button"
            className="action-btn action-btn-secondary"
            onClick={onOpenSettings}
          >
            Открыть Settings
          </button>
        </section>
      )}

      {timezoneName && (
        <section className="bento-card upload-timezone-banner success" role="status">
          <div>
            <div className="upload-timezone-banner-title">
              UTC и часовые агрегаты будут доступны
            </div>
            <p className="upload-timezone-banner-text">
              Текущая mock-настройка: <strong>{timezoneName}</strong>
              {timezonePreview ? ` · ${timezonePreview}` : ''}
            </p>
          </div>
          <button
            type="button"
            className="action-btn action-btn-secondary"
            onClick={onOpenSettings}
          >
            Изменить таймзону
          </button>
        </section>
      )}

      <div className="upload-top-grid">
        <section
          className={`bento-card upload-dropzone ${dragActive ? 'dragging' : ''}`}
          onDragOver={(event) => {
            event.preventDefault()
            setDragActive(true)
          }}
          onDragEnter={(event) => {
            event.preventDefault()
            setDragActive(true)
          }}
          onDragLeave={(event) => {
            event.preventDefault()
            if (event.currentTarget === event.target) {
              setDragActive(false)
            }
          }}
          onDrop={handleDrop}
        >
          <input
            ref={inputRef}
            type="file"
            multiple
            accept=".txt,.hh,.zip"
            hidden
            onChange={handleInputChange}
          />

          <div className="upload-dropzone-icon">HH</div>
          <h2 className="upload-dropzone-title">Перетащите hand history сюда</h2>
          <p className="upload-dropzone-text">
            Поддерживаются `.txt`, `.hh` и `.zip`. ZIP может содержать mix из HH/TS,
            а неподдержанные members будут пропущены с видимыми diagnostics.
          </p>

          <div className="upload-dropzone-actions">
            <button
              className="action-btn action-btn-primary"
              type="button"
              onClick={() => inputRef.current?.click()}
            >
              Выбрать файлы
            </button>
            <button
              className="action-btn action-btn-secondary"
              type="button"
              onClick={resetQueue}
              disabled={!files.length && batchState.status === 'idle'}
            >
              Очистить
            </button>
          </div>

          <div className="upload-helper-text">
            {errorMessage || 'Остановка server-side ещё не подключена в этом срезе.'}
          </div>
        </section>

        <aside className="bento-card upload-contract-card">
          <div className="card-header">
            <span className="card-title">Runtime контракт</span>
            <span className={`status-chip tone-${statusMeta[batchState.status]?.tone ?? 'neutral'}`}>
              {statusMeta[batchState.status]?.label ?? 'Неизвестно'}
            </span>
          </div>

          <div className="upload-contract-section">
            <div className="upload-section-label">Стадии ingest flow</div>
            <div className="stage-pill-list">
              {REAL_UPLOAD_STAGES.map((stage) => (
                <span key={stage} className="stage-pill">
                  {stage}
                </span>
              ))}
            </div>
          </div>

          <div className="upload-contract-section">
            <div className="upload-section-label">События для UI</div>
            <div className="callback-list">
              {REAL_UPLOAD_EVENTS.map((event) => (
                <div key={event.name} className="callback-item">
                  <code>{event.name}</code>
                  <span>{event.description}</span>
                </div>
              ))}
            </div>
          </div>
        </aside>
      </div>

      <section className="upload-summary-grid">
        <div className="bento-card upload-summary-card">
          <span className="summary-label">Файлов в партии</span>
          <strong>{batchState.totalFiles}</strong>
        </div>
        <div className="bento-card upload-summary-card">
          <span className="summary-label">Завершено</span>
          <strong>{batchState.completedFiles}/{batchState.totalFiles}</strong>
        </div>
        <div className="bento-card upload-summary-card">
          <span className="summary-label">Общий прогресс</span>
          <strong>{batchState.progress}%</strong>
        </div>
        <div className="bento-card upload-summary-card">
          <span className="summary-label">Текущая стадия</span>
          <strong>{batchState.currentStage}</strong>
        </div>
      </section>

      <div className="upload-bottom-grid">
        <section className="bento-card upload-list-card">
          <div className="card-header">
            <span className="card-title">Очередь файлов</span>
            <span className="upload-helper-text">Живой статус по каждому ingest member</span>
          </div>

          {!files.length && (
            <div className="empty-state">
              Выберите или перетащите файлы, чтобы увидеть реальный upload/status flow.
            </div>
          )}

          {!!files.length && (
            <div className="upload-file-list">
              {files.map((file) => (
                <div key={file.id} className="upload-file-item">
                  <div className="upload-file-top">
                    <div>
                      <div className="upload-file-name">{file.name}</div>
                      <div className="upload-file-meta">
                        {file.readableSize || 'Размер уточняется после spool/scan'}
                      </div>
                    </div>
                    <span className={`status-chip tone-${statusMeta[file.status]?.tone ?? 'neutral'}`}>
                      {statusMeta[file.status]?.label ?? file.status}
                    </span>
                  </div>

                  <div className="upload-file-stage">{file.stageLabel}</div>
                  <div className="upload-progress-track">
                    <div
                      className="upload-progress-fill"
                      style={{ width: `${file.progress}%` }}
                    />
                  </div>
                  <div className="upload-file-meta">{file.progress}%</div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="bento-card upload-log-card">
          <div className="card-header">
            <span className="card-title">Журнал событий</span>
            <span className="upload-helper-text">Persisted ingest events и diagnostics</span>
          </div>

          {!activityLog.length && (
            <div className="empty-state">
              После старта загрузки здесь появятся реальные backend-события.
            </div>
          )}

          {!!activityLog.length && (
            <div className="activity-log">
              {activityLog.map((entry) => (
                <div key={entry.id} className={`activity-log-item tone-${entry.tone}`}>
                  <span className="activity-log-time">{entry.time}</span>
                  <span className="activity-log-message">{entry.message}</span>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  )
}
