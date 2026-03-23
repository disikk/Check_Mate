# Check Mate

## Current Architecture

Проект стартует как фронтенд-прототип кабинета покерной школы.

### Stack

- React + Vite
- Plain CSS with CSS variables (Themes: Light / Dark via `data-theme` attribute)
- Vitest + Testing Library for minimal UI smoke coverage

### Design & Theme

- UI is inspired by `poker-ev-tracker` color scheme.
- Two themes implemented: Light and Dark. Switch via `ThemeToggle` in the TopBar.
- CSS uses plain variables mapped to `poker-ev-tracker` tailwind equivalents, generating a "glassmorphism" modern interface.

### Product Structure

- `Student` role:
  - загрузка hand history;
  - статистика ошибок;
  - динамика утечек;
  - MBR / FT ориентиры;
  - журнал ошибок по конкретным рукам.

- `Coach` role:
  - управление ренжами;
  - просмотр аналитики по ученику / группе / пулу;
  - heatmap-матрицы;
  - таблица учеников и качество исполнения.

### First Prototype Scope

- Общий layout с плавающим `Sidebar` (glassmorphism) и `main-content` областью.
- Role toggle `Student / Coach` перенесен внутрь `Sidebar`.
- Продвинутая "Bento Grid" система виджетов для обеих ролей.
- Переключение 2 тем (Light и Dark) через `TopHeader` (встроен в `App.jsx`).

### Implemented UI Modules

- `src/App.jsx` (содержит Layout Wrapper и Top Header)
- `src/components/Sidebar.jsx`
- `src/components/StudentDashboard.jsx` (отрисовывает весь Student UI)
- `src/components/CoachDashboard.jsx` (отрисовывает весь Coach UI)

### Data Layout

- `src/data/mockData.js` хранит моковые данные для обеих ролей.
- Для coach range UI используется сгенерированная 13x13 matrix со статусами `raise / call / mix / fold`.
- Student trend panel использует недельные данные по 5 типам целевых ошибок.

### Data Strategy

- На первом этапе все данные моковые и лежат во фронтенде.
- Структура данных проектируется так, чтобы потом заменить источник на backend / parser слой без переделки UI shell.

### Color Schemes (Precision Lab)

Новый интерфейс использует эффект матового стекла (glassmorphism) с 2 темами (Light и Dark):

#### Light Theme (`color-scheme: light`)
- **Background**: `#f5f7fb` + Radial Gradient (`#6366f1` / 15%)
- **Surface**: `rgba(255, 255, 255, 0.65)` (blur 24px)
- **Border**: `rgba(255, 255, 255, 0.5)`
- **Text**: `#0f172a` (Primary), `#475569` (Soft)
- **Primary Accent**: `#6366f1` (Indigo)

#### Dark Theme (`[data-theme="dark"]`)
- **Background**: `#050811` + Radial Gradient (`#6366f1` / 15%)
- **Surface**: `rgba(15, 23, 42, 0.45)` (blur 24px)
- **Border**: `rgba(255, 255, 255, 0.04)`
- **Text**: `#f8fafc` (Primary), `#cbd5e1` (Soft)
- **Primary Accent**: `#818cf8` (Indigo Light)

#### Shared Semantic Colors
- **Success**: Light `#10b981` / Dark `#34d399`
- **Warning**: Light `#f59e0b` / Dark `#fbbf24`
- **Danger**: Light `#f43f5e` / Dark `#fb7185`
- **Info**: Light `#3b82f6` / Dark `#60a5fa`
