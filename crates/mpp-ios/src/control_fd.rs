//! Convert the inherited full-duplex stdin socket without taking ownership of process stdin.

use std::{io::Stdin, os::fd::AsFd, os::unix::net::UnixStream};

use mpp_core::Result;

use super::invalid;

pub(super) fn inherited(fd: u32) -> Result<tokio::net::UnixStream> {
    if fd != 0 {
        return Err(invalid(
            "The capture control socket must use inherited FD 0",
        ));
    }
    from_stdin(&std::io::stdin())
}

fn from_stdin(stdin: &Stdin) -> Result<tokio::net::UnixStream> {
    socket(stdin.as_fd())
}

fn socket(fd: std::os::fd::BorrowedFd<'_>) -> Result<tokio::net::UnixStream> {
    // AsFd and OwnedFd keep duplication safe, including an invalid or closed inherited descriptor.
    let stream = UnixStream::from(fd.try_clone_to_owned()?);
    stream.peer_addr()?;
    stream.set_nonblocking(true)?;
    Ok(tokio::net::UnixStream::from_std(stream)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn duplicates_connected_socket_without_closing_original() {
        let (original, peer) = UnixStream::pair().unwrap();
        peer.set_nonblocking(true).unwrap();
        let mut peer = tokio::net::UnixStream::from_std(peer).unwrap();
        let mut converted = socket(original.as_fd()).unwrap();
        converted.write_all(b"ping").await.unwrap();
        let mut bytes = [0; 4];
        peer.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"ping");
        peer.write_all(b"pong").await.unwrap();
        converted.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"pong");
        drop(converted);
        original.peer_addr().unwrap();
    }

    #[tokio::test]
    async fn refuses_non_socket_and_other_descriptors() {
        let file = std::fs::File::open("/dev/null").unwrap();
        assert!(socket(file.as_fd()).is_err());
        assert!(inherited(1).is_err());
        assert!(inherited(u32::MAX).is_err());
    }
}
