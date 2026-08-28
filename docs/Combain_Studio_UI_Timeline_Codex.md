# Combain Studio MVP — UI / Timeline Developer Plan

**Проєкт:** Combain Studio MVP  
**Термін:** 30 календарних днів  
**Роль:** Developer 2 — UI / UX / Storyboard / Media / Timeline  
**Стек:** React, TypeScript, Tauri IPC  
**Основна задача:** реалізувати користувацький інтерфейс і Timeline для Golden Path від теми до MP4.

---

# 1. Роль розробника

Developer 2 відповідає за:

```text
Projects
Script
Scenes
Storyboard
Image generation UI
Video generation UI
TTS UI
Media
Timeline
Preview
Export
```

Frontend не звертається напряму до AI API.

```text
React
 ↓
Tauri IPC
 ↓
Rust Core
 ↓
Provider API
```

---

# 2. Основний UX

```text
Project
 ↓
Topic
 ↓
Script
 ↓
Scenes
 ↓
Storyboard
 ↓
Generate Image
 ↓
Generate Video
 ↓
Generate Voice
 ↓
Timeline
 ↓
Preview
 ↓
Export
```

Користувач завжди повинен бачити поточний статус операції, вибрану версію asset та можливість виконати наступну дію.

---

# 3. Правила роботи з Codex

Codex НЕ повинен:

- змінювати Rust без необхідності;
- викликати AI API напряму з React;
- вигадувати IPC commands;
- створювати дублікати domain models;
- змінювати API contract без узгодження;
- додавати великі функції поза поточною фазою;
- будувати professional NLE;
- додавати непотрібні UI frameworks.

Для кожної фази:

```text
1. Read current specification.
2. State scope.
3. State out-of-scope.
4. Identify required IPC/API.
5. Implement current UI only.
6. Run frontend build.
7. Run lint/typecheck.
8. Test manually.
9. Report changed files.
10. Report known limitations.
```

---

# Phase 1 — UI Bootstrap

## Scope

Створити базовий React application shell.

### Реалізувати

- React;
- TypeScript;
- routing;
- base layout;
- theme;
- shared UI primitives;
- loading state;
- error state.

### Acceptance

```text
Application opens.
Main layout renders.
Navigation works.
TypeScript build succeeds.
Lint succeeds.
```

---

# Phase 2 — Application Layout

## Scope

Створити головну структуру Combain Studio.

```text
┌────────────────────────────────────────────┐
│ Top Bar                                    │
├────────────┬───────────────────────────────┤
│ Sidebar    │ Workspace                     │
│            │                               │
│ Projects   │                               │
│ Script     │                               │
│ Scenes     │                               │
│ Storyboard │                               │
│ Media      │                               │
│ Timeline   │                               │
│ Export     │                               │
└────────────┴───────────────────────────────┘
```

### Acceptance

- navigation works;
- selected section visible;
- workspace changes;
- reusable components created.

---

# Phase 3 — Projects UI

## Scope

Створити інтерфейс створення та відкриття проєктів.

### Реалізувати

- project list;
- New Project;
- project name;
- topic;
- style;
- Open Project;
- empty state.

### IPC

```text
list_projects()
create_project()
get_project()
```

### Acceptance

```text
New Project
 ↓
Project created
 ↓
Project opens
```

Після перезапуску проєкт присутній у списку.

---

# Phase 4 — Script / Scene Editor

## Scope

UI для сценарію та сцен.

### Реалізувати

- topic input;
- Generate Script;
- loading state;
- script view/editor;
- scene list;
- scene selection;
- scene editing.

### Scene

```text
Title
Narration
Visual Prompt
Video Prompt
Duration
```

### Acceptance

```text
Topic
 ↓
Generate Script
 ↓
Scenes appear
 ↓
User can edit scene
```

---

# Phase 5 — Storyboard

## Scope

Візуальна сітка/список сцен.

### Scene Card

```text
┌──────────────────────────┐
│ Scene 01                 │
│                          │
│ [ IMAGE / PLACEHOLDER ]  │
│                          │
│ Narration                │
│ Duration                 │
│                          │
│ [Generate Image]         │
│ [Generate Video]         │
└──────────────────────────┘
```

### Показувати

- scene number;
- title;
- narration;
- duration;
- image status;
- video status;
- selected version;
- progress;
- error/warning.

### Acceptance

5–10 scenes можна переглядати й редагувати в storyboard.

---

# Phase 6 — Image Generation UI

## Scope

UI image generation через IPC.

### Реалізувати

- Generate;
- Regenerate;
- Edit Prompt;
- loading;
- success;
- error;
- versions;
- Select Version.

### Versions

```text
v1
v2
v3
```

### Acceptance

```text
Generate
 ↓
Loading
 ↓
Image
 ↓
Regenerate
 ↓
v2
 ↓
Select
```

---

# Phase 7 — Video Generation UI

## Scope

UI для Image-to-Video та Text-to-Video.

### Пріоритет

```text
Image-to-Video
```

### Реалізувати

- Generate Video;
- provider/model display;
- progress;
- processing state;
- preview;
- regenerate;
- version selection;
- error state.

### Acceptance

```text
Selected image
+
Video prompt
 ↓
Generate
 ↓
Video
 ↓
Preview
```

