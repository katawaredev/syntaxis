use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use dioxus::prelude::ServerFnError;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use syntaxis_workspace::WorkspaceId;

use crate::ai::{
    api::{
        PiOperationResult, PiPackageAction, PiPackageSearch, PiPackageSummary, PiResourceScope,
        PiSettingsSnapshot, PiSkill, PromptTemplate, SkillCatalogView, SkillSearchPage,
        SkillSearchResult,
    },
    generated_settings::{PiSettingKind, PI_SETTINGS_SCHEMA_VERSION, PI_SETTING_DEFINITIONS},
};

mod auth;
mod packages;
mod prompts;
mod resources;
mod runtime;
mod settings;
mod skills;

pub(crate) use auth::{
    cancel_pi_provider_login, logout_pi_provider, pi_provider_login_status, pi_providers,
    respond_to_pi_provider_login, start_pi_provider_login,
};
pub(crate) use packages::{manage_pi_package, pi_packages};
pub(crate) use prompts::{delete_prompt_template, prompt_templates, save_prompt_template};
use resources::*;
use runtime::*;
pub(crate) use settings::{pi_settings, update_pi_setting};
pub(crate) use skills::{
    browse_pi_skills, delete_pi_skill, install_pi_skill, pi_skills, save_pi_skill,
    search_pi_skills, skill_catalog_available, update_tracked_pi_skills,
};

pub(crate) async fn update_pi(
    workspace_id: WorkspaceId,
) -> Result<PiOperationResult, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let output = run_pi(&workspace.root, &["update", "--all", "--no-approve"], false).await?;
    let updated_skills = update_tracked_pi_skills(Path::new(&workspace.root)).await?;
    let skill_message = match updated_skills {
        0 => "No tracked skills.sh skills needed refreshing.".to_owned(),
        1 => "Refreshed 1 tracked skills.sh skill.".to_owned(),
        count => format!("Refreshed {count} tracked skills.sh skills."),
    };
    Ok(PiOperationResult {
        message: if output.is_empty() {
            skill_message
        } else {
            format!("{}\n{skill_message}", output.trim())
        },
    })
}
