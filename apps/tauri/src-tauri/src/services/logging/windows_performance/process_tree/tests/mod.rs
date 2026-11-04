use super::*;
#[test]
fn process_selection_excludes_other_apps_and_shell_descendants() {
    let items = [
        Process {
            pid: 2,
            parent: 1,
            webview: true,
        },
        Process {
            pid: 3,
            parent: 2,
            webview: true,
        },
        Process {
            pid: 4,
            parent: 99,
            webview: true,
        },
        Process {
            pid: 5,
            parent: 1,
            webview: false,
        },
        Process {
            pid: 6,
            parent: 5,
            webview: true,
        },
    ];
    assert_eq!(webview_tree(1, &items, &[]), BTreeSet::from([1, 2, 3]));
}

#[test]
fn registered_browser_outside_parent_tree_includes_only_its_webview_descendants() {
    let items = [
        Process {
            pid: 2,
            parent: 99,
            webview: true,
        },
        Process {
            pid: 3,
            parent: 2,
            webview: true,
        },
        Process {
            pid: 4,
            parent: 98,
            webview: true,
        },
        Process {
            pid: 5,
            parent: 2,
            webview: false,
        },
        Process {
            pid: 6,
            parent: 5,
            webview: true,
        },
    ];
    assert_eq!(webview_tree(1, &items, &[2, 2]), BTreeSet::from([1, 2, 3]));
    let reversed: Vec<_> = items
        .iter()
        .rev()
        .map(|item| Process {
            pid: item.pid,
            parent: item.parent,
            webview: item.webview,
        })
        .collect();
    assert_eq!(webview_tree(1, &reversed, &[2]), BTreeSet::from([1, 2, 3]));
    assert_eq!(webview_tree(1, &items, &[5, 999]), BTreeSet::from([1]));
}
