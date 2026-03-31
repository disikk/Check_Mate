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

import FileDropZone from './FileDropZone'
import UploadProgress from './UploadProgress'

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
  const socketDisposeRef = useRef(null)

  const [dragActive, setDragActive] = useState(false)
  const [viewState, setViewState] = useState(initialUploadViewState)
  const [sessionInfo, setSessionInfo] = useState(null)
  const [errorMessage, setErrorMessage] = useState('')

  const { batchState, files } = viewState

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

  /* FileDropZone forwards file arrays here; empty array = clear request */
  const handleFilesSelected = (selectedFiles) => {
    if (selectedFiles.length === 0) {
      resetQueue()
    } else {
      void beginUpload(selectedFiles)
    }
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
        <FileDropZone
          onFilesSelected={handleFilesSelected}
          dragActive={dragActive}
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
          onDragOver={(event) => {
            event.preventDefault()
            setDragActive(true)
          }}
          onDrop={handleDrop}
          disabled={!files.length && batchState.status === 'idle'}
        />

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

      <UploadProgress viewState={viewState} />

      <div className="upload-helper-text">
        {errorMessage || 'Остановка server-side ещё не подключена в этом срезе.'}
      </div>
    </div>
  )
}
