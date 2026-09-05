use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use dioxus::prelude::ServerFnError;
use syntaxis_agent::ThinkingLevel;
use syntaxis_workspace::WorkspaceId;

use crate::ai::api::ModelPreferences;

const MAX_MODEL_KEY_BYTES: usize = 1_024;
const MAX_AVAILABLE_MODELS: usize = 2_000;

struct PreferenceStore {
    path: PathBuf,
    file: PreferenceFile,
    legacy: Option<ModelPreferences>,
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct PreferenceFile {
    #[serde(default)]
    workspaces: BTreeMap<String, ModelPreferences>,
}

impl PreferenceStore {
    fn open(path: PathBuf) -> Result<Self, String> {
        let (file, legacy) = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<PreferenceFile>(&bytes) {
                Ok(file) => (file, None),
                Err(_) => {
                    let preferences = serde_json::from_slice::<ModelPreferences>(&bytes)
                        .map_err(|error| format!("Could not read model preferences: {error}"))?;
                    (PreferenceFile::default(), Some(preferences))
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (PreferenceFile::default(), None)
            }
            Err(error) => return Err(format!("Could not read model preferences: {error}")),
        };
        Ok(Self { path, file, legacy })
    }

    fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.file)
            .map_err(|error| format!("Could not encode model preferences: {error}"))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, &self.path))
            .map_err(|error| format!("Could not save model preferences: {error}"))
    }

    fn preferences_mut(&mut self, workspace_id: String) -> (&mut ModelPreferences, bool) {
        let migrated = !self.file.workspaces.contains_key(&workspace_id) && self.legacy.is_some();
        if migrated {
            self.file
                .workspaces
                .insert(workspace_id.clone(), self.legacy.take().unwrap_or_default());
        }
        (
            self.file.workspaces.entry(workspace_id).or_default(),
            migrated,
        )
    }
}

static PREFERENCES: OnceLock<Result<Mutex<PreferenceStore>, String>> = OnceLock::new();

pub(super) fn sync(
    workspace_id: WorkspaceId,
    available_models: Vec<String>,
) -> Result<ModelPreferences, ServerFnError> {
    if available_models.is_empty() || available_models.len() > MAX_AVAILABLE_MODELS {
        return Err(request_error("The available model list is invalid.", 400));
    }
    for key in &available_models {
        validate_key(key)?;
    }
    let mut store = store()?;
    let (preferences, migrated) = store.preferences_mut(workspace_id.0);
    let preferences = preferences.clone();
    if migrated {
        store.save().map_err(internal_error)?;
    }
    Ok(preferences)
}

pub(super) fn set_favourite(
    workspace_id: WorkspaceId,
    model_key: String,
    favourite: bool,
) -> Result<ModelPreferences, ServerFnError> {
    validate_key(&model_key)?;
    let mut store = store()?;
    let (preferences, _) = store.preferences_mut(workspace_id.0);
    preferences
        .favourites
        .retain(|existing| existing != &model_key);
    if favourite {
        preferences.favourites.insert(0, model_key);
    }
    let preferences = preferences.clone();
    store.save().map_err(internal_error)?;
    Ok(preferences)
}

pub(super) fn set_effort(
    workspace_id: WorkspaceId,
    model_key: String,
    effort: ThinkingLevel,
) -> Result<ModelPreferences, ServerFnError> {
    validate_key(&model_key)?;
    let mut store = store()?;
    let (preferences, _) = store.preferences_mut(workspace_id.0);
    preferences.efforts.insert(model_key, effort);
    let preferences = preferences.clone();
    store.save().map_err(internal_error)?;
    Ok(preferences)
}

fn store() -> Result<MutexGuard<'static, PreferenceStore>, ServerFnError> {
    PREFERENCES
        .get_or_init(|| {
            PreferenceStore::open(
                crate::workspace::api::server::data_directory().join("model-preferences.json"),
            )
            .map(Mutex::new)
        })
        .as_ref()
        .map_err(|error| internal_error(error.clone()))?
        .lock()
        .map_err(|_| internal_error("The model preference store is unavailable."))
}

fn validate_key(key: &str) -> Result<(), ServerFnError> {
    let valid = key.split_once('\u{1f}').is_some_and(|(provider, model)| {
        !provider.is_empty()
            && !model.is_empty()
            && !provider.chars().any(char::is_control)
            && !model.chars().any(char::is_control)
    });
    if key.len() > MAX_MODEL_KEY_BYTES || !valid {
        return Err(request_error("The model identifier is invalid.", 400));
    }
    Ok(())
}

fn request_error(message: impl Into<String>, code: u16) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}

fn internal_error(message: impl Into<String>) -> ServerFnError {
    request_error(message, 500)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_keys_allow_only_the_provider_separator_control_character() {
        validate_key("openai\u{1f}gpt-5.6-sol").unwrap();
        validate_key("missing-separator").unwrap_err();
        validate_key("openai\u{1f}gpt\n5").unwrap_err();
        validate_key("openai\u{1f}gpt\u{1f}duplicate").unwrap_err();
    }

    #[test]
    fn preferences_are_persisted_per_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("model-preferences.json");
        let mut store = PreferenceStore::open(path.clone()).unwrap();
        let first = store.file.workspaces.entry("first".into()).or_default();
        first.favourites = vec!["p\u{1f}sol".into()];
        first
            .efforts
            .insert("p\u{1f}sol".into(), ThinkingLevel::Medium);
        let second = store.file.workspaces.entry("second".into()).or_default();
        second
            .efforts
            .insert("p\u{1f}sol".into(), ThinkingLevel::High);
        store.save().unwrap();

        let reopened = PreferenceStore::open(path).unwrap();
        let first = reopened.file.workspaces.get("first").unwrap();
        let second = reopened.file.workspaces.get("second").unwrap();
        assert_eq!(first.favourites, ["p\u{1f}sol"]);
        assert_eq!(
            first.efforts.get("p\u{1f}sol"),
            Some(&ThinkingLevel::Medium)
        );
        assert_eq!(second.efforts.get("p\u{1f}sol"), Some(&ThinkingLevel::High));
    }
}
