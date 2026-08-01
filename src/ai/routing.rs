use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiQuery {
    pub(super) session_id: Option<String>,
}

impl AiQuery {
    pub(super) fn with_session(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
        }
    }
}

impl From<&str> for AiQuery {
    fn from(query: &str) -> Self {
        let session_id = url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            matches!(key.as_ref(), "sessionId" | "session_id")
                .then(|| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
        Self { session_id }
    }
}

impl fmt::Display for AiQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        if let Some(session_id) = self.session_id.as_deref() {
            serializer.append_pair("sessionId", session_id);
        }
        formatter.write_str(&serializer.finish())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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

    const fn slug(self) -> &'static str {
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

impl std::str::FromStr for AiSettingsSection {
    type Err = std::convert::Infallible;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_links_round_trip_through_the_router() {
        let route = crate::app::Route::Ai {
            slug: "syntaxis-demo".into(),
            query: AiQuery::with_session("session/with spaces".into()),
        };
        let link = route.to_string();
        assert_eq!(
            link,
            "/workspaces/syntaxis-demo/ai?sessionId=session%2Fwith+spaces"
        );
        assert_eq!(link.parse::<crate::app::Route>().unwrap(), route);
    }

    #[test]
    fn settings_section_links_round_trip_through_the_router() {
        for section in AiSettingsSection::ALL {
            let route = crate::app::Route::AiSettings {
                slug: "syntaxis-demo".into(),
                section,
            };
            let link = route.to_string();
            assert_eq!(
                link,
                format!("/workspaces/syntaxis-demo/ai/settings/{}", section.slug())
            );
            assert_eq!(link.parse::<crate::app::Route>().unwrap(), route);
        }
    }

    #[test]
    fn legacy_settings_links_redirect_to_general() {
        let route = "/workspaces/syntaxis-demo/ai/settings"
            .parse::<crate::app::Route>()
            .unwrap();
        assert_eq!(
            route,
            crate::app::Route::AiSettings {
                slug: "syntaxis-demo".into(),
                section: AiSettingsSection::General,
            }
        );
    }

    #[test]
    fn unknown_settings_sections_fall_back_to_general() {
        let route = "/workspaces/syntaxis-demo/ai/settings/not-a-section"
            .parse::<crate::app::Route>()
            .unwrap();
        assert_eq!(
            route,
            crate::app::Route::AiSettings {
                slug: "syntaxis-demo".into(),
                section: AiSettingsSection::General,
            }
        );
    }
}
