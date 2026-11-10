use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
use encoding_rs::{Encoding, GBK, UTF_8};

use crate::{
    error::Error,
    protocol::{Packet, Status, StatusCode},
};

pub(super) struct FilenameEncoding {
    configured: &'static Encoding,
    detected_gbk: AtomicBool,
}

impl FilenameEncoding {
    pub(super) fn new(configured: &'static Encoding) -> Self {
        Self {
            configured,
            detected_gbk: AtomicBool::new(false),
        }
    }

    pub(super) fn current(&self) -> &'static Encoding {
        if self.detected_gbk.load(Ordering::Relaxed) {
            GBK
        } else {
            self.configured
        }
    }

    pub(super) fn decode_packet(&self, bytes: &mut Bytes) -> Result<Packet, Error> {
        let original = bytes.clone();
        let result = match Packet::from_bytes_with_encoding(bytes, self.current()) {
            Err(Error::BadMessage(message))
                if message == "invalid filename encoding" && self.current() == UTF_8 =>
            {
                Packet::from_bytes_with_encoding(&mut original.clone(), GBK).map(|packet| {
                    // Commit detection before callers immediately open a decoded name.
                    self.detected_gbk.store(true, Ordering::Relaxed);
                    log::info!(
                        "SFTP filename encoding detected as GBK from a non-UTF-8 NAME response"
                    );
                    packet
                })
            }
            result => result,
        };
        match result {
            Err(Error::BadMessage(message))
                if message == "invalid filename encoding" && original.len() >= 5 =>
            {
                // Deliver the failure to the waiting request instead of letting it time out.
                Ok(Packet::Status(Status {
                    id: u32::from_be_bytes(original[1..5].try_into().unwrap()),
                    status_code: StatusCode::BadMessage,
                    error_message: "SFTP filename cannot be decoded losslessly; check the server filename encoding".into(),
                    language_tag: String::new(),
                }))
            }
            result => result,
        }
    }
}
