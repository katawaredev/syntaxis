pub use syntaxis_app_shell::{AiQuery, AiSettingsSection};

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
