use super::*;
fn bookmark(path: &str) -> PathBookmark {
    PathBookmark {
        id: String::new(),
        scope: "ignored".into(),
        path: path.into(),
        name: path.into(),
        kind: BookmarkKind::Folder,
    }
}
#[test]
fn isolates_connections_deduplicates_and_reorders() {
    let mut items = vec![];
    for (scope, path) in [
        ("local", "/a"),
        ("remote:a", "/a"),
        ("remote:b", "/a"),
        ("remote:a", "/b"),
        ("remote:a", "/a"),
    ] {
        apply(&mut items, scope, BookmarkAction::Add, bookmark(path)).unwrap();
    }
    assert_eq!(items.len(), 4);
    let target = items[3].clone();
    apply(&mut items, "remote:a", BookmarkAction::Up, target.clone()).unwrap();
    assert_eq!(items[1].path, "/b");
    assert_eq!(items[2].scope, "remote:b");
    apply(
        &mut items,
        "remote:b",
        BookmarkAction::Remove,
        target.clone(),
    )
    .unwrap();
    assert_eq!(items.len(), 4);
    apply(&mut items, "remote:a", BookmarkAction::Remove, target).unwrap();
    assert_eq!(items.len(), 3);
    let encoded = serde_json::to_string(&items).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<PathBookmark>>(&encoded)
            .unwrap()
            .len(),
        3
    );
}
