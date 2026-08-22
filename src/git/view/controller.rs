use dioxus::prelude::*;
use syntaxis_git::BranchComparison;
use syntaxis_ui::prelude::Tone;

use crate::client_error::server_error_message;

use super::super::api;
use super::super::operations::{
    Mutation, RepositoryAction, RepositoryActionSuccess, run_mutation, run_repository_action,
};
use super::super::repository::SelectedChange;
use super::GitDialog;

#[derive(Clone, Copy)]
pub(super) struct RepositoryActionSignals {
    pub pending: Signal<bool>,
    pub refreshing: Signal<bool>,
    pub operation_error: Signal<Option<String>>,
    pub selected: Signal<Option<SelectedChange>>,
    pub dialog: Signal<GitDialog>,
    pub refresh_key: Signal<u64>,
    pub toast: Signal<Option<(String, Tone)>>,
}

pub(super) fn mutation_handler(
    slug: String,
    mut pending: Signal<bool>,
    mut operation_error: Signal<Option<String>>,
    mut selected: Signal<Option<SelectedChange>>,
    mut dialog: Signal<GitDialog>,
    mut refresh_key: Signal<u64>,
) -> EventHandler<Mutation> {
    EventHandler::new(move |mutation| {
        let slug = slug.clone();
        pending.set(true);
        operation_error.set(None);
        spawn(async move {
            let result = run_mutation(slug, mutation).await;
            pending.set(false);
            match result {
                Ok(success) => {
                    selected.set(success.selection);
                    if success.closes_dialog {
                        dialog.set(GitDialog::None);
                    }
                    *refresh_key.write() += 1;
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    })
}

pub(super) fn repository_action_handler(
    slug: String,
    signals: RepositoryActionSignals,
) -> EventHandler<RepositoryAction> {
    let RepositoryActionSignals {
        mut pending,
        mut refreshing,
        mut operation_error,
        mut selected,
        mut dialog,
        mut refresh_key,
        mut toast,
    } = signals;
    EventHandler::new(move |action: RepositoryAction| {
        let slug = slug.clone();
        let refresh_action = action.refresh_only();
        let show_success = action.shows_success_message();
        pending.set(true);
        if refresh_action {
            refreshing.set(true);
        }
        operation_error.set(None);
        spawn(async move {
            let result = run_repository_action(slug, action).await;
            pending.set(false);
            if refresh_action {
                refreshing.set(false);
                *refresh_key.write() += 1;
            }
            match result {
                Ok(RepositoryActionSuccess::Complete(message)) => {
                    if !refresh_action {
                        dialog.set(GitDialog::None);
                        selected.set(None);
                        *refresh_key.write() += 1;
                    }
                    if show_success {
                        toast.set(Some((message, Tone::Success)));
                    }
                }
                Ok(RepositoryActionSuccess::MergeConflicts(count)) => {
                    dialog.set(GitDialog::None);
                    *refresh_key.write() += 1;
                    operation_error.set(Some(format!(
                        "Merge stopped with conflicts in {count} file(s). Resolve the highlighted files or abort the merge."
                    )));
                }
                Ok(RepositoryActionSuccess::RebaseStopped { conflicts, message }) => {
                    dialog.set(GitDialog::None);
                    selected.set(None);
                    *refresh_key.write() += 1;
                    let message = if conflicts == 0 {
                        format!(
                            "{message} The rebase remains paused; continue, skip this commit, use the terminal, or abort."
                        )
                    } else {
                        format!(
                            "Rebase stopped with conflicts in {conflicts} file(s). Resolve the highlighted files, then continue or abort the rebase."
                        )
                    };
                    toast.set(Some((message, Tone::Warning)));
                }
                Ok(RepositoryActionSuccess::ForceWithLeaseRequired(message)) => {
                    operation_error.set(Some(message));
                    dialog.set(GitDialog::ForcePush);
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    })
}

pub(super) fn compare_handler(
    slug: String,
    mut pending: Signal<bool>,
    mut operation_error: Signal<Option<String>>,
    mut comparison: Signal<Option<BranchComparison>>,
) -> EventHandler<(String, String)> {
    EventHandler::new(move |(base, head)| {
        let slug = slug.clone();
        pending.set(true);
        operation_error.set(None);
        comparison.set(None);
        spawn(async move {
            match api::compare(slug, base, head).await {
                Ok(result) => comparison.set(Some(result)),
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
            pending.set(false);
        });
    })
}
