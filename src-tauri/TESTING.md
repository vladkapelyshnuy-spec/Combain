# Testing Guide

## Overview

Combain Studio has comprehensive test coverage across all layers of the application:

- **292 total tests** (283 unit tests + 9 integration tests)
- **Unit tests**: Test individual modules in isolation
- **Integration tests**: Test complete workflows end-to-end

## Running Tests

```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test golden_path_test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

## Test Structure

```
src-tauri/
├── src/
│   ├── commands/          # Command tests inline
│   ├── db/                # Database tests
│   ├── domain/            # Domain model tests
│   ├── ffmpeg/            # FFmpeg integration tests
│   ├── provider/          # Provider tests
│   ├── repository/        # Repository tests
│   ├── storage/           # Storage tests
│   └── validation/        # Validation tests
└── tests/
    ├── golden_path_test.rs   # End-to-end integration test
    └── helpers/
        └── mod.rs            # Test utilities
```

## Golden Path Integration Test

The `golden_path_test.rs` validates the complete user workflow:

1. **Project Creation**: Create a new project
2. **Scene Management**: Create multiple scenes
3. **Asset Generation**: Generate mock image, video, and audio assets
4. **Asset Versioning**: Create and select asset versions
5. **Timeline Creation**: Build timeline from assets
6. **Clip Management**: Add clips to timeline tracks
7. **Validation**: Validate project readiness for render

### Test Scenarios

- ✅ Complete golden path workflow
- ✅ Missing asset validation
- ✅ Missing selected version validation
- ✅ Empty timeline validation
- ✅ Incomplete scene validation

## Test Helpers

Located in `tests/helpers/mod.rs`:

### TestEnvironment

Provides a clean test environment for each test:

```rust
let env = TestEnvironment::new()?;
let db = env.db();
let data_root = &env.data_root;
```

- In-memory SQLite database
- Temporary filesystem for generated files
- Automatic cleanup on drop

### Mock Asset Generation

Create minimal valid media files for testing:

```rust
create_mock_png(&path)?;   // 1x1 transparent PNG
create_mock_mp4(&path)?;   // Minimal valid MP4
create_mock_mp3(&path)?;   // Minimal valid MP3
```

## Test Coverage by Module

### Domain Models (`src/domain/`)
- Project, Scene, Asset, AssetVersion
- Timeline, TimelineClip
- Job, JobStatus, JobType
- ProviderProfile
- Validation rules and state transitions

### Repositories (`src/repository/`)
- CRUD operations for all entities
- Foreign key constraints
- Cascade deletions
- Ordering and filtering

### Validation (`src/validation/`)
- Project completeness
- Scene ordering and existence
- Asset type coverage
- Timeline readiness
- File existence checks

### Storage (`src/storage/`)
- File path generation
- Format detection (PNG, JPEG, WebP, MP4, MP3)
- Version immutability enforcement

### FFmpeg (`src/ffmpeg/`)
- Binary detection
- Command building
- Filter graph generation
- Output validation

### Providers (`src/provider/`)
- OpenAI image/video/TTS
- ElevenLabs TTS
- RoyalTechno video
- Error handling and retries

## Writing New Tests

### Unit Tests

Place tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Arrange
        let input = create_test_data();
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

Add scenarios to `tests/golden_path_test.rs`:

```rust
#[test]
fn test_new_workflow() -> Result<(), AppError> {
    let env = TestEnvironment::new()?;
    let conn = env.db().conn();
    
    // Test workflow steps...
    
    Ok(())
}
```

## Test Database

Integration tests use `Database::open_in_memory()` which:
- Creates a fresh in-memory SQLite database
- Runs all migrations automatically
- Enables foreign key constraints
- Provides complete isolation between tests

## Continuous Integration

All tests run automatically on:
- Every commit
- Pull requests
- Before releases

**Current Status**: ✅ All 292 tests passing

## Test Maintenance

- Keep tests fast (current: ~0.5s for all tests)
- Use in-memory DB for speed
- Mock external API calls
- Clean up resources automatically
- Document complex test scenarios
