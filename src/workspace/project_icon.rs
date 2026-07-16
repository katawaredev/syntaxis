use dioxus::prelude::*;
use syntaxis_workspace::WorkspaceIcon;

#[component]
pub fn ProjectIcon(
    name: String,
    icon: WorkspaceIcon,
    #[props(default = false)] compact: bool,
) -> Element {
    let class = if compact {
        "grid size-7 shrink-0 place-items-center overflow-hidden rounded-md border border-border/70 bg-muted/50 text-[9px] font-bold text-muted-foreground"
    } else {
        "grid size-10 shrink-0 place-items-center overflow-hidden rounded-lg border border-border/70 bg-muted/50 text-[10px] font-bold text-muted-foreground shadow-sm"
    };
    let initial = project_initial(&name);
    rsx! {
        span { class,
            match icon {
                WorkspaceIcon::Image { data_url: Some(source), .. } => rsx! {
                    img { class: "size-full object-contain", src: source, alt: "" }
                },
                WorkspaceIcon::Image { data_url: None, .. } => rsx! {
                    {initial.clone()}
                },
                WorkspaceIcon::Symbol { .. } => rsx! {
                    {initial}
                },
            }
        }
    }
}

fn project_initial(name: &str) -> String {
    name.trim().chars().next().map_or_else(
        || "?".to_string(),
        |character| character.to_uppercase().collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::project_initial;

    #[test]
    fn derives_a_single_uppercase_initial_from_the_project_name() {
        assert_eq!(project_initial("devbox"), "D");
        assert_eq!(project_initial("  syntaxis"), "S");
        assert_eq!(project_initial(""), "?");
    }
}
