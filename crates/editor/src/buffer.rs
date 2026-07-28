use syntaxis_workspace::FileVersion;

use crate::EditorConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferStatus {
    Clean,
    Dirty,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalChange {
    Unchanged,
    Reload,
    Conflict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorBuffer {
    pub path: String,
    pub saved_contents: String,
    pub contents: String,
    pub version: FileVersion,
    pub status: BufferStatus,
    pub config: EditorConfig,
    pending_save: Option<String>,
}

impl EditorBuffer {
    pub fn open(
        path: impl Into<String>,
        contents: String,
        version: FileVersion,
        config: EditorConfig,
    ) -> Self {
        Self {
            path: path.into(),
            saved_contents: contents.clone(),
            contents,
            version,
            status: BufferStatus::Clean,
            config,
            pending_save: None,
        }
    }

    pub fn edit(&mut self, contents: String) {
        self.contents = contents;
        self.update_status();
    }

    pub fn apply_edits(&mut self, edits: &[(usize, usize, String)]) -> bool {
        let Some(edits) = normalize_edits(&self.contents, edits) else {
            return false;
        };
        for (start, end, text) in edits {
            self.contents.replace_range(start..end, text);
        }
        self.update_status();
        true
    }

    fn update_status(&mut self) {
        self.status = if self.contents == self.saved_contents {
            BufferStatus::Clean
        } else {
            BufferStatus::Dirty
        };
    }

    pub fn mark_saved(&mut self, contents: String, version: FileVersion) {
        self.contents.clone_from(&contents);
        self.saved_contents = contents;
        self.version = version;
        self.status = BufferStatus::Clean;
        self.pending_save = None;
    }

    pub fn begin_save(&mut self, contents: String) {
        self.pending_save = Some(contents);
    }

    pub fn finish_save(&mut self, contents: String, version: FileVersion) {
        self.saved_contents = contents;
        self.version = version;
        self.pending_save = None;
        self.status = if self.contents == self.saved_contents {
            BufferStatus::Clean
        } else {
            BufferStatus::Dirty
        };
    }

    pub fn cancel_save(&mut self) {
        self.pending_save = None;
    }

    pub fn has_pending_save(&self) -> bool {
        self.pending_save.is_some()
    }

    pub fn revert(&mut self) {
        self.contents.clone_from(&self.saved_contents);
        self.status = BufferStatus::Clean;
    }

    pub fn reconcile_external(&mut self, contents: String, version: FileVersion) -> ExternalChange {
        if self.pending_save.as_deref() == Some(&contents) {
            self.finish_save(contents, version);
            return ExternalChange::Unchanged;
        }
        if contents == self.saved_contents {
            self.version = version;
            return ExternalChange::Unchanged;
        }
        if contents == self.contents || self.status == BufferStatus::Clean {
            self.mark_saved(contents, version);
            return ExternalChange::Reload;
        }

        self.status = BufferStatus::Conflict;
        ExternalChange::Conflict
    }

    pub fn rename(&mut self, path: impl Into<String>) {
        self.path = path.into();
    }

    pub fn is_dirty(&self) -> bool {
        self.status != BufferStatus::Clean
    }
}

fn normalize_edits<'a>(
    contents: &str,
    edits: &'a [(usize, usize, String)],
) -> Option<Vec<(usize, usize, &'a str)>> {
    let mut normalized = edits
        .iter()
        .map(|(edit_start, edit_end, text)| {
            let start = utf16_offset_to_byte(contents, *edit_start)?;
            let end = utf16_offset_to_byte(contents, *edit_end)?;
            (start <= end).then_some((start, end, text.as_str()))
        })
        .collect::<Option<Vec<_>>>()?;
    normalized.sort_unstable_by(|left, right| right.0.cmp(&left.0).then(right.1.cmp(&left.1)));
    normalized
        .windows(2)
        .all(|pair| pair[0].0 >= pair[1].1)
        .then_some(normalized)
}

fn utf16_offset_to_byte(value: &str, target: usize) -> Option<usize> {
    let mut utf16_offset = 0;
    for (byte_offset, character) in value.char_indices() {
        if utf16_offset == target {
            return Some(byte_offset);
        }
        utf16_offset += character.len_utf16();
        if utf16_offset > target {
            return None;
        }
    }
    (utf16_offset == target).then_some(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(length: u64, modified_unix_nanos: u128) -> FileVersion {
        FileVersion {
            length,
            modified_unix_nanos,
        }
    }

    #[test]
    fn editing_and_reverting_preserve_the_saved_snapshot() {
        let mut buffer = EditorBuffer::open(
            "src/main.rs",
            "old".into(),
            version(3, 1),
            EditorConfig::default(),
        );
        buffer.edit("new".into());
        assert_eq!(buffer.status, BufferStatus::Dirty);
        buffer.revert();
        assert_eq!(buffer.contents, "old");
        assert_eq!(buffer.status, BufferStatus::Clean);
    }

    #[test]
    fn external_changes_ignore_own_saves_and_conflict_only_with_unsaved_work() {
        let mut clean = EditorBuffer::open(
            "file.txt",
            "old".into(),
            version(3, 1),
            EditorConfig::default(),
        );
        assert_eq!(
            clean.reconcile_external("external".into(), version(8, 2)),
            ExternalChange::Reload
        );
        assert_eq!(clean.contents, "external");

        clean.edit("mine".into());
        clean.begin_save("mine".into());
        clean.edit("mine, still typing".into());
        assert_eq!(
            clean.reconcile_external("mine".into(), version(4, 3)),
            ExternalChange::Unchanged
        );
        assert_eq!(clean.status, BufferStatus::Dirty);
        assert_eq!(clean.saved_contents, "mine");

        assert_eq!(
            clean.reconcile_external("someone else's edit".into(), version(20, 4)),
            ExternalChange::Conflict
        );
        assert_eq!(clean.status, BufferStatus::Conflict);
        assert_eq!(clean.contents, "mine, still typing");
    }

    #[test]
    fn finishing_a_save_does_not_overwrite_edits_made_while_it_was_running() {
        let mut buffer = EditorBuffer::open(
            "file.txt",
            "old".into(),
            version(3, 1),
            EditorConfig::default(),
        );
        buffer.edit("saved snapshot".into());
        buffer.begin_save("saved snapshot".into());
        buffer.edit("newer local edit".into());

        buffer.finish_save("saved snapshot".into(), version(14, 2));

        assert_eq!(buffer.contents, "newer local edit");
        assert_eq!(buffer.saved_contents, "saved snapshot");
        assert_eq!(buffer.status, BufferStatus::Dirty);
    }

    #[test]
    fn editor_transactions_apply_utf16_offsets_and_multiple_changes() {
        let mut buffer = EditorBuffer::open(
            "file.txt",
            "a😀bc".into(),
            version(7, 1),
            EditorConfig::default(),
        );
        assert!(buffer.apply_edits(&[(1, 3, "🙂".into()), (4, 5, "d".into()),]));
        assert_eq!(buffer.contents, "a🙂bd");
        assert_eq!(buffer.status, BufferStatus::Dirty);
    }

    #[test]
    fn editor_transactions_reject_offsets_inside_surrogate_pairs() {
        let mut buffer = EditorBuffer::open(
            "file.txt",
            "😀".into(),
            version(4, 1),
            EditorConfig::default(),
        );
        assert!(!buffer.apply_edits(&[(1, 2, String::new())]));
        assert_eq!(buffer.contents, "😀");
    }
}
