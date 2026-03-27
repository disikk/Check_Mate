import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test } from 'vitest'
import App from './App'
import { MOCK_USER_TIMEZONE_STORAGE_KEY } from './services/mockUserTimezone'

describe('P2-03 mock timezone flow', () => {
  beforeEach(() => {
    window.localStorage.clear()
  })

  test('upload shows a soft warning when timezone is missing but remains usable', async () => {
    const user = userEvent.setup()
    render(<App />)

    await user.click(screen.getByRole('button', { name: /загрузка рук/i }))

    expect(
      screen.getByText(/таймзона для GG импорта пока не выбрана/i),
    ).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: 'Выбрать файлы' }),
    ).toBeEnabled()
  })

  test('settings persist timezone in local storage and remove upload warning', async () => {
    const user = userEvent.setup()
    render(<App />)

    await user.click(screen.getByRole('button', { name: /настройки/i }))
    const timezoneInput = screen.getByLabelText(/часовой пояс \(iana\)/i)

    await user.clear(timezoneInput)
    await user.type(timezoneInput, 'Asia/Krasnoyarsk')
    await user.click(screen.getByRole('button', { name: 'Сохранить таймзону' }))

    expect(window.localStorage.getItem(MOCK_USER_TIMEZONE_STORAGE_KEY)).toBe(
      'Asia/Krasnoyarsk',
    )
    expect(screen.getByDisplayValue('Asia/Krasnoyarsk')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /загрузка рук/i }))

    expect(
      screen.queryByText(/таймзона для GG импорта пока не выбрана/i),
    ).not.toBeInTheDocument()
    expect(
      screen.getByText(/utc и часовые агрегаты будут доступны/i),
    ).toBeInTheDocument()
  })
})
