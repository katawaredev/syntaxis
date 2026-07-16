use dioxus::prelude::*;
use dioxus_primitives::popover::{PopoverContent, PopoverRoot, PopoverTrigger};
use futures_util::{future::FutureExt, StreamExt};
use syntaxis_agent::{
    AgentNotification, AgentNotificationKind, NotificationClientMessage, NotificationServerMessage,
    PROTOCOL_VERSION,
};
use syntaxis_ui::prelude::{AppIcon, Icon};

use crate::app::Route;

#[derive(Clone, Copy)]
pub(crate) struct AgentNotificationCenter {
    items: Signal<Vec<AgentNotification>>,
    viewing: Signal<Option<(String, String)>>,
    client: Coroutine<NotificationClientMessage>,
}

impl AgentNotificationCenter {
    pub(crate) fn view(mut self, workspace_id: String, session_id: Option<String>) {
        let viewing = session_id.map(|session_id| (workspace_id, session_id));
        self.viewing.set(viewing.clone());
        if let Some((workspace_id, session_id)) = viewing {
            self.clear(workspace_id, session_id);
        }
    }

    pub(crate) fn stop_viewing(mut self, workspace_id: &str) {
        if self
            .viewing
            .peek()
            .as_ref()
            .is_some_and(|(current, _)| current == workspace_id)
        {
            self.viewing.set(None);
        }
    }

    pub(crate) fn clear(mut self, workspace_id: String, session_id: String) {
        self.items.write().retain(|notification| {
            notification.workspace_id != workspace_id || notification.session_id != session_id
        });
        self.client.send(NotificationClientMessage::ClearSession {
            workspace_id,
            session_id,
        });
    }
}

#[allow(clippy::too_many_lines)] // The reconnecting bidirectional websocket is one state machine.
pub(crate) fn use_agent_notification_center() -> AgentNotificationCenter {
    let mut items = use_signal(Vec::<AgentNotification>::new);
    let viewing = use_signal(|| None::<(String, String)>);
    let client = use_coroutine(
        move |mut outgoing: UnboundedReceiver<NotificationClientMessage>| async move {
            let mut attempt = 0_u8;
            loop {
                if attempt > 0 {
                    dioxus_sdk_time::sleep(std::time::Duration::from_millis(reconnect_delay_ms(
                        attempt,
                    )))
                    .await;
                }
                let Ok(socket) = super::api::agent_notification_socket(
                    dioxus::fullstack::WebSocketOptions::new(),
                )
                .await
                else {
                    attempt = attempt.saturating_add(1).min(8);
                    continue;
                };
                if socket
                    .send(NotificationClientMessage::Hello {
                        version: PROTOCOL_VERSION,
                    })
                    .await
                    .is_err()
                {
                    attempt = attempt.saturating_add(1).min(8);
                    continue;
                }
                loop {
                    let send = outgoing.next().fuse();
                    let receive = socket.recv().fuse();
                    futures_util::pin_mut!(send, receive);
                    match futures_util::future::select(send, receive).await {
                        futures_util::future::Either::Left((Some(message), _)) => {
                            if socket.send(message).await.is_err() {
                                attempt = attempt.saturating_add(1).min(8);
                                break;
                            }
                        }
                        futures_util::future::Either::Left((None, _)) => return,
                        futures_util::future::Either::Right((Ok(message), _)) => match message {
                            NotificationServerMessage::Hello { version }
                                if version == PROTOCOL_VERSION =>
                            {
                                attempt = 0;
                            }
                            NotificationServerMessage::Snapshot { notifications } => {
                                let mut visible = Vec::new();
                                for notification in notifications {
                                    if is_viewed(&notification, viewing().as_ref()) {
                                        let _ = socket
                                            .send(NotificationClientMessage::ClearSession {
                                                workspace_id: notification.workspace_id,
                                                session_id: notification.session_id,
                                            })
                                            .await;
                                    } else {
                                        visible.push(notification);
                                    }
                                }
                                visible.sort_by_key(|notification| {
                                    std::cmp::Reverse(notification.created_at_ms)
                                });
                                items.set(visible);
                            }
                            NotificationServerMessage::Upsert { notification } => {
                                if is_viewed(&notification, viewing().as_ref()) {
                                    let _ = socket
                                        .send(NotificationClientMessage::ClearSession {
                                            workspace_id: notification.workspace_id,
                                            session_id: notification.session_id,
                                        })
                                        .await;
                                } else {
                                    upsert(&mut items, notification.clone());
                                    show_browser_notification(notification);
                                }
                            }
                            NotificationServerMessage::Removed {
                                workspace_id,
                                session_id,
                            } => items.write().retain(|notification| {
                                notification.workspace_id != workspace_id
                                    || notification.session_id != session_id
                            }),
                            NotificationServerMessage::Error { .. }
                            | NotificationServerMessage::Pong { .. }
                            | NotificationServerMessage::Hello { .. } => {}
                        },
                        futures_util::future::Either::Right((Err(_), _)) => {
                            attempt = attempt.saturating_add(1).min(8);
                            break;
                        }
                    }
                }
            }
        },
    );
    AgentNotificationCenter {
        items,
        viewing,
        client,
    }
}

