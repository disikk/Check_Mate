const MAX_ACTIVITY_LOG = 24

export const initialUploadViewState = {
  batchState: {
    status: 'idle',
    progress: 0,
    totalFiles: 0,
    completedFiles: 0,
    currentStage: 'Ожидание файлов',
  },
  files: [],
  activityLog: [],
}

export function formatFileSize(size) {
  if (size >= 1024 * 1024) {
    return `${(size / (1024 * 1024)).toFixed(1)} MB`
  }

  if (size >= 1024) {
    return `${Math.round(size / 1024)} KB`
  }

  return `${size} B`
}

function formatLogTime() {
  return new Date().toLocaleTimeString('ru-RU', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function logToneForEvent(entry) {
  if (entry.event_kind === 'bundle_terminal') {
    const status = entry.payload?.status
    if (status === 'failed' || status === 'partial_success') {
      return 'warning'
    }

    return 'success'
  }

  if (entry.event_kind === 'diagnostic_logged') {
    return 'warning'
  }

  if (entry.event_kind === 'file_updated' && entry.payload?.status === 'failed_terminal') {
    return 'warning'
  }

  return 'info'
}

function mapBatchStatus(status) {
  if (status === 'queued') {
    return 'queued'
  }

  if (status === 'running' || status === 'finalizing') {
    return 'processing'
  }

  if (status === 'failed') {
    return 'failed'
  }

  if (status === 'succeeded' || status === 'partial_success') {
    return 'completed'
  }

  return 'idle'
}

function mapFileStatus(status) {
  if (status === 'queued') {
    return 'queued'
  }

  if (status === 'running' || status === 'failed_retriable') {
    return 'processing'
  }

  if (status === 'succeeded') {
    return 'completed'
  }

  if (status === 'failed_terminal') {
    return 'failed'
  }

  return 'queued'
}

function mapActivityEntry(entry) {
  return {
    id: `event-${entry.sequence_no}`,
    time: formatLogTime(),
    message: entry.message,
    tone: logToneForEvent(entry),
  }
}

function preserveReadableSize(previousFiles, memberPath) {
  return previousFiles.find((file) => file.memberPath === memberPath || file.name === memberPath)?.readableSize ?? ''
}

function mapSnapshotFile(file, previousFiles) {
  return {
    id: file.bundle_file_id,
    bundleFileId: file.bundle_file_id,
    sourceFileId: file.source_file_id,
    sourceFileMemberId: file.source_file_member_id,
    name: file.member_path,
    memberPath: file.member_path,
    readableSize: preserveReadableSize(previousFiles, file.member_path),
    progress: file.progress_percent,
    stageLabel: file.stage_label,
    status: mapFileStatus(file.status),
    backendStatus: file.status,
    diagnostics: file.diagnostics ?? [],
  }
}

function prependActivityLog(currentLog, entry) {
  return [entry, ...currentLog.filter((item) => item.id !== entry.id)].slice(0, MAX_ACTIVITY_LOG)
}

export function createTransferViewState(selectedFiles) {
  const files = selectedFiles.map((file, index) => ({
    id: `${file.name}-${file.lastModified}-${index}`,
    name: file.name,
    memberPath: file.name,
    readableSize: formatFileSize(file.size),
    progress: 5,
    stageLabel: 'Передача файла',
    status: 'processing',
    backendStatus: 'transfer',
    diagnostics: [],
  }))

  return {
    batchState: {
      status: 'processing',
      progress: 5,
      totalFiles: files.length,
      completedFiles: 0,
      currentStage: 'Передача файла',
    },
    files,
    activityLog: [
      {
        id: `local-start-${Date.now()}`,
        time: formatLogTime(),
        message: `Новая партия: ${files.length} файл(ов) отправляется на сервер.`,
        tone: 'info',
      },
    ],
  }
}

export function applyBundleSnapshot(currentState, snapshot) {
  return {
    batchState: {
      status: mapBatchStatus(snapshot.status),
      progress: snapshot.progress_percent,
      totalFiles: snapshot.total_files,
      completedFiles: snapshot.completed_files,
      currentStage: snapshot.stage_label,
    },
    files: snapshot.files.map((file) => mapSnapshotFile(file, currentState.files)),
    activityLog: snapshot.activity_log.map(mapActivityEntry),
  }
}

function upsertFileFromPayload(files, payload) {
  const nextFile = {
    id: payload.bundle_file_id,
    bundleFileId: payload.bundle_file_id,
    sourceFileId: payload.source_file_id,
    sourceFileMemberId: payload.source_file_member_id,
    name: payload.member_path,
    memberPath: payload.member_path,
    readableSize: preserveReadableSize(files, payload.member_path),
    progress: payload.progress_percent,
    stageLabel: payload.stage_label,
    status: mapFileStatus(payload.status),
    backendStatus: payload.status,
    diagnostics: files.find((file) => file.bundleFileId === payload.bundle_file_id)?.diagnostics ?? [],
  }

  const existingIndex = files.findIndex((file) => file.bundleFileId === payload.bundle_file_id)
  if (existingIndex === -1) {
    return [...files, nextFile]
  }

  return files.map((file, index) => (index === existingIndex ? nextFile : file))
}

function applyBundlePayload(batchState, payload) {
  return {
    status: mapBatchStatus(payload.status),
    progress: payload.progress_percent,
    totalFiles: payload.total_files,
    completedFiles: payload.completed_files,
    currentStage: payload.stage_label,
  }
}

export function applyBundleEvent(currentState, message) {
  if (message.type === 'bundle_snapshot') {
    return applyBundleSnapshot(currentState, message.data)
  }

  const entry = message.data
  const nextState = {
    batchState: currentState.batchState,
    files: currentState.files,
    activityLog: prependActivityLog(currentState.activityLog, mapActivityEntry(entry)),
  }

  if (message.type === 'bundle_updated' || message.type === 'bundle_terminal') {
    nextState.batchState = applyBundlePayload(currentState.batchState, entry.payload)
  }

  if (message.type === 'file_updated') {
    nextState.files = upsertFileFromPayload(currentState.files, entry.payload)
  }

  return nextState
}
