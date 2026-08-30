# Combain Studio MVP — Core / AI Developer Plan

**Проєкт:** Combain Studio MVP  
**Термін:** 30 календарних днів  
**Роль:** Developer 1 — Core / AI / Backend / FFmpeg  
**Стек:** Tauri 2, Rust, SQLite, FFmpeg/FFprobe, зовнішні AI API  
**Важливо:** локальні AI-моделі в MVP НЕ реалізуються.

---

# 1. Роль розробника

Developer 1 відповідає за внутрішню логіку Combain Studio:

```text
React UI
   ↓
Tauri IPC
   ↓
Rust Core
   ├── Domain
   ├── SQLite
   ├── Jobs
   ├── Provider integrations
   ├── Storage
   └── FFmpeg
```

Його задача — забезпечити стабільний backend-контур, до якого UI звертається через узгоджений IPC/API контракт.

---

# 2. Golden Path

Core повинен забезпечити:

```text
Topic
 ↓
Script
 ↓
Scenes
 ↓
Images
 ↓
Video clips
 ↓
Voice
 ↓
Timeline
 ↓
FFmpeg
 ↓
MP4
```

---

# 3. Правила роботи з Codex

Codex НЕ повинен:

- реалізовувати весь проєкт одразу;
- змінювати UI без потреби;
- додавати функціональність поза поточною фазою;
- hardcode API keys;
- hardcode provider-specific logic у domain layer;
- змінювати API contract без узгодження;
- видаляти старі asset versions;
- використовувати paid API у automated tests;
- додавати непотрібні dependencies.

Для кожної фази:

```text
1. Read current specification.
2. State scope.
3. State out-of-scope.
4. Inspect architecture.
5. Implement current phase only.
6. Run build.
7. Run tests.
8. Run lint/format.
9. Fix errors.
10. Report changed files.
11. Report known limitations.
```

---

# Phase 1 — Core Bootstrap

## Scope

Створити фундамент Rust/Tauri частини.

### Реалізувати

- Tauri 2;
- Rust project/workspace;
- application entry point;
- IPC command mechanism;
- configuration loading;
- base logging;
- базову модель помилок;
- базові tests.

### Acceptance

```text
Tauri app launches.
Rust backend starts.
Frontend can call test IPC command.
Tests run.
Formatting/linting pass.
```

### Out of Scope

- AI;
- SQLite domain;
- Timeline;
- FFmpeg.

---

# Phase 2 — Domain Models

## Scope

Створити MVP domain model.

### Entities

```text
Project
Scene
Asset
AssetVersion
Timeline
TimelineClip
Job
```

### Вимоги

- typed IDs;
- serialization/deserialization;
- Result-based errors;
- no dependency on React/UI;
- provider-independent domain model.

### Acceptance

```text
Entities compile.
Serialization tests pass.
Domain layer is independent from UI.
```

---

# Phase 3 — SQLite Persistence

## Scope

Реалізувати локальне зберігання.

### Реалізувати

- SQLite;
- schema;
- migrations;
- repositories;
- project persistence;
- scene persistence;
- asset persistence;
- asset version persistence;
- timeline persistence;
- job persistence.

### Acceptance

```text
Create project.
Save project.
Close app.
Reopen app.
Project remains.

Migration tests pass.
Repository tests pass.
```

---

# Phase 4 — Project / Scene API

## Scope

Створити IPC/API для UI.

### Commands

```text
create_project()
get_project()
list_projects()
update_project()

get_scenes()
create_scene()
update_scene()
delete_scene()
reorder_scenes()
```

### Acceptance

```text
UI can create project.
UI can load project.
UI can create/update/delete scenes.
Scene ordering persists.
```

---

# Phase 5 — AI Director / Text Provider

## Scope

Підключити текстовий AI API.

### Реалізувати

- TextProvider;
- API adapter;
- request validation;
- structured JSON output;
- script generation;
- scene segmentation.

### Input

```text
Topic
Duration
Style
Language
```

### Output

```json
{
  "title": "5 фактів про космос",
  "scenes": [
    {
      "number": 1,
      "title": "Вступ",
      "narration": "Текст озвучки...",
      "visual_prompt": "Cinematic space scene...",
      "video_prompt": "Slow camera movement...",
      "duration": 5
    }
  ]
}
```

### Acceptance

```text
Valid topic produces valid structured response.
Schema validation works.
Invalid response does not corrupt DB.
Mock provider tests pass.
```

---

# Phase 6 — Image Provider

## Scope

Підключити image-generation API.

### Реалізувати

- ImageProvider;
- API adapter;
- request;
- response handling;
- download;
- file validation;
- Asset creation;
- AssetVersion creation.

### Commands

```text
generate_image()
get_asset_versions()
select_asset_version()
```

### Acceptance

```text
Scene → Generate → image file.
Asset is created.
AssetVersion is created.
Previous versions remain.
Selected version can be changed.
```

---

# Phase 7 — Video Provider

## Scope

Підключити video-generation API.

### Пріоритет

```text
Image-to-Video
```

Підтримати також:

```text
Text-to-Video
```

