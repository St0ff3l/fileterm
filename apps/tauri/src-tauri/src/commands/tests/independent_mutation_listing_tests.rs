use super::*;
#[test]
fn modifying_an_independent_directory_or_navigating_during_refresh_keeps_the_visible_listing() {
    let old = vec![serde_json::json!({"name":"visible-file"})];
    let incoming = vec![serde_json::json!({"name":"transfer-file"})];
    let mut files = old.clone();
    apply_remote_mutation_listing("/terminal", "/transfer", &mut files, incoming.clone());
    assert_eq!(files, old);
    apply_remote_mutation_listing("/navigated", "/terminal", &mut files, incoming.clone());
    assert_eq!(files, old);
    apply_remote_mutation_listing("/terminal/", "/terminal", &mut files, incoming.clone());
    assert_eq!(files, incoming);
}
