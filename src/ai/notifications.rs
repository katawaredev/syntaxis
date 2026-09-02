use dioxus::prelude::*;
use futures_util::{StreamExt, future::FutureExt};
use syntaxis_notifications::{
    AppNotification, NotificationClientMessage, NotificationKind, NotificationServerMessage,
    NotificationTarget, PROTOCOL_VERSION,
};
use syntaxis_ui::prelude::NotificationPopover;

use crate::{
    app::Route,
    notification::{self, SystemNotification},
};

#[derive(Clone, Copy)]
pub(crate) struct NotificationCenter {
    items: Signal<Vec<AppNotification>>,
    viewing: Signal<Option<(String, NotificationTarget)>>,
    client: Coroutine<NotificationClientMessage>,
}

impl NotificationCenter {
    pub(crate) fn view(mut self, workspace_id: String, target: Option<NotificationTarget>) {
        let viewing = target.map(|target| (workspace_id, target));
        self.viewing.set(viewing.clone());
        if let Some((workspace_id, target)) = viewing {
            self.clear(workspace_id, target);
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

    pub(crate) fn clear(mut self, workspace_id: String, target: NotificationTarget) {
        self.items.write().retain(|notification| {
            notification.workspace_id != workspace_id || notification.target != target
        });
        self.client.send(NotificationClientMessage::Clear {
            workspace_id,
            target,
        });
    }

    fn clear_all(mut self) {
        let notifications = std::mem::take(&mut *self.items.write());
        for notification in notifications {
            self.client.send(NotificationClientMessage::Clear {
                workspace_id: notification.workspace_id,
                target: notification.target,
            });
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the reconnecting bidirectional websocket is one state machine"
)]
pub(crate) fn use_notification_center() -> NotificationCenter {
    let mut items = use_signal(Vec::<AppNotification>::new);
    let viewing = use_signal(|| None::<(String, NotificationTarget)>);
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
                let Ok(socket) =
                    super::api::notification_socket(dioxus::fullstack::WebSocketOptions::new())
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
                                            .send(NotificationClientMessage::Clear {
                                                workspace_id: notification.workspace_id,
                                                target: notification.target,
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
                                        .send(NotificationClientMessage::Clear {
                                            workspace_id: notification.workspace_id,
                                            target: notification.target,
                                        })
                                        .await;
                                } else {
                                    upsert(&mut items, notification.clone());
                                    show_system_notification(&notification);
                                }
                            }
                            NotificationServerMessage::Removed {
                                workspace_id,
                                target,
                            } => items.write().retain(|notification| {
                                notification.workspace_id != workspace_id
                                    || notification.target != target
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
    NotificationCenter {
        items,
        viewing,
        client,
    }
}

#[component]
pub(crate) fn NotificationMenu() -> Element {
    let center = use_context::<NotificationCenter>();
    let mut open = use_signal(|| false);
    let notifications = (center.items)();
    let count = notifications.len();
    rsx! {
        NotificationPopover {
            count,
            open: open(),
            on_open_change: move |next| open.set(next),
            on_clear_all: move |()| center.clear_all(),
            for notification in notifications {
                NotificationRow {
                    key: "{notification.workspace_id}:{notification.target.session_id()}",
                    notification,
                    on_open: move |(workspace_id, target)| {
                        center.clear(workspace_id, target);
                        open.set(false);
                    },
                }
            }
        }
    }
}

#[component]
fn NotificationRow(
    notification: AppNotification,
    on_open: EventHandler<(String, NotificationTarget)>,
) -> Element {
    let kind_label = match notification.kind {
        NotificationKind::Completed => "Completed",
        NotificationKind::Attention => "Needs attention",
        NotificationKind::Failed => "Failed",
    };
    let dot_class = match notification.kind {
        NotificationKind::Completed => "bg-success",
        NotificationKind::Attention => "bg-warning",
        NotificationKind::Failed => "bg-destructive",
    };
    let workspace_id = notification.workspace_id.clone();
    let notification_target = notification.target.clone();
    let target = notification_route(&notification);
    rsx! {
        Link {
            class: "block rounded-lg px-2.5 py-2.5 hover:bg-accent focus-visible:bg-accent focus-visible:outline-none",
            to: target,
            onclick: move |_| on_open.call((workspace_id.clone(), notification_target.clone())),
            div { class: "flex items-center gap-2",
                span { class: "size-1.5 shrink-0 rounded-full {dot_class}" }
                strong { class: "min-w-0 flex-1 truncate text-[11px]", "{notification.title}" }
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

fn is_viewed(
    notification: &AppNotification,
    viewing: Option<&(String, NotificationTarget)>,
) -> bool {
    viewing.is_some_and(|(workspace_id, target)| {
        notification.workspace_id == *workspace_id && notification.target == *target
    })
}

fn upsert(items: &mut Signal<Vec<AppNotification>>, notification: AppNotification) {
    let mut items = items.write();
    items.retain(|candidate| {
        candidate.workspace_id != notification.workspace_id
            || candidate.target != notification.target
    });
    items.push(notification);
    items.sort_by_key(|notification| std::cmp::Reverse(notification.created_at_ms));
}

fn show_system_notification(notification: &AppNotification) {
    let path = notification_route(notification).to_string();
    let title = match notification.kind {
        NotificationKind::Completed => format!("{} finished", notification.title),
        NotificationKind::Attention => {
            format!("{} needs attention", notification.title)
        }
        NotificationKind::Failed => format!("{} failed", notification.title),
    };
    notification::show(SystemNotification {
        title,
        body: format!("{} · {}", notification.workspace_name, notification.message),
        route: path,
        tag: format!(
            "syntaxis-{}-{}",
            notification.workspace_id,
            notification.target.session_id()
        ),
    });
}

fn notification_route(notification: &AppNotification) -> Route {
    match &notification.target {
        NotificationTarget::Agent { session_id } => Route::Ai {
            slug: notification.workspace_slug.clone(),
            query: super::AiQuery::with_session(session_id.clone()),
        },
        NotificationTarget::Terminal { session_id } => Route::Terminal {
            slug: notification.workspace_slug.clone(),
            query: crate::terminal::TerminalQuery::with_session(session_id.clone()),
        },
    }
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
