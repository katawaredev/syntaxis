use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use dioxus::prelude::ServerFnError;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use syntaxis_workspace::WorkspaceId;

use crate::ai::{
    api::{
        PiAdvancedSettingsSnapshot, PiOperationResult, PiPackageAction, PiPackageSearch,
        PiPackageSummary, PiResourceScope, PiSettingsSnapshot, PiSkill, PromptTemplate,
        SkillCatalogView, SkillSearchPage, SkillSearchResult,
    },
    generated_settings::{PI_SETTING_DEFINITIONS, PiSettingKind},
};

mod auth;
mod instructions;
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
pub(crate) use instructions::{pi_global_instructions, save_pi_global_instructions};
pub(crate) use packages::{manage_pi_package, pi_packages};
pub(crate) use prompts::{delete_prompt_template, prompt_templates, save_prompt_template};
use resources::*;
use runtime::*;
pub(crate) use settings::{
    pi_advanced_settings, pi_settings, save_pi_advanced_settings, update_pi_setting,
};
pub(crate) use skills::{
    browse_pi_skills, delete_pi_skill, install_pi_skill, pi_skills, save_pi_skill,
    search_pi_skills, skill_catalog_available, update_tracked_pi_skills,
};

pub(crate) async fn update_pi(
    workspace_id: WorkspaceId,
) -> Result<PiOperationResult, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let output = run_pi(&workspace.root, &["update", "--all", "--no-approve"], false).await?;
    let version = verify_pi_integration(Path::new(&workspace.root)).await?;
    let updated_skills = update_tracked_pi_skills(Path::new(&workspace.root)).await?;
    let skill_message = match updated_skills {
        0 => "No tracked skills.sh skills needed refreshing.".to_owned(),
        1 => "Refreshed 1 tracked skills.sh skill.".to_owned(),
        count => format!("Refreshed {count} tracked skills.sh skills."),
    };
    Ok(PiOperationResult {
        message: if output.is_empty() {
            format!("Pi {version} is ready.\n{skill_message}")
        } else {
            format!(
                "{}\nPi {version} integration verified.\n{skill_message}",
                output.trim()
            )
        },
    })
}
