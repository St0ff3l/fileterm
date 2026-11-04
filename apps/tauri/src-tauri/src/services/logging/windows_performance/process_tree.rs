use std::collections::BTreeSet;

pub(super) struct Process {
    pub(super) pid: u32,
    pub(super) parent: u32,
    pub(super) webview: bool,
}

pub(super) fn webview_tree(root: u32, items: &[Process], browsers: &[u32]) -> BTreeSet<u32> {
    let mut selected = BTreeSet::from([root]);
    selected.extend(
        items
            .iter()
            .filter(|item| item.webview && browsers.contains(&item.pid))
            .map(|item| item.pid),
    );
    loop {
        let before = selected.len();
        for item in items {
            if item.webview && selected.contains(&item.parent) {
                selected.insert(item.pid);
            }
        }
        if before == selected.len() {
            return selected;
        }
    }
}

#[cfg(test)]
mod tests;
