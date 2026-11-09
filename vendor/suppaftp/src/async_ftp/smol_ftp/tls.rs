//! # Tls
//!
//! Tls wrappers

use std::fmt::Debug;

use smol::io::{AsyncRead as Read, AsyncWrite as Write};
use smol::net::TcpStream;

#[cfg(feature = "smol-async-native-tls")]
mod native_tls;
#[cfg(feature = "smol-async-native-tls")]
pub use self::native_tls::{AsyncNativeTlsConnector, AsyncNativeTlsStream};

#[cfg(any(feature = "smol-rustls-aws-lc-rs", feature = "smol-rustls-ring"))]
mod rustls;
#[cfg(any(feature = "smol-rustls-aws-lc-rs", feature = "smol-rustls-ring"))]
pub use self::rustls::{AsyncRustlsConnector, AsyncRustlsStream};

#[cfg(feature = "async-secure")]
#[async_trait::async_trait]
pub trait AsyncTlsConnector: Debug {
    type Stream: SmolTlsStream;

    async fn connect(&self, domain: &str, stream: TcpStream) -> crate::FtpResult<Self::Stream>;
}

/// A trait for a smol based TLS stream.
///
/// This kind of stream is returned when using a data connection in FTP.
pub trait SmolTlsStream: Debug + Read + Write + Unpin {
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

/// A placeholder TLS stream used for plain (non-secure) async FTP connections.
///
/// It is only ever used as the `T` type parameter of a plain async FTP stream; a plain FTP data
/// connection is always a TCP stream, so the I/O and accessor methods below are never reached. The
/// I/O methods return an [`std::io::ErrorKind::Unsupported`] error instead of panicking, while the
/// infallible accessors panic, as reaching them indicates a logic error in the library.
#[derive(Debug)]
pub struct AsyncNoTlsStream;

/// Error returned by the I/O methods of [`AsyncNoTlsStream`].
fn no_tls_stream_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "AsyncNoTlsStream is a placeholder for plain FTP and cannot perform TLS I/O",
    )
}

impl Read for AsyncNoTlsStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(no_tls_stream_error()))
    }
}

impl Write for AsyncNoTlsStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(no_tls_stream_error()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(no_tls_stream_error()))
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(no_tls_stream_error()))
    }
}

impl SmolTlsStream for AsyncNoTlsStream {
    type InnerStream = TcpStream;

    fn tcp_stream(self) -> crate::FtpResult<TcpStream> {
        unreachable!(
            "AsyncNoTlsStream is a placeholder for plain FTP and has no underlying TcpStream"
        )
    }

    fn get_ref(&self) -> &TcpStream {
        unreachable!(
            "AsyncNoTlsStream is a placeholder for plain FTP and has no underlying TcpStream"
        )
    }

    fn mut_ref(&mut self) -> &mut Self::InnerStream {
        unreachable!(
            "AsyncNoTlsStream is a placeholder for plain FTP and has no underlying TcpStream"
        )
    }
}
