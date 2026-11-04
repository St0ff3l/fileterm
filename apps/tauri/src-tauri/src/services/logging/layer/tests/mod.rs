use super::*;
use tracing_log::log::{Level, Log, Record};
use tracing_subscriber::prelude::*;

#[test]
fn bridged_russh_warning_uses_connection_category() {
    let directory = std::env::temp_dir().join(format!(
        "fileterm-log-bridge-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let subscriber = tracing_subscriber::registry().with(FileTermLogLayer::new(directory.clone()));
    tracing::subscriber::with_default(subscriber, || {
        tracing_log::LogTracer::new().log(
            &Record::builder()
                .level(Level::Warn)
                .target("russh::client::kex")
                .args(format_args!("DH prime size outside requested range"))
                .build(),
        );
    });
    writer::flush(std::time::Duration::from_secs(2));
    let log = std::fs::read_to_string(directory.join("app.log")).unwrap();
    assert!(log.contains("category=connection"), "{log}");
    assert!(log.contains("russh::client::kex"), "{log}");
    std::fs::remove_dir_all(directory).unwrap();
}
