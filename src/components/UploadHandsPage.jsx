import { useEffect, useRef, useState } from 'react'
import {
  HAND_UPLOAD_CALLBACKS,
  HAND_UPLOAD_STAGES,
  simulateHandUpload,
} from '../services/mockHandUpload'

const initialBatchState = {
  status: 'idle',
  progress: 0,
  totalFiles: 0,
  completedFiles: 0,
  currentStage: 'Ожидание файлов',
}

const statusMeta = {
  idle: { label: 'Ожидание', tone: 'neutral' },
  queued: { label: 'В очереди', tone: 'neutral' },
  processing: { label: 'В работе', tone: 'info' },
  completed: { label: 'Готово', tone: 'success' },
  cancelled: { label: 'Остановлено', tone: 'warning' },
}

function createId(prefix) {
  return `${prefix}-${Date.now()}-${Math.round(Math.random() * 100000)}`
}

function formatFileSize(size) {
  if (size >= 1024 * 1024) {
    return `${(size / (1024 * 1024)).toFixed(1)} MB`
  }

  if (size >= 1024) {
    return `${Math.round(size / 1024)} KB`
  }

  return `${size} B`
}

function createLogEntry(message, tone = 'neutral') {
  return {
    id: createId('log'),
    time: new Date().toLocaleTimeString('ru-RU', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    }),
    message,
    tone,
  }
}

