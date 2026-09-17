//! Shared overlay host.

#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::*;

#[component]
#[allow(
    clippy::too_many_arguments,
    reason = "the overlay host receives the existing independent reactive controller signals"
)]
pub(super) fn FilesOverlays(
    mut file_dialog: Signal<Option<FileActionDialog>>,
    close_request: Signal<Option<CloseRequest>>,
    mut git_revert_request: Signal<Option<GitRevertRequest>>,
    mut go_to_line: Signal<bool>,
    mut toast: Signal<Option<ToastState>>,
    workspace: Signal<Option<WorkspaceRecord>>,
    editor_configs: Signal<Vec<EditorConfigSource>>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    loading_path: Signal<Option<String>>,
    loading_documents: Signal<BTreeSet<String>>,
    pending: Signal<bool>,
    refresh: Signal<u64>,
    diff: Signal<Option<UnifiedDiff>>,
    editor_selection: Signal<EditorSelection>,
    command_revision: Signal<u64>,
    editor_command: Signal<Option<EditorCommand>>,
) -> Element {
    let files = use_context::<crate::FilesPorts>();
    let mutation_files = files.clone();
    let discard_files = files;
    rsx! {
        if let Some(dialog) = file_dialog() {
            FileMutationDialog {
                dialog: dialog.clone(),
                on_close: move |()| file_dialog.set(None),
                on_submit: move |destination| {
                    file_dialog.set(None);
                    run_file_action(
                        mutation_files.clone(),
                        dialog.clone(),
                        destination,
                        workspace(),
                        editor_configs(),
                        documents,
                        active_path,
                        loading_path,
                        loading_documents,
                        pending,
                        refresh,
                        toast,
                    );
                },
            }
        }
        if let Some(request) = close_request() {
            DirtyClosePrompt {
                request,
                workspace,
                documents,
                active_path,
                close_request,
                toast,
            }
        }
        if let Some(request) = git_revert_request() {
            GitDiscardPrompt {
                path: request.path.clone(),
                original: request.action == RevertAction::Original,
                on_close: move |()| git_revert_request.set(None),
                on_confirm: move |()| {
                    git_revert_request.set(None);
                    discard_git_change(
                        request.path.clone(),
                        request.action == RevertAction::Original,
                        GitDiscardContext {
                            files: discard_files.clone(),
                            workspace: workspace(),
                            documents,
                            active_path,
                            refresh,
                            diff,
                            toast,
                        },
                    );
                },
            }
        }
        if go_to_line() {
            GoToLineDialog {
                current: editor_selection().line.max(1),
                on_close: move |()| go_to_line.set(false),
                on_submit: move |line| {
                    issue_command(
                        command_revision,
                        editor_command,
                        EditorCommandKind::GoToLine {
                            line,
                        },
                    );
                    go_to_line.set(false);
                },
            }
        }
        if let Some(toast_state) = toast() {
            Toast {
                message: toast_state.message,
                tone: toast_state.tone,
                on_close: move |()| toast.set(None),
            }
        }
    }
}
