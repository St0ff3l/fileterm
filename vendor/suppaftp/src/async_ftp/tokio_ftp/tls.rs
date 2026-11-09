//! # Tls
//!
//! Tls wrappers

use std::fmt::Debug;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

#[cfg(feature = "tokio-async-native-tls")]
mod native_tls;
#[cfg(feature = "tokio-async-native-tls")]
pub use self::native_tls::{AsyncNativeTlsConnector, AsyncNativeTlsStream};

#[cfg(any(feature = "tokio-rustls-aws-lc-rs", feature = "tokio-rustls-ring"))]
mod rustls;
#[cfg(any(feature = "tokio-rustls-aws-lc-rs", feature = "tokio-rustls-ring"))]
pub use self::rustls::{AsyncRustlsConnector, AsyncRustlsStream};

#[cfg(feature = "async-secure")]
#[async_trait::async_trait]
pub trait AsyncTlsConnector: Debug {
    type Stream: TokioTlsStream;

    async fn connect(&self, domain: &str, stream: TcpStream) -> crate::FtpResult<Self::Stream>;
}

/// A trait for a Tokio based TLS stream.
///
/// This kind of stream is returned when using a data connection in FTP.
pub trait TokioTlsStream: Debug + AsyncRead + AsyncWrite + Unpin {
    type InnerStream: AsyncRead + AsyncWrite;

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

impl AsyncRead for AsyncNoTlsStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(no_tls_stream_error()))
    }
}

impl AsyncWrite for AsyncNoTlsStream {
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

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(no_tls_stream_error()))
    }
}

impl TokioTlsStream for AsyncNoTlsStream {
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