---

# Phase 8 — TTS / Audio UI

## Scope

UI для генерації озвучки.

### Реалізувати

- Generate Voice;
- loading;
- success;
- error;
- audio preview;
- duration/status.

### Acceptance

```text
Narration
 ↓
Generate Voice
 ↓
Audio ready
 ↓
Play
```

---

# Phase 9 — Media Library

## Scope

Єдиний UI для перегляду assets.

### Categories

```text
Images
Videos
Audio
```

### Показувати

- thumbnail;
- type;
- scene;
- version;
- provider;
- model;
- selected state.

### Дії

- Open;
- Preview;
- Select;
- Replace where applicable.

### Acceptance

Assets із різних сцен можна знайти та переглянути.

---

# Phase 10 — Timeline UI

## Scope

Створити MVP Timeline.

### Tracks

```text
VIDEO
AUDIO
```

### Обов'язково

- Add;
- Move;
- Reorder;
- Trim;
- Split;
- Delete;
- Save;
- Play.

### Мінімальний вигляд

```text
VIDEO
┌──────┬──────┬──────┬──────┐
│ S1   │ S2   │ S3   │ S4   │
└──────┴──────┴──────┴──────┘

AUDIO
┌─────────────────────────────┐
│           VOICE             │
└─────────────────────────────┘
```

### Не реалізовувати в MVP

- professional NLE;
- effects;
- keyframes;
- color grading;
- complex transitions;
- ripple editing;
- complex snapping.

### Acceptance

```text
User can place clips.
User can move clips.
User can trim clips.
User can split clips.
User can delete clips.
Timeline persists.
```

---

# Phase 11 — Preview / Export UI

## Scope

Зв'язати Timeline з preview та render.

### Preview

Мінімум:

- Play;
- Pause;
- Duration.

### Export

```text
Output name
Output folder
Resolution

[ EXPORT VIDEO ]
```

### Progress

```text
Rendering...
████████░░ 80%
```

### Acceptance

```text
Timeline
 ↓
Preview
 ↓
Export
 ↓
MP4
```

---

# Phase 12 — Polish / Integration

## Scope

Фінальне UI-полірування.

### Перевірити

- loading states;
- error states;
- empty states;
- disabled buttons;
- job status;
- selected versions;
- navigation;
- desktop layout;
- visual consistency.

### Acceptance

Повний Golden Path проходить через UI:

```text
New Project
 ↓
Topic
 ↓
Generate Script
 ↓
Scenes
 ↓
Generate Image
 ↓
Generate Video
 ↓
Generate Voice
 ↓
Timeline
 ↓
Preview
 ↓
Export
```

---

# 4. Shared API Contract

Frontend використовує тільки узгоджені IPC/API commands:

```text
create_project()
get_project()
list_projects()

generate_script()

get_scenes()
update_scene()

generate_image()
generate_video()
generate_tts()

get_assets()
get_asset_versions()
select_asset_version()

get_timeline()
save_timeline()

render_video()
get_job_status()
cancel_job()
```

Не вигадувати нові commands локально.

---

# 5. UI State Model

Для кожної async operation:

```text
IDLE
LOADING
SUCCESS
ERROR
```

Для jobs:

```text
QUEUED
RUNNING
COMPLETED
FAILED
CANCELLED
```

Кнопка Generate повинна бути disabled під час активної операції, якщо provider не підтримує паралельний запуск.

---

# 6. Asset Version UX

Користувач повинен бачити:

```text
Scene 03

Image:
[ v1 ] [ v2 ] [ v3 ]

Selected: v2
```

Нова генерація не повинна приховувати старі версії.

---

# 7. Timeline ↔ Asset Contract

Timeline використовує конкретну версію asset.

Наприклад:

```text
Scene 03
Image Asset
Version 2
```

Не можна автоматично переключати Timeline на нову генерацію.

---

# 8. Що не робити

Не реалізовувати в рамках UI MVP:

- local AI UI;
- provider routing;
- multi-user;
- cloud sync;
- advanced NLE;
- professional audio mixer;
- advanced proxy system;
- 4K editing optimization;
- updater;
- installer UI;
- advanced backup UI.

---

# 9. UI Definition of Done

UI MVP завершено, коли:

```text
Project creation works.
Project reopen works.
Script generation is visible.
Scenes are editable.
Storyboard works.
Image generation UI works.
Video generation UI works.
TTS UI works.
Media Library works.
Timeline works.
Preview works.
Export works.
Errors are understandable.
Loading states work.
```

---

# 10. Coordination with Core Developer

Перед інтеграцією:

```text
1. Confirm IPC name.
2. Confirm input shape.
3. Confirm output shape.
4. Confirm error shape.
5. Implement UI against contract.
6. Use mock data if Core is not ready.
```

Якщо Core ще не готовий, не змінювати архітектуру заради тимчасового UI.

---

# 11. Пріоритети

### P0

- Projects;
- Script;
- Scenes;
- Storyboard;
- Image;
- Video;
- TTS;
- Timeline;
- Preview;
- Export.

### P1

- Media Library improvements;
- better progress;
- better errors.

### P2

- cosmetic improvements;
- optional shortcuts;
- advanced timeline features.
