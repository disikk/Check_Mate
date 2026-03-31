import { useRef } from 'react'

/**
 * Drag-and-drop zone + file input UI for hand history uploads.
 * Stateless: all file selection is forwarded to the parent via onFilesSelected.
 */
export default function FileDropZone({
  onFilesSelected,
  dragActive,
  onDragEnter,
  onDragLeave,
  onDragOver,
  onDrop,
  disabled,
}) {
  const inputRef = useRef(null)

  const handleInputChange = (event) => {
    onFilesSelected(Array.from(event.target.files ?? []))
    event.target.value = ''
  }

  return (
    <section
      className={`bento-card upload-dropzone ${dragActive ? 'dragging' : ''}`}
      onDragOver={onDragOver}
      onDragEnter={onDragEnter}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
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
          onClick={() => onFilesSelected([])}
          disabled={disabled}
        >
          Очистить
        </button>
      </div>
    </section>
  )
}
