use dioxus::prelude::*;

pub(super) fn clear_saved_draft(draft_key: &str) {
    let draft_key = draft_key.to_owned();
    spawn(async move {
        let _ = crate::storage::remove(draft_key).await;
    });
}

pub(super) fn use_persisted_draft(draft: Signal<String>, draft_key: &str) -> Signal<bool> {
    let draft_key = draft_key.to_owned();
    let mut requested_key = use_signal(String::new);
    let mut loaded_key = use_signal(|| None::<String>);
    let mut dirty = use_signal(|| false);
    let mut save_revision = use_signal(|| 0_u64);
    use_effect(use_reactive((&draft_key,), move |(key,)| {
        requested_key.set(key.clone());
        loaded_key.set(None);
        dirty.set(false);
        let mut draft = draft;
        draft.set(String::new());
        spawn(async move {
            let stored = crate::storage::get(key.clone())
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
            if requested_key.peek().as_str() != key {
                return;
            }
            if !*dirty.peek() {
                draft.set(stored);
            }
            loaded_key.set(Some(key));
        });
    }));
    use_effect(move || {
        let value = draft();
        let Some(key) = loaded_key() else {
            return;
        };
        *save_revision.write() += 1;
        let revision = save_revision();
        spawn(async move {
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
            if save_revision() != revision {
                return;
            }
            if value.is_empty() {
                let _ = crate::storage::remove(key).await;
            } else {
                let _ = crate::storage::set(key, value).await;
            }
        });
    });
    dirty
}
