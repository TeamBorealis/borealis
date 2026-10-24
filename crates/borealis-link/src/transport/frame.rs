//! Length-prefixed framing: `u32` big-endian length, then that many bytes. A zero-length frame
//! is valid. Oversized announcements are refused before any allocation.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::LinkError;

const HEADER: usize = 4;

/// Write one frame and flush.
///
/// # Errors
/// [`LinkError::FrameTooLarge`] above `max`, or I/O failure.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    w: &mut W,
    frame: &[u8],
    max: usize,
) -> Result<(), LinkError> {
    if frame.len() > max {
        return Err(LinkError::FrameTooLarge {
            size: frame.len(),
            max,
        });
    }
    let len = u32::try_from(frame.len()).map_err(|_| LinkError::FrameTooLarge {
        size: frame.len(),
        max,
    })?;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(frame).await?;
    w.flush().await?;
    Ok(())
}

/// Read one frame; `Ok(None)` on a clean end of stream before a header starts.
///
/// # Errors
/// [`LinkError::FrameTooLarge`] for an announced size above `max`, [`LinkError::Io`] for a
/// stream that ends inside a frame.
pub async fn read_frame<R: AsyncRead + Unpin>(
    r: &mut R,
    max: usize,
) -> Result<Option<Vec<u8>>, LinkError> {
    let mut header = [0u8; HEADER];
    let mut got = 0;
    while got < HEADER {
        let n = r.read(&mut header[got..]).await?;
        if n == 0 {
            return if got == 0 {
                Ok(None)
            } else {
                Err(LinkError::Io("stream ended inside a frame header".into()))
            };
        }
        got += n;
    }
    let size = usize::try_from(u32::from_be_bytes(header)).unwrap_or(usize::MAX);
    if size > max {
        return Err(LinkError::FrameTooLarge { size, max });
    }
    let mut body = vec![0u8; size];
    r.read_exact(&mut body)
        .await
        .map_err(|_| LinkError::Io("stream ended inside a frame".into()))?;
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_frames_including_empty() {
        let (mut a, mut b) = tokio::io::duplex(64);
        let writer = tokio::spawn(async move {
            write_frame(&mut a, b"hello", 16).await.unwrap();
            write_frame(&mut a, b"", 16).await.unwrap();
            write_frame(&mut a, &[7u8; 16], 16).await.unwrap();
        });
        assert_eq!(read_frame(&mut b, 16).await.unwrap().unwrap(), b"hello");
        assert_eq!(read_frame(&mut b, 16).await.unwrap().unwrap(), b"");
        assert_eq!(read_frame(&mut b, 16).await.unwrap().unwrap(), [7u8; 16]);
        writer.await.unwrap();
        assert!(read_frame(&mut b, 16).await.unwrap().is_none(), "clean EOF");
    }

    #[tokio::test]
    async fn oversized_frames_are_refused_on_both_sides() {
        let (mut a, mut b) = tokio::io::duplex(64);
        assert!(matches!(
            write_frame(&mut a, &[0u8; 17], 16).await,
            Err(LinkError::FrameTooLarge { size: 17, max: 16 })
        ));
        a.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
        assert!(matches!(
            read_frame(&mut b, 16).await,
            Err(LinkError::FrameTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn truncated_frames_are_errors() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&[0, 0, 0, 5, b'x']).await.unwrap();
        drop(a);
        assert!(matches!(
            read_frame(&mut b, 16).await,
            Err(LinkError::Io(_))
        ));
        let (mut c, mut d) = tokio::io::duplex(64);
        c.write_all(&[0, 0]).await.unwrap();
        drop(c);
        assert!(matches!(
            read_frame(&mut d, 16).await,
            Err(LinkError::Io(_))
        ));
    }
}
