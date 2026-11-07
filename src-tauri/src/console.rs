use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsoleEvent {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: u64,
}

impl ConsoleEvent {
    pub fn from_line(line: &str) -> Self {
        let level = if line.contains("[WARN]") || line.contains("WARN") {
            LogLevel::Warn
        } else if line.contains("[ERROR]") || line.contains("ERROR") {
            LogLevel::Error
        } else {
            LogLevel::Info
        };

        let message = if let Some(index) = line.rfind(':') {
            line[index + 1..].trim().to_string()
        } else {
            line.trim().to_string()
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            level,
            message,
            timestamp,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FakeProcess {
    stdout: Vec<String>,
    stdin: Vec<String>,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stdin(&self) -> &[String] {
        &self.stdin
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    #[error("failed to write command: {0}")]
    Write(String),
}

pub struct LiveConsole<P = FakeProcess> {
    process: P,
    events: Vec<ConsoleEvent>,
}

impl LiveConsole<FakeProcess> {
    pub fn new(process: FakeProcess) -> Self {
        Self {
            process,
            events: Vec::new(),
        }
    }
}

impl LiveConsole<FakeProcess> {
    pub fn read_stdout(&mut self) -> Vec<ConsoleEvent> {
        let mut events = Vec::new();
        for line in self.process.stdout.drain(..) {
            let event = ConsoleEvent::from_line(&line);
            events.push(event.clone());
            self.events.push(event);
        }
        events
    }

    pub fn send_command(&mut self, command: &str) -> Result<(), ConsoleError> {
        if command.trim().is_empty() {
            return Err(ConsoleError::Write("command cannot be empty".to_string()));
        }

        self.process.stdin.push(command.to_string());
        Ok(())
    }

    pub fn process(&self) -> &FakeProcess {
        &self.process
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_written_by_subprocess_reaches_ui_event_stream() {
        let mut process = FakeProcess::new();
        process
            .stdout
            .push("[12:00:00] [INFO]: Server started".to_string());

        let mut console = LiveConsole::new(process);
        let events = console.read_stdout();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message, "Server started");
        assert_eq!(events[0].level, LogLevel::Info);
    }

    #[test]
    fn typed_command_is_written_to_server_stdin() {
        let process = FakeProcess::new();
        let mut console = LiveConsole::new(process);

        console.send_command("say hello").unwrap();

        assert_eq!(console.process().stdin(), vec!["say hello".to_string()]);
    }
}
