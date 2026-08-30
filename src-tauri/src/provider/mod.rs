//! AI provider integrations: provider-independent traits (see [`text_provider`])
//! plus concrete adapters. Domain and command code must depend only on the
//! traits, never on a specific adapter, so providers can be swapped without
//! touching business logic.
#![allow(unused_imports)]

pub mod openai_text_provider;
pub mod text_provider;

#[cfg(test)]
pub mod mock;

pub use openai_text_provider::{OpenAiConfig, OpenAiTextProvider};
pub use text_provider::{
    validate_script_response, GeneratedScene, ScriptRequest, ScriptResponse, TextProvider,
};
