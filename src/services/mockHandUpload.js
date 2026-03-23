const HAND_UPLOAD_STAGES = [
  { id: 'transfer', label: 'Передача файла', startPercent: 0, endPercent: 32, ticks: 4, baseDelay: 110 },
  { id: 'validation', label: 'Проверка структуры', startPercent: 32, endPercent: 52, ticks: 3, baseDelay: 130 },
  { id: 'parsing', label: 'Парсинг раздач', startPercent: 52, endPercent: 88, ticks: 5, baseDelay: 150 },
  { id: 'indexing', label: 'Подготовка индекса', startPercent: 88, endPercent: 100, ticks: 3, baseDelay: 120 },
]

const HAND_UPLOAD_CALLBACKS = [
  { name: 'onBatchStart', description: 'Сообщает фронту о старте новой партии файлов.' },
  { name: 'onFileStart', description: 'Открывает обработку конкретного файла.' },
  { name: 'onFileStage', description: 'Меняет стадию файла, чтобы UI показывал текущий шаг.' },
  { name: 'onFileProgress', description: 'Передаёт прогресс файла внутри текущей стадии.' },
  { name: 'onBatchProgress', description: 'Синхронизирует общий прогресс всей партии.' },
  { name: 'onFileComplete', description: 'Закрывает файл со статусом готовности.' },
  { name: 'onBatchComplete', description: 'Фиксирует завершение всей партии.' },
  { name: 'onCancelled', description: 'Позволяет фронту корректно остановить сценарий.' },
]

function wait(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

function getStageProgress(stage, tick) {
  const percentStep = (stage.endPercent - stage.startPercent) / stage.ticks
  return Math.min(100, Math.round(stage.startPercent + percentStep * tick))
}

function getBatchProgress(fileIndex, totalFiles, fileProgress) {
  if (!totalFiles) {
    return 100
  }

  const completedShare = fileIndex / totalFiles
  const currentFileShare = (fileProgress / 100) * (1 / totalFiles)

  return Math.min(100, Math.round((completedShare + currentFileShare) * 100))
}

export function simulateHandUpload(files, callbacks = {}) {
  const normalizedFiles = files.map((item, index) => ({
    id: item.id ?? `file-${index + 1}`,
    file: item.file ?? item,
    name: item.name ?? item.file?.name ?? `file-${index + 1}`,
    size: item.size ?? item.file?.size ?? 0,
  }))

  let cancelled = false

  const emit = (callbackName, payload) => {
    if (callbacks[callbackName]) {
      callbacks[callbackName](payload)
    }
  }

  const run = async () => {
    emit('onBatchStart', {
      totalFiles: normalizedFiles.length,
      files: normalizedFiles,
    })

    for (let fileIndex = 0; fileIndex < normalizedFiles.length; fileIndex += 1) {
      if (cancelled) {
        break
      }

      const file = normalizedFiles[fileIndex]

      emit('onFileStart', {
        fileId: file.id,
        file,
        fileIndex,
        totalFiles: normalizedFiles.length,
      })

      for (const stage of HAND_UPLOAD_STAGES) {
        if (cancelled) {
          break
        }

        emit('onFileStage', {
          fileId: file.id,
          file,
          stageId: stage.id,
          stageLabel: stage.label,
          fileIndex,
          totalFiles: normalizedFiles.length,
        })

        for (let tick = 1; tick <= stage.ticks; tick += 1) {
          if (cancelled) {
            break
          }

          const fileProgress = getStageProgress(stage, tick)
          const batchProgress = getBatchProgress(
            fileIndex,
            normalizedFiles.length,
            fileProgress,
          )

          emit('onFileProgress', {
            fileId: file.id,
            file,
            fileIndex,
            totalFiles: normalizedFiles.length,
            stageId: stage.id,
            stageLabel: stage.label,
            stageProgress: Math.round((tick / stage.ticks) * 100),
            fileProgress,
            batchProgress,
          })

          emit('onBatchProgress', {
            totalFiles: normalizedFiles.length,
            completedFiles: fileIndex,
            currentFileId: file.id,
            batchProgress,
          })

          await wait(stage.baseDelay + Math.min(160, Math.round(file.size / 45000)))
        }
      }

      if (cancelled) {
        break
      }

      emit('onFileComplete', {
        fileId: file.id,
        file,
        fileIndex,
        totalFiles: normalizedFiles.length,
        batchProgress: getBatchProgress(fileIndex + 1, normalizedFiles.length, 0),
      })
    }

    if (cancelled) {
      emit('onCancelled', {})
      return
    }

    emit('onBatchComplete', {
      totalFiles: normalizedFiles.length,
      completedFiles: normalizedFiles.length,
      batchProgress: 100,
    })
  }

  run()

  return () => {
    cancelled = true
  }
}

export { HAND_UPLOAD_STAGES, HAND_UPLOAD_CALLBACKS }
