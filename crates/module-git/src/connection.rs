use dioxus::prelude::*;
use syntaxis_ui::prelude::{Button, ButtonKind, Field, TextInput, TextInputType};

use crate::{GitConnectionSettings, GitPorts};

/// Settings are submitted explicitly and never read back with a saved token.
#[allow(
    clippy::clone_on_ref_ptr,
    reason = "PortHandle is Rc in the browser and Arc in native runtimes"
)]
#[component]
pub fn GitConnectionForm() -> Element {
    let ports = use_context::<GitPorts>();
    let Some(port) = ports.connection().cloned() else {
        return rsx! {};
    };
    let mut origin = use_signal(|| "https://github.com".to_owned());
    let mut proxy = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut token = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    rsx! {
        details { class: "rounded-md border border-border p-3 text-xs",
            summary { class: "cursor-pointer font-medium", "Git connection settings" }
            div { class: "mt-3 grid gap-3",
                p { "Settings last until reload. A trusted proxy can read repository traffic and credentials. Leave proxy empty for direct connections; GitHub requires a proxy. Saving replaces this host's settings; an empty token clears its credentials." }
                Field { control_id: "git-host", label: "Git host (HTTPS)",
                    TextInput { value: origin(), disabled: busy(), oninput: move |event: FormEvent| origin.set(event.value()) }
                }
                Field { control_id: "git-proxy", label: "Trusted CORS proxy URL (optional)",
                    TextInput { value: proxy(), disabled: busy(), placeholder: "https://your-git-proxy.example", oninput: move |event: FormEvent| proxy.set(event.value()) }
                }
                Field { control_id: "git-username", label: "Username (optional for GitHub tokens)",
                    TextInput { value: username(), disabled: busy(), oninput: move |event: FormEvent| username.set(event.value()) }
                }
                Field { control_id: "git-token", label: "Access token",
                    TextInput { value: token(), input_type: TextInputType::Password, disabled: busy(), oninput: move |event: FormEvent| token.set(event.value()) }
                }
                Field { control_id: "git-author", label: "Commit author name",
                    TextInput { value: name(), disabled: busy(), oninput: move |event: FormEvent| name.set(event.value()) }
                }
                Field { control_id: "git-email", label: "Commit author email",
                    TextInput { value: email(), disabled: busy(), oninput: move |event: FormEvent| email.set(event.value()) }
                }
                if let Some(message) = notice() { p { role: "status", "{message}" } }
                Button { label: "Save for this session", kind: ButtonKind::Secondary, disabled: busy(), onclick: move |_| {
                    let port = port.clone();
                    let settings = GitConnectionSettings { origin: origin(), proxy: proxy(), username: username(), token: token(), name: name(), email: email() };
                    busy.set(true);
                    spawn(async move {
                        match port.configure(settings).await {
                            Ok(()) => { token.set(String::new()); notice.set(Some("Git settings saved for this session.".into())); }
                            Err(error) => notice.set(Some(error.message)),
                        }
                        busy.set(false);
                    });
                } }
            }
        }
    }
}