#[component]
pub(crate) fn AgentNotificationMenu() -> Element {
    let center = use_context::<AgentNotificationCenter>();
    let mut open = use_signal(|| false);
    let notifications = (center.items)();
    let count = notifications.len();
    let badge_count = count.min(99).to_string();
    rsx! {
        PopoverRoot {
            class: "relative shrink-0",
            is_modal: false,
            open: open(),
            on_open_change: move |next| open.set(next),
            PopoverTrigger {
                class: if open() { "relative grid size-8 place-items-center rounded-lg bg-accent text-foreground" } else { "relative grid size-8 place-items-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground" },
                aria_label: if count == 0 { "Agent notifications".to_owned() } else { format!("Agent notifications, {count} unread") },
                title: "Agent notifications",
                Icon { icon: AppIcon::Bell, size: 15 }
                if count > 0 {
                    span { class: "absolute -top-0.5 -right-0.5 grid min-w-4 h-4 place-items-center rounded-full bg-primary px-1 text-[8px] font-semibold leading-none text-primary-foreground ring-2 ring-background",
                        "{badge_count}"
                    }
                }
            }
            PopoverContent { class: "absolute top-[calc(100%+6px)] right-0 z-90 w-[min(360px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center justify-between border-b border-border px-3 py-2.5",
                    strong { class: "text-xs", "Agent notifications" }
                    if count > 0 {
                        span { class: "text-[9px] text-muted-foreground", "{count} need attention" }
                    }
                }
                div { class: "max-h-[min(420px,70vh)] overflow-y-auto p-1.5",
                    if notifications.is_empty() {
                        div { class: "px-4 py-8 text-center",
                            div { class: "mx-auto grid size-8 place-items-center rounded-full bg-secondary text-muted-foreground",
                                Icon { icon: AppIcon::Bell, size: 14 }
                            }
                            p { class: "mt-2 text-xs font-medium", "Nothing needs attention" }
                            p { class: "mt-1 text-[10px] text-muted-foreground",
                                "Completed agents and questions will appear here."
                            }
                        }
                    }
                    for notification in notifications {
                        NotificationRow {
                            key: "{notification.workspace_id}:{notification.session_id}",
                            notification,
                            on_open: move |(workspace_id, session_id)| {
                                center.clear(workspace_id, session_id);
                                open.set(false);
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NotificationRow(
    notification: AgentNotification,
    on_open: EventHandler<(String, String)>,
) -> Element {
    let kind_label = match notification.kind {
        AgentNotificationKind::Completed => "Completed",
        AgentNotificationKind::Attention => "Needs attention",
        AgentNotificationKind::Failed => "Failed",
    };
    let dot_class = match notification.kind {
        AgentNotificationKind::Completed => "bg-success",
        AgentNotificationKind::Attention => "bg-warning",
        AgentNotificationKind::Failed => "bg-destructive",
    };
    let workspace_id = notification.workspace_id.clone();
    let session_id = notification.session_id.clone();
    let target = Route::Ai {
        slug: notification.workspace_slug.clone(),
        query: super::AiQuery::with_session(notification.session_id.clone()),
    };
    rsx! {
        Link {
            class: "block rounded-lg px-2.5 py-2.5 hover:bg-accent focus-visible:bg-accent focus-visible:outline-none",
            to: target,
            onclick: move |_| on_open.call((workspace_id.clone(), session_id.clone())),
            div { class: "flex items-center gap-2",
                span { class: "size-1.5 shrink-0 rounded-full {dot_class}" }
                strong { class: "min-w-0 flex-1 truncate text-[11px]", "{notification.session_title}" }
                time { class: "shrink-0 text-[9px] text-muted-foreground",
                    {notification_age(notification.created_at_ms)}
                }
            }
            div { class: "mt-1 flex items-center gap-1.5 pl-3.5 text-[9px] text-muted-foreground",
                span { class: "truncate", "{notification.workspace_name}" }
                span { "·" }
                span { class: "shrink-0", "{kind_label}" }
            }
            p { class: "mt-1 line-clamp-2 pl-3.5 text-[10px] leading-relaxed text-muted-foreground",
                "{notification.message}"
            }
        }
    }
}

fn is_viewed(notification: &AgentNotification, viewing: Option<&(String, String)>) -> bool {
    viewing.is_some_and(|(workspace_id, session_id)| {
        notification.workspace_id == *workspace_id && notification.session_id == *session_id
    })
}

fn upsert(items: &mut Signal<Vec<AgentNotification>>, notification: AgentNotification) {
    let mut items = items.write();
    items.retain(|candidate| {
        candidate.workspace_id != notification.workspace_id
            || candidate.session_id != notification.session_id
    });
    items.push(notification);
    items.sort_by_key(|notification| std::cmp::Reverse(notification.created_at_ms));
}

fn show_browser_notification(notification: AgentNotification) {
    let path = Route::Ai {
        slug: notification.workspace_slug,
        query: super::AiQuery::with_session(notification.session_id.clone()),
    }
    .to_string();
    let title = match notification.kind {
        AgentNotificationKind::Completed => format!("{} finished", notification.session_title),
        AgentNotificationKind::Attention => {
            format!("{} needs attention", notification.session_title)
        }
        AgentNotificationKind::Failed => format!("{} failed", notification.session_title),
    };
    let eval = document::eval(
        r#"
        const [title, body, path, tag] = await dioxus.recv();
        if (!("Notification" in globalThis) || Notification.permission !== "granted") return;
        const alert = new Notification(title, { body, tag });
        alert.onclick = () => {
            globalThis.focus();
            globalThis.location.href = path;
            alert.close();
        };
        "#,
    );
    let _ = eval.send((
        title,
        format!("{} · {}", notification.workspace_name, notification.message),
        path,
        format!(
            "syntaxis-agent-{}-{}",
            notification.workspace_id, notification.session_id
        ),
    ));
}

fn notification_age(timestamp: u64) -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(timestamp);
    let minutes = now.saturating_sub(timestamp) / 60_000;
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1_439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1_440),
    }
}

fn reconnect_delay_ms(attempt: u8) -> u64 {
    500_u64
        .saturating_mul(1_u64 << attempt.saturating_sub(1).min(5))
        .min(10_000)
}