export default function UploadHandsPage() {
  const inputRef = useRef(null)
  const cancelUploadRef = useRef(null)
  const sessionRef = useRef(0)

  const [dragActive, setDragActive] = useState(false)
  const [batchState, setBatchState] = useState(initialBatchState)
  const [files, setFiles] = useState([])
  const [activityLog, setActivityLog] = useState([])

  useEffect(() => (
    () => {
      cancelUploadRef.current?.()
      sessionRef.current += 1
    }
  ), [])

  const appendLog = (message, tone = 'neutral') => {
    setActivityLog((prev) => [
      createLogEntry(message, tone),
      ...prev,
    ].slice(0, 12))
  }

  const updateFile = (fileId, patch) => {
    setFiles((prev) => prev.map((item) => (
      item.id === fileId
        ? { ...item, ...patch }
        : item
    )))
  }

  const resetQueue = () => {
    cancelUploadRef.current?.()
    cancelUploadRef.current = null
    sessionRef.current += 1
    setFiles([])
    setBatchState(initialBatchState)
    setActivityLog([createLogEntry('Очередь очищена.', 'neutral')])
  }

  const cancelUpload = () => {
    if (batchState.status !== 'processing') {
      return
    }

    cancelUploadRef.current?.()
    cancelUploadRef.current = null
    sessionRef.current += 1
    setBatchState((prev) => ({
      ...prev,
      status: 'cancelled',
      currentStage: 'Загрузка остановлена пользователем',
    }))
    setFiles((prev) => prev.map((item) => (
      item.status === 'completed'
        ? item
        : { ...item, status: 'cancelled', stageLabel: 'Остановлено', progress: item.progress }
    )))
    appendLog('Текущая загрузка остановлена.', 'warning')
  }

  const beginUpload = (selectedFiles) => {
    if (!selectedFiles.length) {
      return
    }

    cancelUploadRef.current?.()

    const sessionId = sessionRef.current + 1
    sessionRef.current = sessionId

    const nextFiles = selectedFiles.map((file, index) => ({
      id: `${file.name}-${file.lastModified}-${index}`,
      file,
      name: file.name,
      size: file.size,
      readableSize: formatFileSize(file.size),
      progress: 0,
      stageLabel: 'Ожидает обработки',
      status: 'queued',
    }))

    setFiles(nextFiles)
    setBatchState({
      status: 'processing',
      progress: 0,
      totalFiles: nextFiles.length,
      completedFiles: 0,
      currentStage: 'Подготовка пакета к загрузке',
    })
    setActivityLog([
      createLogEntry(`Новая партия: ${nextFiles.length} файл(ов).`, 'info'),
    ])

    const isActiveSession = () => sessionRef.current === sessionId

    cancelUploadRef.current = simulateHandUpload(nextFiles, {
      onBatchStart: ({ totalFiles }) => {
        if (!isActiveSession()) {
          return
        }

        appendLog(`Upload pipeline запущен для ${totalFiles} файл(ов).`, 'info')
      },
      onFileStart: ({ fileId, fileIndex, totalFiles, file }) => {
        if (!isActiveSession()) {
          return
        }

        updateFile(fileId, {
          status: 'processing',
          stageLabel: 'Файл принят в работу',
        })

        appendLog(
          `Файл ${fileIndex + 1}/${totalFiles}: ${file.name} принят в работу.`,
          'info',
        )
      },
      onFileStage: ({ fileId, stageLabel, file }) => {
        if (!isActiveSession()) {
          return
        }

        updateFile(fileId, {
          status: 'processing',
          stageLabel,
        })

        setBatchState((prev) => ({
          ...prev,
          currentStage: `${stageLabel} / ${file.name}`,
        }))
      },
      onFileProgress: ({ fileId, fileProgress, stageLabel, batchProgress }) => {
        if (!isActiveSession()) {
          return
        }

        updateFile(fileId, {
          progress: fileProgress,
          stageLabel,
        })

        setBatchState((prev) => ({
          ...prev,
          progress: batchProgress,
        }))
      },
      onBatchProgress: ({ batchProgress, completedFiles }) => {
        if (!isActiveSession()) {
          return
        }

        setBatchState((prev) => ({
          ...prev,
          progress: batchProgress,
          completedFiles,
        }))
      },
      onFileComplete: ({ fileId, file, fileIndex, totalFiles, batchProgress }) => {
        if (!isActiveSession()) {
          return
        }

        updateFile(fileId, {
          status: 'completed',
          stageLabel: 'Файл готов к импорту',
          progress: 100,
        })

        setBatchState((prev) => ({
          ...prev,
          progress: batchProgress,
          completedFiles: fileIndex + 1,
          currentStage: `Готово: ${file.name}`,
        }))

        appendLog(
          `Файл ${fileIndex + 1}/${totalFiles}: ${file.name} успешно подготовлен.`,
          'success',
        )
      },
      onBatchComplete: ({ totalFiles, completedFiles, batchProgress }) => {
        if (!isActiveSession()) {
          return
        }

        cancelUploadRef.current = null
        setBatchState({
          status: 'completed',
          progress: batchProgress,
          totalFiles,
          completedFiles,
          currentStage: 'Все файлы подготовлены к импорту',
        })
        appendLog('Вся партия готова. Фронтовый callback-flow отработал полностью.', 'success')
      },
      onCancelled: () => {
        if (!isActiveSession()) {
          return
        }

        cancelUploadRef.current = null
      },
    })
  }

  const handleInputChange = (event) => {
    beginUpload(Array.from(event.target.files ?? []))
    event.target.value = ''
  }

  const handleDrop = (event) => {
    event.preventDefault()
    setDragActive(false)
    beginUpload(Array.from(event.dataTransfer.files ?? []))
  }

  const isBusy = batchState.status === 'processing'

  return (
    <div className="page-shell">
      <section className="bento-card page-intro-card">
        <div>
          <div className="page-eyebrow">Hand history intake</div>
          <h1 className="page-heading">Загрузка рук с подготовленным callback-flow</h1>
          <p className="page-description">
            UI уже умеет принимать файлы кликом или перетаскиванием и получать
            события прогресса загрузки и парсинга. Пока используется mocked pipeline,
            чтобы фронт был готов к подключению backend.
          </p>
        </div>
        <div className="page-stat-list">
          <div className="page-stat-pill">
            <span>Текущий режим</span>
            <strong>Frontend ready</strong>
          </div>
          <div className="page-stat-pill">
            <span>Прогресс</span>
            <strong>{batchState.progress}%</strong>
          </div>
        </div>
      </section>

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
            accept=".txt,.hh,.zip,.json"
            hidden
            onChange={handleInputChange}
          />

          <div className="upload-dropzone-icon">HH</div>
          <h2 className="upload-dropzone-title">Перетащите hand history сюда</h2>
          <p className="upload-dropzone-text">
            Или откройте файловый диалог. После выбора файлов страница запускает
            mocked upload/parser pipeline и обновляет интерфейс через коллбеки.
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
            <button
              className="action-btn action-btn-danger"
              type="button"
              onClick={cancelUpload}
              disabled={!isBusy}
            >
              Остановить
            </button>
          </div>
        </section>

        <aside className="bento-card upload-contract-card">
          <div className="card-header">
            <span className="card-title">Callback контракт</span>
            <span className={`status-chip tone-${statusMeta[batchState.status]?.tone ?? 'neutral'}`}>
              {statusMeta[batchState.status]?.label ?? 'Неизвестно'}
            </span>
          </div>

          <div className="upload-contract-section">
            <div className="upload-section-label">Стадии mocked pipeline</div>
            <div className="stage-pill-list">
              {HAND_UPLOAD_STAGES.map((stage) => (
                <span key={stage.id} className="stage-pill">
                  {stage.label}
                </span>
              ))}
            </div>
          </div>

          <div className="upload-contract-section">
            <div className="upload-section-label">События для фронта</div>
            <div className="callback-list">
              {HAND_UPLOAD_CALLBACKS.map((callback) => (
                <div key={callback.name} className="callback-item">
                  <code>{callback.name}</code>
                  <span>{callback.description}</span>
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
          <span className="summary-label">Готово</span>
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
            <span className="upload-helper-text">Прогресс и статус по каждому файлу</span>
          </div>

          {!files.length && (
            <div className="empty-state">
              Выберите или перетащите файлы, чтобы увидеть живой callback-flow.
            </div>
          )}

          {!!files.length && (
            <div className="upload-file-list">
              {files.map((file) => (
                <div key={file.id} className="upload-file-item">
                  <div className="upload-file-top">
                    <div>
                      <div className="upload-file-name">{file.name}</div>
                      <div className="upload-file-meta">{file.readableSize}</div>
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
            <span className="upload-helper-text">Последние callback-события</span>
          </div>

          {!activityLog.length && (
            <div className="empty-state">
              После старта загрузки здесь появятся события pipeline.
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
