use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    #[error("endpoint ID mismatch")]
    InvalidEndpointId,
    #[error("client is not paired")]
    UnpairedClient,
    #[error("invalid or expired pairing ticket")]
    InvalidPairingTicket,
    #[error("empty command")]
    EmptyCommand,
    #[error("unsupported command: {0}")]
    UnsupportedCommand(String),
}

/// Commands executable over the remote control channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteCommand {
    Start,
    Stop,
    Status,
    Console { command: String },
    PlayerList,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteClient {
    pub endpoint_id: String,
    pub paired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingTicket {
    pub endpoint_id: String,
    pub ticket_code: String,
    pub qr_payload: String,
}

#[derive(Debug, Clone)]
pub struct RemoteController {
    expected_endpoint_id: String,
    active_ticket: Option<String>,
    paired_clients: HashSet<String>,
}

impl RemoteController {
    pub fn new(expected_endpoint_id: &str) -> Self {
        Self {
            expected_endpoint_id: expected_endpoint_id.to_string(),
            active_ticket: None,
            paired_clients: HashSet::new(),
        }
    }

    pub fn endpoint_id(&self) -> &str {
        &self.expected_endpoint_id
    }

    /// Generates a new pairing ticket for the remote/mobile client to scan or enter.
    pub fn create_pairing_ticket(&mut self) -> PairingTicket {
        let ticket_code = format!(
            "{:06x}",
            (self
                .expected_endpoint_id
                .chars()
                .map(|c| c as u64)
                .sum::<u64>()
                * 31
                + self.paired_clients.len() as u64 * 17
                + 12345)
                % 0xFFFFFF
        );
        self.active_ticket = Some(ticket_code.clone());
        let qr_payload = format!(
            "redstone://pair/{}/{}",
            self.expected_endpoint_id, ticket_code
        );

        PairingTicket {
            endpoint_id: self.expected_endpoint_id.clone(),
            ticket_code,
            qr_payload,
        }
    }

    /// Pairs a remote client if the ticket code is valid.
    pub fn pair_client(
        &mut self,
        client_endpoint_id: &str,
        ticket_code: &str,
    ) -> Result<RemoteClient, RemoteError> {
        let trimmed_ticket = ticket_code.trim();
        if trimmed_ticket.is_empty() || self.active_ticket.as_deref() != Some(trimmed_ticket) {
            return Err(RemoteError::InvalidPairingTicket);
        }

        self.paired_clients.insert(client_endpoint_id.to_string());
        self.active_ticket = None;

        Ok(RemoteClient {
            endpoint_id: client_endpoint_id.to_string(),
            paired: true,
        })
    }

    /// Revokes pairing for a remote client.
    pub fn unpair_client(&mut self, client_endpoint_id: &str) -> bool {
        self.paired_clients.remove(client_endpoint_id)
    }

    /// Checks if a client is currently paired.
    pub fn is_paired(&self, client_endpoint_id: &str) -> bool {
        self.paired_clients.contains(client_endpoint_id)
    }

    /// Returns the list of all currently paired client endpoint IDs.
    pub fn list_paired_clients(&self) -> Vec<String> {
        let mut list: Vec<String> = self.paired_clients.iter().cloned().collect();
        list.sort();
        list
    }

    pub fn authorize(&self, endpoint_id: &str) -> Result<(), RemoteError> {
        if endpoint_id != self.expected_endpoint_id {
            return Err(RemoteError::InvalidEndpointId);
        }

        Ok(())
    }

    /// Parses a string command into a strongly-typed `RemoteCommand`.
    pub fn parse_command(raw: &str) -> Result<RemoteCommand, RemoteError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(RemoteError::EmptyCommand);
        }

        if trimmed.eq_ignore_ascii_case("start") {
            Ok(RemoteCommand::Start)
        } else if trimmed.eq_ignore_ascii_case("stop") {
            Ok(RemoteCommand::Stop)
        } else if trimmed.eq_ignore_ascii_case("status") {
            Ok(RemoteCommand::Status)
        } else if trimmed.eq_ignore_ascii_case("players")
            || trimmed.eq_ignore_ascii_case("playerlist")
        {
            Ok(RemoteCommand::PlayerList)
        } else if let Some(cmd) = trimmed.strip_prefix("console ") {
            let inner = cmd.trim();
            if inner.is_empty() {
                return Err(RemoteError::EmptyCommand);
            }
            Ok(RemoteCommand::Console {
                command: inner.to_string(),
            })
        } else if let Some(cmd) = trimmed.strip_prefix("cmd ") {
            let inner = cmd.trim();
            if inner.is_empty() {
                return Err(RemoteError::EmptyCommand);
            }
            Ok(RemoteCommand::Console {
                command: inner.to_string(),
            })
        } else {
            Err(RemoteError::UnsupportedCommand(trimmed.to_string()))
        }
    }

    pub fn execute(
        &self,
        client: &RemoteClient,
        command: &str,
    ) -> Result<RemoteCommand, RemoteError> {
        self.authorize(&client.endpoint_id)?;

        if !client.paired && !self.is_paired(&client.endpoint_id) {
            return Err(RemoteError::UnpairedClient);
        }

        Self::parse_command(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_endpoint_id_is_rejected() {
        let controller = RemoteController::new("expected-id");
        assert!(controller.authorize("wrong-id").is_err());
    }

    #[test]
    fn commands_from_unpaired_clients_are_rejected() {
        let controller = RemoteController::new("expected-id");
        let client = RemoteClient {
            endpoint_id: "expected-id".to_string(),
            paired: false,
        };

        assert!(controller.execute(&client, "start").is_err());
    }

    #[test]
    fn pairing_with_valid_ticket_authorizes_client() {
        let mut controller = RemoteController::new("expected-id");
        let ticket = controller.create_pairing_ticket();
        assert!(!ticket.ticket_code.is_empty());
        assert!(ticket
            .qr_payload
            .starts_with("redstone://pair/expected-id/"));

        let client = controller.pair_client("mobile-client-1", &ticket.ticket_code);
        assert!(client.is_ok());
        assert!(controller.is_paired("mobile-client-1"));
        assert_eq!(controller.list_paired_clients(), vec!["mobile-client-1"]);

        // Expired/consumed ticket fails on second attempt
        assert!(controller
            .pair_client("mobile-client-2", &ticket.ticket_code)
            .is_err());
    }

    #[test]
    fn pairing_with_invalid_ticket_fails() {
        let mut controller = RemoteController::new("expected-id");
        let _ticket = controller.create_pairing_ticket();

        assert_eq!(
            controller.pair_client("mobile-client", "wrong-code"),
            Err(RemoteError::InvalidPairingTicket)
        );
    }

    #[test]
    fn supported_commands_are_parsed_correctly() {
        let mut controller = RemoteController::new("expected-id");
        let ticket = controller.create_pairing_ticket();
        let _ = controller.pair_client("expected-id", &ticket.ticket_code);

        let client = RemoteClient {
            endpoint_id: "expected-id".to_string(),
            paired: true,
        };

        assert_eq!(
            controller.execute(&client, "start").unwrap(),
            RemoteCommand::Start
        );
        assert_eq!(
            controller.execute(&client, "STOP").unwrap(),
            RemoteCommand::Stop
        );
        assert_eq!(
            controller.execute(&client, "status").unwrap(),
            RemoteCommand::Status
        );
        assert_eq!(
            controller.execute(&client, "players").unwrap(),
            RemoteCommand::PlayerList
        );
        assert_eq!(
            controller.execute(&client, "console say hello!").unwrap(),
            RemoteCommand::Console {
                command: "say hello!".to_string()
            }
        );
        assert_eq!(
            controller
                .execute(&client, "cmd tp Steve 100 64 100")
                .unwrap(),
            RemoteCommand::Console {
                command: "tp Steve 100 64 100".to_string()
            }
        );
    }

    #[test]
    fn empty_and_unknown_commands_fail() {
        let client = RemoteClient {
            endpoint_id: "expected-id".to_string(),
            paired: true,
        };
        let controller = RemoteController::new("expected-id");

        assert_eq!(
            controller.execute(&client, "   "),
            Err(RemoteError::EmptyCommand)
        );
        assert_eq!(
            controller.execute(&client, "unknown_action"),
            Err(RemoteError::UnsupportedCommand(
                "unknown_action".to_string()
            ))
        );
    }

    #[test]
    fn unpairing_client_revokes_access() {
        let mut controller = RemoteController::new("expected-id");
        let ticket = controller.create_pairing_ticket();
        let _ = controller.pair_client("expected-id", &ticket.ticket_code);

        assert!(controller.is_paired("expected-id"));
        assert!(controller.unpair_client("expected-id"));
        assert!(!controller.is_paired("expected-id"));

        let client = RemoteClient {
            endpoint_id: "expected-id".to_string(),
            paired: false,
        };
        assert_eq!(
            controller.execute(&client, "status"),
            Err(RemoteError::UnpairedClient)
        );
    }
}