### Реалізувати

- VideoProvider;
- API adapter;
- async requests;
- polling;
- operation ID;
- download;
- validation;
- AssetVersion.

### Acceptance

```text
Selected image + prompt → video.
Async operation is tracked.
Generated video is stored.
New generation creates new version.
```

---

# Phase 8 — TTS Provider

## Scope

Підключити TTS API.

### Реалізувати

- TTSProvider;
- synthesis request;
- audio download;
- validation;
- AssetVersion.

### Acceptance

```text
Scene narration → audio.
Audio is stored.
Audio can be used by Timeline.
Errors are reported.
```

---

# Phase 9 — Job System

## Scope

Відстеження довгих та асинхронних операцій.

### Job Types

```text
IMAGE_GENERATION
VIDEO_GENERATION
TTS
RENDER
```

### States

```text
QUEUED
RUNNING
COMPLETED
FAILED
CANCELLED
```

### Job відповідає за

- status;
- progress;
- error;
- provider operation ID за потреби;
- timestamps.

### Acceptance

```text
Generation creates Job.
Job state changes correctly.
UI can request status.
Failure is persisted.
Cancellation works where provider supports it.
```

---

# Phase 10 — Timeline Core + FFmpeg

## Scope

Реалізувати backend частину Timeline та render.

### Timeline

```text
Timeline
TimelineClip
track
start_time
duration
asset/version reference
```

### Operations

```text
add
move
remove
trim
split
reorder
save
load
```

### FFmpeg

Реалізувати:

- render command/graph;
- video stream;
- audio stream;
- MP4 output;
- temporary output;
- output validation.

### Acceptance

```text
Timeline survives restart.
Render uses exact timeline asset/version references.
Valid MP4 is produced.
Video and audio are present.
Existing output is not silently overwritten.
```

---

# Phase 11 — Full Integration / Validation

## Scope

Об'єднати всі Core-компоненти.

### Golden Path

```text
create_project
 ↓
generate_script
 ↓
create_scenes
 ↓
generate_image
 ↓
generate_video
 ↓
generate_tts
 ↓
create_timeline
 ↓
render
```

### Реалізувати

- validation;
- consistent errors;
- missing asset checks;
- timeline checks;
- render checks.

### Acceptance

Один тестовий проєкт проходить весь Golden Path без ручного редагування БД чи внутрішніх файлів.

---

# Phase 12 — Stabilization / Release

## Scope

Підготувати Core до MVP demo.

### Реалізувати

- critical bug fixing;
- logging cleanup;
- error handling;
- production configuration;
- Windows build;
- FFmpeg packaging check;
- clean-machine verification.

### Acceptance

```text
Production build succeeds.
Core tests pass.
Golden Path works.
No critical TODOs.
No secrets in repository.
```

---

# 4. Shared API Contract

Мінімальний IPC/API:

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

Кожен command має мати:

```text
Input
Output
Error
Side effects
```

---

# 5. Критичні правила

## Asset Versioning

Нова генерація:

```text
v1
v2
v3
```

не повинна видаляти або перезаписувати попередні версії.

## Timeline Source of Truth

Render використовує конкретну asset/version reference з Timeline.

Не використовувати правило:

```text
"взяти останній файл у папці"
```

## Secrets

API keys не зберігати у:

```text
frontend
Git
logs
project JSON
```

---

# 6. Пріоритети

### P0

- SQLite;
- Project;
- Scene;
- AI Director;
- Image Provider;
- Video Provider;
- TTS;
- Timeline Core;
- FFmpeg.

### P1

- Job progress;
- cancellation;
- advanced validation.

### P2

- minor optimization;
- additional provider capabilities.

---

# 7. Заборонено збільшувати scope

У рамках MVP не додавати без окремого рішення:

- local AI;
- provider router;
- multi-user;
- cloud backend;
- updater;
- advanced backup;
- professional NLE;
- advanced color grading;
- arbitrary plugins.

---

# 8. Порядок роботи Claude

Перед початком кожної Phase Claude **повинен** виконати такі кроки:

1. Прочитати це ТЗ.
2. Назвати `scope`.
3. Назвати `out-of-scope`.
4. Сформувати `implementation plan`.
5. Перелічити файли, які створюються або змінюються.
6. Перевірити API docs (якщо потрібно).
7. Реалізувати **тільки** поточну Phase.
8. Виконати `build`.
9. Виконати `tests`.
10. Виконати `lint`.
11. Виправити помилки (якщо є).
12. Надати **Phase Report** (за встановленим форматом).
13. Зупинитися та чекати окремої команди на наступну Phase.

---

# 9. Формат Phase Report

Після завершення кожної Phase Claude має видати звіт суворо за цим шаблоном:

Phase: [Номер та назва]
Status: [Success / Failed]

Implemented:
- ...

Changed files:
- ...

Tests:
- ...

Manual checks:
- ...

Known limitations:
- ...

Open questions:
- ...

Ready for next phase: [YES / NO]

---

# 10. Перший prompt для Claude

Стартова команда для агента перед початком будь-якої роботи:
`Прочитай файл ТЗ повністю.`