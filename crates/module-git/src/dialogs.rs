#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::{
    AExtension, ActionCallback, AnyStorage, AreaExtension, BaseExtension, BranchComparison,
    BranchInfo, Button, ButtonExtension, ButtonKind, Checkbox, CommitInfo, CommitRequest,
    ControlSize, DataExtension, DialogActions, DialogForm, Element, EventHandler, Field,
    FieldsetExtension, FormEvent, FormExtension, GitDialog, GitPorts, GlobalAttributesExtension, HasFormData,
    HasKeyboardData, History, IframeExtension, InputExtension, Key, KeyboardEvent, LiExtension,
    LinkExtension, MapExtension, MetaExtension, MeterExtension, Modal, ObjectExtension,
    OptgroupExtension, OptionExtension, OutputExtension, ParamExtension, ProgressExtension, Props,
    RawPatch, ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt,
    ReadableResultExt, ReadableStrExt, ReadableVecExt, RemoteInfo, RemoteRequest, SelectExtension,
    SlotExtension, Storage, StyleExtension, SvgAttributesExtension, TagInfo, TagRequest, TextArea,
    TextInput, TextInputType, TextareaExtension, TrackExtension, WorkspaceRecord, WritableExt, component,
    dioxus_core, dioxus_elements, dioxus_signals, display_remote_url, remote_request, rsx,
    short_oid, spawn, use_context, use_signal,
};

#[path = "dialogs/commit.rs"]
mod commit;
#[path = "dialogs/merge.rs"]
mod merge;
#[path = "dialogs/rebase.rs"]
mod rebase;
#[path = "dialogs/refs.rs"]
mod refs;
#[path = "dialogs/remotes.rs"]
mod remotes;

pub(super) use commit::{CommitDialog, SigningDialog};
pub(super) use merge::{
    AbortMergeDialog, CommitHistoryActionDialog, CompareMergeDialog, DiscardAllDialog,
    ForcePushDialog,
};
pub(super) use rebase::{AbortRebaseDialog, PullRebaseDialog, SkipRebaseDialog};
pub(super) use refs::{BranchDialog, TagDialog};
pub(super) use remotes::{RemoteDialog, RemoveRemoteDialog};
