//! # Tls
//!
//! Tls wrappers

use std::fmt::Debug;
use std::io::{Read, Write};
use std::net::TcpStream;

#[cfg(feature = "native-tls")]
mod native_tls;
#[cfg(feature = "native-tls")]
pub use self::native_tls::{NativeTlsConnector, NativeTlsStream};

#[cfg(any(feature = "rustls-aws-lc-rs", feature = "rustls-ring"))]
mod rustls;
#[cfg(any(feature = "rustls-aws-lc-rs", feature = "rustls-ring"))]
pub use self::rustls::{RustlsConnector, RustlsStream};

#[cfg(feature = "secure")]
pub trait TlsConnector: Debug {
    type Stream: TlsStream;

    fn connect(&self, domain: &str, stream: TcpStream) -> crate::FtpResult<Self::Stream>;
}

/// A trait for a TLS stream.
///
/// This kind of stream is returned when using a data connection in FTP.
pub trait TlsStream: Debug {
    type InnerStream: Read + Write;

    /// Get underlying tcp stream.
    ///
    /// Returns an error if the underlying socket cannot be turned into an owned [`TcpStream`].
    fn tcp_stream(self) -> crate::FtpResult<TcpStream>;

    /// Get ref to underlying tcp stream
    fn get_ref(&self) -> &TcpStream;

    /// Get mutable reference to tls stream
    fn mut_ref(&mut self) -> &mut Self::InnerStream;
}

/// A placeholder TLS stream used for plain (non-secure) FTP connections.
///
/// It is only ever used as the `T` type parameter of a plain [`crate::FtpStream`]; the data
/// connection of a plain FTP session is always a TCP stream, so the TLS methods below are never
/// reached. Calling any of them indicates a logic error in the library and therefore panics.
#[derive(Debug)]
pub struct NoTlsStream;

impl TlsStream for NoTlsStream {
    type InnerStream = TcpStream;

    fn tcp_stream(self) -> crate::FtpResult<TcpStream> {
        unreachable!("NoTlsStream is a placeholder for plain FTP and has no underlying TcpStream")
    }

    fn get_ref(&self) -> &TcpStream {
        unreachable!("NoTlsStream is a placeholder for plain FTP and has no underlying TcpStream")
    }

    fn mut_ref(&mut self) -> &mut Self::InnerStream {
        unreachable!("NoTlsStream is a placeholder for plain FTP and has no underlying TcpStream")
    }
}
