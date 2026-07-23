use futures_lite::future::block_on;

use crate::{
    MockWorkspaceFiles, RelativePath, WorkspaceAvailability, WorkspaceFiles, WorkspaceIcon,
    WorkspaceIconSymbol, WorkspaceId, WorkspaceRecord,
};

fn workspace() -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("mock"),
        slug: "mock".into(),
        name: "Mock".into(),
        root: "/mock".into(),
        icon: WorkspaceIcon::Symbol {
            name: WorkspaceIconSymbol::Folder,
        },
        profile: crate::WorkspaceProfile::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        availability: WorkspaceAvailability::Available,
    }
}

#[test]
fn mock_files_implement_the_full_mutation_contract() {
    let workspace = workspace();
    let files = MockWorkspaceFiles::default();
    let source = RelativePath::try_from("src").expect("source path should be valid");
    let file = RelativePath::try_from("src/main.rs").expect("file path should be valid");
    let copy = RelativePath::try_from("src/copy.rs").expect("copy path should be valid");
    let moved = RelativePath::try_from("src/moved.rs").expect("moved path should be valid");

    block_on(files.create_directory(&workspace, &source))
        .expect("source directory should be created");
    block_on(files.create_file(&workspace, &file)).expect("file should be created");
    let initial =
        block_on(files.read_text(&workspace, &file, 1024)).expect("file should be readable");
    block_on(files.write_text(
        &workspace,
        &file,
        "fn main() {}",
        Some(&initial.version),
        1024,
    ))
    .expect("file should be writable");
    block_on(files.copy(&workspace, &file, &copy)).expect("file should be copied");
    block_on(files.move_entry(&workspace, &copy, &moved)).expect("file should be moved");
    assert_eq!(
        block_on(files.list(&workspace, &source))
            .expect("source directory should be listable")
            .len(),
        2
    );
    block_on(files.delete(&workspace, &moved)).expect("moved file should be deleted");
    block_on(files.stat(&workspace, &moved)).expect_err("deleted file must not exist");
}
