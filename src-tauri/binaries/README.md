# Bundled FFmpeg runtime

Combain Studio uses the static **GyanD FFmpeg 8.0.1 full build for Windows x64**
as Tauri sidecars. The pinned archive is published at:

`https://github.com/GyanD/codexffmpeg/releases/download/8.0.1/ffmpeg-8.0.1-full_build.zip`

Expected archive SHA-256:

`467CDE100A47ED4B03A897988AEB4A296890C1E2B2D2864204657D002BC5FB90`

From the repository root, prepare the sidecars with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\setup-ffmpeg.ps1
```

The script verifies the archive before extraction, validates both executables,
and installs them using Tauri's target-triple naming convention:

- `ffmpeg-x86_64-pc-windows-msvc.exe`
- `ffprobe-x86_64-pc-windows-msvc.exe`

The generated `ffmpeg-sidecars.json` records the verified archive identity and
the extracted executable hashes. A repeated setup skips downloading only while
both sidecars still match that manifest.

GyanD identifies all its static builds, including this full build, as GPLv3.
Redistributing these binaries therefore requires compliance with GPLv3 and the
licenses of enabled third-party libraries/codecs, including corresponding-source
and notice obligations where applicable. Review the build configuration and
distribution obligations before shipping an installer.

The application does not download executables at runtime and does not require
FFmpeg on the user's `PATH`.
