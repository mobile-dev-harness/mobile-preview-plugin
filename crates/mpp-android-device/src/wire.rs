use std::{
    io::{self, Read},
    time::Instant,
};

pub enum Line {
    Ready(Vec<u8>),
    Pending,
    Eof,
}

/// Preserve partially received commands across socket read deadlines.
pub fn read_line(
    reader: &mut impl Read,
    pending: &mut Vec<u8>,
    limit: usize,
    deadline: Option<Instant>,
) -> io::Result<Line> {
    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "device message deadline exceeded",
            ));
        }
        let mut byte = [0];
        let read = reader.read(&mut byte);
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "device message deadline exceeded",
            ));
        }
        match read {
            Ok(0) if pending.is_empty() => return Ok(Line::Eof),
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated device message",
                ));
            }
            Ok(_) if byte[0] == b'\n' => return Ok(Line::Ready(std::mem::take(pending))),
            Ok(_) if pending.len() >= limit => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "device message exceeds limit",
                ));
            }
            Ok(_) => pending.push(byte[0]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(Line::Pending);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

#[derive(Default)]
pub struct ParameterSets {
    sps: Vec<u8>,
    pps: Vec<u8>,
}

impl ParameterSets {
    pub fn update(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut starts = Vec::new();
        let mut index = 0;
        while index + 3 <= bytes.len() {
            let prefix = if bytes[index..].starts_with(&[0, 0, 0, 1]) {
                4
            } else if bytes[index..].starts_with(&[0, 0, 1]) {
                3
            } else {
                index += 1;
                continue;
            };
            starts.push((index, index + prefix));
            index += prefix;
        }
        if starts.first().is_none_or(|(start, _)| *start != 0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "codec configuration is not Annex B",
            ));
        }
        for (position, (_, payload)) in starts.iter().enumerate() {
            let end = starts
                .get(position + 1)
                .map_or(bytes.len(), |(start, _)| *start);
            if *payload >= end {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty configuration NAL",
                ));
            }
            let target = match bytes[*payload] & 0x1f {
                7 => &mut self.sps,
                8 => &mut self.pps,
                _ => continue,
            };
            target.clear();
            target.extend_from_slice(&[0, 0, 0, 1]);
            target.extend_from_slice(&bytes[*payload..end]);
        }
        Ok(())
    }

    pub fn configuration(&self) -> Option<Vec<u8>> {
        if self.sps.is_empty() || self.pps.is_empty() {
            return None;
        }
        let mut bytes = self.sps.clone();
        bytes.extend_from_slice(&self.pps);
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_lines_do_not_consume_the_next_message() {
        let mut reader = io::Cursor::new(b"one\ntwo\n".to_vec());
        let mut pending = Vec::new();
        assert!(
            matches!(read_line(&mut reader, &mut pending, 3, None).unwrap(), Line::Ready(bytes) if bytes == b"one")
        );
        assert!(
            matches!(read_line(&mut reader, &mut pending, 3, None).unwrap(), Line::Ready(bytes) if bytes == b"two")
        );
        assert!(matches!(
            read_line(&mut reader, &mut pending, 3, None).unwrap(),
            Line::Eof
        ));
        assert!(read_line(&mut io::Cursor::new(b"four\n"), &mut pending, 3, None).is_err());
        assert!(
            read_line(
                &mut io::Cursor::new(b"x\n"),
                &mut Vec::new(),
                3,
                Some(Instant::now())
            )
            .is_err()
        );
    }

    #[test]
    fn configuration_collects_split_sets_without_losing_or_duplicating_them() {
        let mut sets = ParameterSets::default();
        sets.update(&[0, 0, 1, 0x67, 2]).unwrap();
        assert!(sets.configuration().is_none());
        sets.update(&[0, 0, 0, 1, 0x68, 3]).unwrap();
        let expected = vec![0, 0, 0, 1, 0x67, 2, 0, 0, 0, 1, 0x68, 3];
        assert_eq!(sets.configuration(), Some(expected.clone()));
        sets.update(&expected).unwrap();
        assert_eq!(sets.configuration(), Some(expected));
        assert!(sets.update(&[9, 0, 0, 1, 0x67]).is_err());
    }
}
