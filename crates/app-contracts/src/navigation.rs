use std::{convert::Infallible, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use syntaxis_workspace::{RelativePath, WorkspaceId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileLocation {
    pub path: RelativePath,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub end_line: Option<usize>,
    pub end_column: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiSettingsSection {
    #[default]
    General,
    ProviderAccounts,
    GlobalInstructions,
    PromptTemplates,
    Skills,
    Extensions,
}

impl AiSettingsSection {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::ProviderAccounts,
        Self::GlobalInstructions,
        Self::PromptTemplates,
        Self::Skills,
        Self::Extensions,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::ProviderAccounts => "Provider accounts",
            Self::GlobalInstructions => "Global instructions",
            Self::PromptTemplates => "Prompt templates",
            Self::Skills => "Skills",
            Self::Extensions => "Extensions",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::ProviderAccounts => "provider-accounts",
            Self::GlobalInstructions => "global-instructions",
            Self::PromptTemplates => "prompt-templates",
            Self::Skills => "skills",
            Self::Extensions => "extensions",
        }
    }
}

impl fmt::Display for AiSettingsSection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slug())
    }
}

impl FromStr for AiSettingsSection {
    type Err = Infallible;

    fn from_str(section: &str) -> Result<Self, Self::Err> {
        Ok(match section {
            "provider-accounts" => Self::ProviderAccounts,
            "global-instructions" => Self::GlobalInstructions,
            "prompt-templates" => Self::PromptTemplates,
            "skills" => Self::Skills,
            "extensions" => Self::Extensions,
            _ => Self::General,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum NavigationIntent {
    Home,
    Files {
        workspace: WorkspaceId,
        location: Option<FileLocation>,
    },
    Terminal {
        workspace: WorkspaceId,
        session_id: Option<String>,
    },
    Git {
        workspace: WorkspaceId,
        selection: Option<String>,
    },
    Preview {
        workspace: WorkspaceId,
    },
    Ai {
        workspace: WorkspaceId,
        conversation_id: Option<String>,
    },
    AiSettings {
        workspace: WorkspaceId,
        section: AiSettingsSection,
    },
}
