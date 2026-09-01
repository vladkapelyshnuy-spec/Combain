//! Repositories: map domain entities (see [`crate::domain`]) to and from
//! SQLite rows via [`crate::db::Database`].
//!
//! Each submodule is a set of free functions operating on a `&Connection`
//! rather than a generic `Repository<T>` trait — there is exactly one
//! storage backend, so the extra abstraction has no payoff yet.
//!
//! IPC commands (Phase 4) are the real caller of most of this API; until
//! then, tests are the only caller, so most items are unused outside test
//! builds.
#![cfg_attr(not(test), allow(dead_code))]

pub mod asset_repository;
pub mod asset_version_repository;
pub mod job_repository;
pub mod project_repository;
pub mod provider_profile_repository;
pub mod scene_repository;
pub mod timeline_clip_repository;
pub mod timeline_repository;
