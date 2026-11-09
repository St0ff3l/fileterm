# FileTerm patches to suppaftp 10.0.2

Based on the crates.io suppaftp 10.0.2 package from <https://github.com/veeso/suppaftp>, licensed MIT OR Apache-2.0.

The Tokio client adds an explicit text encoding setter. UTF-8 remains the default. Commands are encoded before transmission, while control replies and directory listings are decoded into UTF-8 for existing consumers. The encoding is preserved across explicit TLS upgrades and does not change file payloads. Unrepresentable commands and invalid legacy replies fail before loss of filename identity.

FileTerm uses its existing encoding_rs dependency and offers UTF-8 or GBK on FTP profiles. Sync and async-std clients are outside this patch.
