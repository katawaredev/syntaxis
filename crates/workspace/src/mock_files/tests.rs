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
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        availability: WorkspaceAvailability::Available,
    }
}

#[test]
fn mock_files_implement_the_full_mutation_contract() {
    let workspace = workspace();
    let files = MockWorkspaceFiles::default();
    let source = RelativePath::try_from("src").unwrap();
    let file = RelativePath::try_from("src/main.rs").unwrap();
    let copy = RelativePath::try_from("src/copy.rs").unwrap();
    let moved = RelativePath::try_from("src/moved.rs").unwrap();

    block_on(files.create_directory(&workspace, &source)).unwrap();
    block_on(files.create_file(&workspace, &file)).unwrap();
    let initial = block_on(files.read_text(&workspace, &file, 1024)).unwrap();
    block_on(files.write_text(
        &workspace,
        &file,
        "fn main() {}",
        Some(&initial.version),
        1024,
    ))
    .unwrap();
    block_on(files.copy(&workspace, &file, &copy)).unwrap();
    block_on(files.move_entry(&workspace, &copy, &moved)).unwrap();
    assert_eq!(block_on(files.list(&workspace, &source)).unwrap().len(), 2);
    block_on(files.delete(&workspace, &moved)).unwrap();
    assert!(block_on(files.stat(&workspace, &moved)).is_err());
}
