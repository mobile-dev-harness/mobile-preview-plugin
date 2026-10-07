use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Device, Error, Result};

pub const MAX_OWNER_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Connecting,
    TransportReady,
    Disconnected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Session {
    pub id: String,
    pub owner: String,
    pub device: Device,
    pub generation: u64,
    pub state: SessionState,
}

/// Coordinates local ownership; IDs and generations are not authentication tokens.
#[derive(Debug, Default)]
pub struct LeaseManager {
    sessions: BTreeMap<String, Session>,
    generation: u64,
}

impl LeaseManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserve before asynchronous probing so concurrent connects cannot claim the same device.
    pub fn reserve(&mut self, owner: &str, device: Device) -> Result<Session> {
        if owner.trim().is_empty()
            || owner.len() > MAX_OWNER_BYTES
            || owner.chars().any(char::is_control)
        {
            return Err(Error::InvalidArgument {
                message: format!(
                    "owner must contain 1 to {MAX_OWNER_BYTES} UTF-8 bytes without control characters"
                ),
            });
        }
        if device.id.trim().is_empty() || device.id.chars().any(char::is_control) {
            return Err(Error::InvalidArgument {
                message: "device ID must be nonempty and contain no control characters".to_owned(),
            });
        }
        if let Some(existing) = self.sessions.values().find(|session| {
            session.owner == owner
                || (session.device.platform == device.platform && session.device.id == device.id)
        }) {
            return Err(Error::Busy {
                device: existing.device.id.clone(),
                owner: existing.owner.clone(),
            });
        }

        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Error::InvalidArgument {
                message: "session generation exhausted; restart the host".to_owned(),
            })?;
        let session = Session {
            id: format!("session-{generation}"),
            owner: owner.to_owned(),
            device,
            generation,
            state: SessionState::Connecting,
        };
        self.generation = generation;
        self.sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub fn ready(&mut self, owner: &str, id: &str, generation: u64) -> Result<Session> {
        self.status(owner, id, generation)?;
        let session = self.sessions.get_mut(id).ok_or(Error::StaleSession)?;
        session.state = SessionState::TransportReady;
        Ok(session.clone())
    }

    pub fn status(&self, owner: &str, id: &str, generation: u64) -> Result<Session> {
        self.sessions
            .get(id)
            .filter(|session| session.owner == owner && session.generation == generation)
            .cloned()
            .ok_or(Error::StaleSession)
    }

    /// The disconnected result is a receipt; subsequent requests using this lease are stale.
    pub fn disconnect(&mut self, owner: &str, id: &str, generation: u64) -> Result<Session> {
        self.status(owner, id, generation)?;
        let mut session = self.sessions.remove(id).ok_or(Error::StaleSession)?;
        session.state = SessionState::Disconnected;
        Ok(session)
    }

    pub fn disconnect_all(&mut self) -> usize {
        let count = self.sessions.len();
        self.sessions.clear();
        count
    }
}
