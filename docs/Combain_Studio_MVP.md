# Combain Studio MVP — 30 Days

**Версія:** MVP 1.0  
**Термін розробки:** 30 календарних днів  
**Команда:** 2 розробники + Codex  
**Платформа:** Windows 10/11  
**Тип:** Desktop-застосунок  
**Концепція:** AI Video Studio за workflow Combain Studio  
**Архітектура:** Tauri 2 + React + TypeScript + Rust + SQLite  
**AI-підхід:** Local-first + Cloud fallback

---

## 1. Мета проєкту

Створити працездатний desktop-застосунок для автоматизованого створення коротких відео за допомогою AI.

Основний workflow:

```text
ТЕМА
  ↓
AI СЦЕНАРІЙ
  ↓
РОЗБИВКА НА СЦЕНИ
  ↓
STORYBOARD
  ↓
ГЕНЕРАЦІЯ ЗОБРАЖЕНЬ
  ↓
ГЕНЕРАЦІЯ ВІДЕО
  ↓
TTS / ОЗВУЧКА
  ↓
TIMELINE
  ↓
PREVIEW
  ↓
FFMPEG
  ↓
ГОТОВИЙ MP4
```

Застосунок має дати користувачу можливість пройти весь шлях від теми до готового відео в одному desktop-інтерфейсі.

Ключовий принцип: AI генерує матеріали, але користувач контролює, які саме assets використовуються у фінальному відео.

---

# 2. Scope MVP

## 2.1. Обов'язковий функціонал

Користувач повинен мати можливість:

1. Запустити застосунок.
2. Створити проєкт.
3. Ввести тему відео.
4. Згенерувати сценарій.
5. Автоматично розбити сценарій на сцени.
6. Переглядати та редагувати сцени.
7. Генерувати зображення для сцени.
8. Перегенеровувати зображення.
9. Зберігати декілька версій asset.
10. Вибирати потрібну версію.
11. Генерувати відеокліп для сцени.
12. Генерувати озвучку.
13. Формувати Timeline.
14. Переміщувати, обрізати, розділяти та видаляти кліпи.
15. Переглядати результат.
16. Експортувати готове відео у MP4.
17. Закрити та повторно відкрити проєкт без втрати даних.

---

# 3. Golden Path

Основний acceptance scenario MVP:

```text
Topic
 ↓
Script
 ↓
5–10 Scenes
 ↓
Images
 ↓
Video clips
 ↓
Voice
 ↓
Timeline
 ↓
Preview
 ↓
MP4
```

Один повністю працездатний Golden Path важливіший за велику кількість додаткових функцій.

---


# 4. Архітектура

```text
┌─────────────────────────────────────────┐
│              React UI                   │
│                                         │
│ Project / Script / Scenes / Timeline    │
└───────────────────┬─────────────────────┘
                    │
                 Tauri IPC
                    │
┌───────────────────▼─────────────────────┐
│              Rust Core                  │
│                                         │
│ Domain / Projects / Assets / Timeline   │
│ Jobs / Persistence / FFmpeg             │
└───────┬───────────────┬─────────────────┘
        │               │
        ▼               ▼
     SQLite          AI Runtime
                        │
                 ┌──────┴───────┐
                 │              │
              Local          Cloud
                 │              │
              Models          APIs
```

## Принципи

- UI не звертається безпосередньо до AI API.
- AI-провайдери ізольовані від бізнес-логіки.
- Дані проєкту зберігаються локально.
- Timeline є джерелом істини для фінального рендеру.
- Генеровані assets мають версії.
- API keys не зберігаються у frontend bundle або Git.

---

# 5. Стек

## Desktop

- Tauri 2

## Frontend

- React
- TypeScript

## Backend / Core

- Rust

## Database

- SQLite

## Video processing

- FFmpeg
- FFprobe

## Local AI Runtime

- Python
- PyTorch
- CUDA
- відповідні inference libraries

---

# 6. AI / API

На етапі MVP локальні AI-моделі НЕ використовуються.

Усі AI-функції працюють через зовнішні API.

Основний принцип:

```text
React UI
   ↓
Tauri / Rust Core
   ↓
Provider API
   ↓
AI Service
```

Для MVP необхідно передбачити абстракції провайдерів, щоб у майбутньому можна було додати локальні моделі без переписування UI та бізнес-логіки.

Конкретні AI-сервіси та API визначаються окремо під час реалізації відповідних Provider.


# 7. AI Provider Abstraction

Передбачити інтерфейси:

```text
TextProvider
ImageProvider
VideoProvider
TTSProvider
```

У MVP усі AI-провайдери працюють через зовнішні API.

```text
TextProvider  → API
ImageProvider → API
VideoProvider → API
TTSProvider   → API
```

Локальні `LocalImageProvider`, `LocalVideoProvider`, `LocalTextProvider` та `LocalTTSProvider` у MVP НЕ реалізовуються.

Архітектура повинна залишатися достатньо абстрактною, щоб у майбутньому за потреби можна було додати локальні провайдери без переписування UI та основної бізнес-логіки.

Конкретні API та моделі визначаються окремо під час реалізації відповідних Provider.

---

# 8. AI Runtime

Local AI Runtime запускається окремо від Rust Core.

```text
Tauri
   │
   │ localhost API
   ▼
Python AI Runtime
   │
   ├── Text
   ├── Image
   ├── Video
   └── TTS
```

Rust відповідає за:

- запуск/контроль задач;
- збереження стану;
- IPC;
- інтеграцію з UI.

Python Runtime відповідає за:

- завантаження локальних моделей;
- inference;
- роботу з CUDA;
- генерацію AI assets.

