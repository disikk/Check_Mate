/**
 * Upload progress display: queue summary cards + file status list + activity log.
 * Pure presentational component -- receives the full viewState from parent.
 */

const statusMeta = {
  idle: { label: 'Ожидание', tone: 'neutral' },
  queued: { label: 'В очереди', tone: 'neutral' },
  processing: { label: 'В работе', tone: 'info' },
  completed: { label: 'Готово', tone: 'success' },
  failed: { label: 'Ошибка', tone: 'warning' },
}

export default function UploadProgress({ viewState }) {
  const { batchState, files, activityLog } = viewState

  return (
    <>
      {/* Summary cards */}
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

      {/* File list + activity log */}
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
    </>
  )
}
