import { describe, expect, it } from 'vitest'

import {
  applyBundleEvent,
  applyBundleSnapshot,
  createTransferViewState,
  initialUploadViewState,
} from './uploadState'

describe('uploadState adapter', () => {
  it('maps backend snapshot to stable UI state and preserves local file size', () => {
    const transferState = createTransferViewState([
      { name: 'one.txt', size: 1024, lastModified: 1 },
    ])

    const nextState = applyBundleSnapshot(transferState, {
      bundle_id: 'bundle-1',
      status: 'queued',
      progress_percent: 40,
      stage_label: 'Проверка структуры',
      total_files: 1,
      completed_files: 0,
      files: [
        {
          bundle_file_id: 'bf-1',
          source_file_id: 'sf-1',
          source_file_member_id: 'sm-1',
          member_path: 'one.txt',
          status: 'queued',
          stage_label: 'Проверка структуры',
          progress_percent: 40,
          diagnostics: [],
        },
      ],
      activity_log: [
        {
          sequence_no: 1,
          event_kind: 'bundle_updated',
          message: 'Партия файлов поставлена в очередь.',
          payload: {
            status: 'queued',
            progress_percent: 40,
            total_files: 1,
            completed_files: 0,
            stage_label: 'Проверка структуры',
          },
        },
      ],
    })

    expect(nextState.batchState).toMatchObject({
      status: 'queued',
      progress: 40,
      totalFiles: 1,
      completedFiles: 0,
      currentStage: 'Проверка структуры',
    })
    expect(nextState.files[0]).toMatchObject({
      id: 'bf-1',
      name: 'one.txt',
      readableSize: '1 KB',
      status: 'queued',
      progress: 40,
    })
    expect(nextState.activityLog[0].message).toContain('Партия файлов поставлена в очередь')
  })

  it('applies websocket updates and keeps unsupported zip diagnostics visible', () => {
    const snapshotState = applyBundleSnapshot(initialUploadViewState, {
      bundle_id: 'bundle-2',
      status: 'queued',
      progress_percent: 40,
      stage_label: 'Проверка структуры',
      total_files: 1,
      completed_files: 0,
      files: [
        {
          bundle_file_id: 'bf-2',
          source_file_id: 'sf-2',
          source_file_member_id: 'sm-2',
          member_path: 'archive/member.ts',
          status: 'queued',
          stage_label: 'Проверка структуры',
          progress_percent: 40,
          diagnostics: [],
        },
      ],
      activity_log: [
        {
          sequence_no: 5,
          event_kind: 'diagnostic_logged',
          message: 'Skipping unsupported ZIP member `notes/readme.md`',
          payload: {
            code: 'unsupported_archive_member',
            member_path: 'notes/readme.md',
          },
        },
      ],
    })

    const runningState = applyBundleEvent(snapshotState, {
      type: 'file_updated',
      data: {
        sequence_no: 6,
        event_kind: 'file_updated',
        message: 'Файл принят в работу.',
        payload: {
          bundle_file_id: 'bf-2',
          source_file_id: 'sf-2',
          source_file_member_id: 'sm-2',
          member_path: 'archive/member.ts',
          status: 'running',
          stage_label: 'Парсинг раздач',
          progress_percent: 72,
        },
      },
    })

    const terminalState = applyBundleEvent(runningState, {
      type: 'bundle_terminal',
      data: {
        sequence_no: 7,
        event_kind: 'bundle_terminal',
        message: 'Партия импортирована с ошибками.',
        payload: {
          status: 'partial_success',
          progress_percent: 100,
          total_files: 1,
          completed_files: 1,
          stage_label: 'Готово с ошибками',
        },
      },
    })

    expect(snapshotState.activityLog[0].message).toContain('unsupported ZIP member')
    expect(runningState.files[0]).toMatchObject({
      status: 'processing',
      stageLabel: 'Парсинг раздач',
      progress: 72,
    })
    expect(terminalState.batchState).toMatchObject({
      status: 'completed',
      progress: 100,
      completedFiles: 1,
      currentStage: 'Готово с ошибками',
    })
  })
})