---

# 9. Мінімальна модель даних

## Project

```text
id
name
topic
style
created_at
updated_at
```

## Scene

```text
id
project_id
order
title
narration
visual_prompt
video_prompt
duration
status
```

## Asset

```text
id
scene_id
type
```

Типи:

```text
IMAGE
VIDEO
AUDIO
```

## AssetVersion

```text
id
asset_id
version
file_path
provider
model
prompt
created_at
selected
```

## Timeline

```text
id
project_id
```

## TimelineClip

```text
id
timeline_id
asset_id
track
start_time
duration
```

## Job

`Job` відповідає за відстеження довготривалих або асинхронних операцій.

Приклади:

- генерація зображення;
- генерація відео;
- генерація TTS;
- фінальний FFmpeg render.

Основні стани:

```text
QUEUED
RUNNING
COMPLETED
FAILED
CANCELLED
```

Мінімальні поля:

```text
id
type
status
progress
error
created_at
updated_at
```

---

# 10. AI Director

## Input

```text
Topic
Duration
Style
Language
```

## Output

AI повинен повертати структурований JSON.

Приклад:

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

Не використовувати довільний текст AI як єдине джерело структурованих даних.

---

# 11. Storyboard

Для кожної сцени UI повинен показувати:

```text
┌──────────────────────────────┐
│ Scene 01                     │
│                              │
│ [ Generated Image ]          │
│                              │
│ Narration:                   │
│ ................             │
│                              │
│ Image: [Generate]            │
│ Video: [Generate]            │
│                              │
│ Versions: v1 v2 v3           │
│             [Select]         │
└──────────────────────────────┘
```

Обов'язково:

- Generate;
- Regenerate;
- Edit prompt;
- View versions;
- Select version.

---

# 12. Image Generation

Pipeline:

```text
Scene
 ↓
visual_prompt
 ↓
ImageProvider
 ↓
image file
 ↓
AssetVersion
```

Після генерації:

1. файл зберігається;
2. створюється Asset;
3. створюється AssetVersion;
4. версія показується у UI;
5. користувач може її вибрати.

---

# 13. Video Generation

Підтримати:

## Image-to-Video

```text
Selected Image
+
Video Prompt
↓
Video
```

## Text-to-Video

```text
Video Prompt
↓
Video
```

**I2V має пріоритет над T2V для MVP.**

AI повинен генерувати короткі shots, а не повний довгий ролик.

Рекомендований MVP підхід:

```text
Scene 1 → 4–6 sec
Scene 2 → 4–6 sec
Scene 3 → 4–6 sec
Scene 4 → 4–6 sec
...
        ↓
      Timeline
        ↓
     Final Video
```

---

# 14. TTS

Pipeline:

```text
Scene narration
 ↓
TTSProvider
 ↓
audio.wav
 ↓
AssetVersion
```

Для MVP кожна сцена може мати власний audio asset.

---

# 15. Timeline

Timeline є джерелом істини для фінального рендеру.

## Tracks

Мінімум:

```text
VIDEO
AUDIO
```

## Обов'язкові операції

- Add;
- Remove;
- Move;
- Reorder;
- Trim;
- Split;
- Save;
- Play.

## Мінімальний UI

```text
VIDEO
┌──────┬──────┬──────┬──────┐
│  S1  │  S2  │  S3  │  S4  │
└──────┴──────┴──────┴──────┘

AUDIO
┌─────────────────────────────┐
│           VOICE             │
└─────────────────────────────┘
```

Professional NLE-функції не входять до MVP.

---

# 16. Preview

MVP використовує простий video player.

Обов'язково:

- Play;
- Pause;
- Duration.

Proxy generation, waveform cache та складна preview optimization не входять до MVP.

---

# 17. Final Render

Використовувати FFmpeg.

Pipeline:

```text
Timeline
 ↓
Validate
 ↓
Build FFmpeg command/graph
 ↓
Render temporary file
 ↓
Validate output
 ↓
Final MP4
```

Acceptance:

- MP4 створюється;
- video присутнє;
- audio присутній;
- тривалість правильна;
- файл можна відкрити стандартним плеєром;
- існуючий файл не перезаписується випадково.

---

# 18. Storage

Local-first структура:

```text
CombainStudio/
│
├── data/
│   ├── database.sqlite
│   ├── projects/
│   ├── assets/
│   ├── exports/
│   └── logs/
│
└── runtime/
```

Backend-сервер для MVP не потрібен.

---

# 19. Security

Заборонено:

- API keys у React;
- API keys у Git;
- API keys у logs;
- hardcoded secrets.

Credential storage повинен бути відокремлений від frontend.

Для Windows передбачити використання системного secure credential storage.

---

# 20. Фінальний результат

Після 30 днів має існувати не просто набір AI-модулів, а єдиний працездатний desktop workflow:

```text
┌─────────────────────────────────────────┐
│            COMBAIN STUDIO MVP          │
├─────────────────────────────────────────┤
│                                         │
│ Topic → Script → Scenes                 │
│                 ↓                       │
│          Storyboard                     │
│                 ↓                       │
│      Image → Video → TTS                │
│                 ↓                       │
│              Timeline                   │
│                 ↓                       │
│              Preview                    │
│                 ↓                       │
│             FFmpeg                      │
│                 ↓                       │
│              VIDEO.MP4                  │
│                                         │
└─────────────────────────────────────────┘
```

**Головний критерій MVP:** користувач повинен самостійно створити хоча б один повний ролик від теми до MP4 без ручного редагування файлів або запуску внутрішніх скриптів.
