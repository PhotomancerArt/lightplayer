#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCommand {
    Next,
    Previous,
    Yes,
    Quit,
}

impl PromptCommand {
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() || input.eq_ignore_ascii_case("n") {
            return Ok(Self::Next);
        }
        if input.eq_ignore_ascii_case("p") {
            return Ok(Self::Previous);
        }
        if input.eq_ignore_ascii_case("y") {
            return Ok(Self::Yes);
        }
        if input.eq_ignore_ascii_case("q") {
            return Ok(Self::Quit);
        }
        Err("expected Enter, q, p, y, or n".into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationEvent {
    Ready,
    Open { gpio: u32 },
    Pulse { gpio: u32 },
    Stop { gpio: Option<u32> },
    Pong,
    Error { message: String },
    Other,
}

impl CalibrationEvent {
    pub fn parse(line: &str) -> Self {
        let line = line.trim();
        if line.starts_with("CAL READY") {
            return Self::Ready;
        }
        if line == "CAL PONG" {
            return Self::Pong;
        }
        if let Some(gpio) = parse_gpio_field(line.strip_prefix("CAL OPEN ")) {
            return Self::Open { gpio };
        }
        if let Some(gpio) = parse_gpio_field(line.strip_prefix("CAL PULSE ")) {
            return Self::Pulse { gpio };
        }
        if let Some(rest) = line.strip_prefix("CAL STOP") {
            return Self::Stop {
                gpio: parse_gpio_field(Some(rest.trim())),
            };
        }
        if let Some(message) = line.strip_prefix("CAL ERR ") {
            return Self::Error {
                message: message.into(),
            };
        }
        Self::Other
    }
}

fn parse_gpio_field(text: Option<&str>) -> Option<u32> {
    let text = text?;
    text.split_ascii_whitespace()
        .find_map(|field| field.strip_prefix("gpio="))
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prompt_commands() {
        assert_eq!(PromptCommand::parse("").unwrap(), PromptCommand::Next);
        assert_eq!(PromptCommand::parse("n").unwrap(), PromptCommand::Next);
        assert_eq!(PromptCommand::parse("p").unwrap(), PromptCommand::Previous);
        assert_eq!(PromptCommand::parse("y").unwrap(), PromptCommand::Yes);
        assert_eq!(PromptCommand::parse("q").unwrap(), PromptCommand::Quit);
        assert!(PromptCommand::parse("next").is_err());
    }

    #[test]
    fn parses_calibration_events() {
        assert_eq!(
            CalibrationEvent::parse("CAL READY target=esp32c6"),
            CalibrationEvent::Ready
        );
        assert_eq!(
            CalibrationEvent::parse("CAL OPEN gpio=18"),
            CalibrationEvent::Open { gpio: 18 }
        );
        assert_eq!(
            CalibrationEvent::parse("CAL PULSE gpio=4 duty=70"),
            CalibrationEvent::Pulse { gpio: 4 }
        );
        assert_eq!(
            CalibrationEvent::parse("CAL STOP gpio=4"),
            CalibrationEvent::Stop { gpio: Some(4) }
        );
        assert_eq!(CalibrationEvent::parse("CAL PONG"), CalibrationEvent::Pong);
    }
}
