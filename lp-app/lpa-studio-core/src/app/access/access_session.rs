//! Login on connect: one pure state machine per Bluetooth device.
//!
//! The board enforces tiers (M3/M4); Studio's job is to make logging in
//! effortless and refusals legible. So when a `ble:` link says hello, Studio
//! asks the link's own hello what it holds (`auth.required`, `auth.granted`)
//! and, when that is nothing, logs in:
//!
//! 1. the account default password, then the remembered ones (most recently
//!    successful first) — **at most [`AUTO_LOGIN_ATTEMPTS`] answers per
//!    connect**, because each wrong one feeds the board's backoff;
//! 2. then a prompt (the password sheet), whose password is tried on the
//!    link as it stands, or on the next one if the board dropped this one
//!    meanwhile (an unauthenticated link is dropped after 10 s, and Web
//!    Bluetooth reconnects it silently).
//!
//! Automatic tries are spent once per device, not once per connect: a board
//! that refused them will refuse them again, and burning its backoff on
//! every silent reconnect would lock out the password the user is about to
//! type. A user gesture (a typed password, "Log in") re-arms it.
//!
//! An **open** device grants play with no login; it is connected at play and
//! never prompted until an edit is refused (`NotPermitted { needs: Edit }`),
//! which opens the sheet with "This needs an edit password".
//!
//! Everything here is a pure function of what the controller tells it; the
//! conversations themselves run in `access_controller.rs`.

use lpa_devices::link::LinkId;
use lpa_devices::time::Millis;
use lpc_access::Tier;

/// Answers sent automatically per connect before prompting.
pub const AUTO_LOGIN_ATTEMPTS: usize = 2;

/// One connection window: the link, and when this window's hello was heard.
/// A reconnect is a new window, and the board treats it as a new link that
/// holds nothing until it logs in again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginWindow {
    pub link: LinkId,
    pub hello_at: Millis,
}

/// Where a device's login stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessPhase {
    /// Nothing known yet (no window, or not asked).
    Unknown,
    /// Asking the link's hello what it holds.
    Checking,
    /// A login conversation is running.
    LoggingIn,
    /// The link holds `tier`. `label` is the secret's name, `None` for an
    /// open device reached without logging in.
    Granted { tier: Tier, label: Option<String> },
    /// The link holds nothing, and no password Studio knows opened it.
    Locked,
    /// The board has no passwords at all and is not open: only USB reaches
    /// it (its owner turned Bluetooth on without a password).
    Unreachable,
}

/// Why the password sheet is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptReason {
    /// Nothing this browser knows was tried (no default, none remembered).
    NoPasswordKnown,
    /// The passwords tried were refused. `retry_after_ms` is the board's
    /// backoff at the time.
    Refused { retry_after_ms: u64 },
    /// An edit was refused on a play login.
    NeedsEdit,
    /// The user asked to log in with a different password.
    Asked,
}

/// A password the user typed, waiting for a link to try it on.
#[derive(Clone, PartialEq, Eq)]
pub struct TypedPassword {
    pub password: String,
    pub remember: bool,
}

impl core::fmt::Debug for TypedPassword {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypedPassword")
            .field("password", &"<redacted>")
            .field("remember", &self.remember)
            .finish()
    }
}

/// What the controller should do next for one device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessStep {
    /// Ask this window's hello what the link holds.
    Check(LoginWindow),
    /// Try these passwords, in order, on this window.
    Login {
        window: LoginWindow,
        passwords: Vec<String>,
        typed: Option<TypedPassword>,
    },
}

/// One Bluetooth device's login state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessSession {
    pub window: Option<LoginWindow>,
    /// The window the phase was learned on.
    phase_window: Option<LoginWindow>,
    pub phase: AccessPhase,
    /// Automatic tries already spent on this device (see the module doc).
    pub auto_spent: bool,
    pub typed: Option<TypedPassword>,
    pub prompt: Option<PromptReason>,
    /// The last thing a login said that the sheet should repeat.
    pub last_refusal: Option<PromptReason>,
    /// A conversation is running (a check or a login).
    pub busy: bool,
    /// The board's backoff runs until this (Studio's clock, ms).
    pub retry_at: Option<Millis>,
}

impl Default for AccessSession {
    fn default() -> Self {
        Self {
            window: None,
            phase_window: None,
            phase: AccessPhase::Unknown,
            auto_spent: false,
            typed: None,
            prompt: None,
            last_refusal: None,
            busy: false,
            retry_at: None,
        }
    }
}

impl AccessSession {
    /// The device's link and hello, as the model sees them now. `None` when
    /// the link is closed or has not said hello.
    pub fn observe(&mut self, window: Option<LoginWindow>) {
        if self.window == window {
            return;
        }
        self.window = window;
        if window.is_none() {
            // Gone: whatever the link held went with it.
            if !matches!(self.phase, AccessPhase::Locked | AccessPhase::Unreachable) {
                self.phase = AccessPhase::Unknown;
            }
            self.phase_window = None;
        }
    }

    /// The next conversation to run, if any. `default_password` and
    /// `remembered` are this browser's, in the order they are tried.
    pub fn next_step(
        &self,
        now: Millis,
        default_password: Option<&str>,
        remembered: &[&str],
    ) -> Option<AccessStep> {
        let window = self.window?;
        if self.busy || self.retry_at.is_some_and(|until| now < until) {
            return None;
        }
        // A typed password goes first, on whatever window is up.
        if let Some(typed) = &self.typed {
            // Unless this window already answered the question it asks.
            let satisfied = match (&self.phase, &self.prompt) {
                (
                    AccessPhase::Granted {
                        tier: Tier::Edit, ..
                    },
                    _,
                ) => true,
                (AccessPhase::Granted { .. }, Some(PromptReason::NeedsEdit)) => false,
                _ => false,
            };
            if !satisfied {
                return Some(AccessStep::Login {
                    window,
                    passwords: vec![typed.password.clone()],
                    typed: Some(typed.clone()),
                });
            }
        }
        if self.phase_window != Some(window) {
            return Some(AccessStep::Check(window));
        }
        match self.phase {
            AccessPhase::Locked if !self.auto_spent => {
                let passwords = auto_candidates(default_password, remembered);
                if passwords.is_empty() {
                    return None;
                }
                Some(AccessStep::Login {
                    window,
                    passwords,
                    typed: None,
                })
            }
            _ => None,
        }
    }

    /// A conversation for `window` started.
    pub fn started(&mut self, step: &AccessStep) {
        self.busy = true;
        if matches!(step, AccessStep::Login { .. }) {
            self.phase = AccessPhase::LoggingIn;
        } else if self.phase_window != self.window {
            self.phase = AccessPhase::Checking;
        }
    }

    /// The hello answered: what `window` holds, and whether it logs in at all.
    pub fn checked(
        &mut self,
        window: LoginWindow,
        required: bool,
        granted: Option<Tier>,
        anything_known: bool,
    ) {
        self.busy = false;
        if self.window != Some(window) {
            return;
        }
        self.phase_window = Some(window);
        self.phase = match (required, granted) {
            (false, _) => AccessPhase::Granted {
                tier: granted.unwrap_or(Tier::Edit),
                label: None,
            },
            (true, Some(tier)) => match &self.phase {
                // Keep the label a login on this window already earned.
                AccessPhase::Granted { tier: held, label } if *held == tier => {
                    AccessPhase::Granted {
                        tier,
                        label: label.clone(),
                    }
                }
                _ => AccessPhase::Granted { tier, label: None },
            },
            (true, None) => AccessPhase::Locked,
        };
        if matches!(self.phase, AccessPhase::Locked)
            && self.auto_spent
            && self.typed.is_none()
            && self.prompt.is_none()
        {
            // The board refused what we know before; say so again rather
            // than trying it again.
            self.prompt = Some(
                self.last_refusal
                    .clone()
                    .unwrap_or(PromptReason::NoPasswordKnown),
            );
        }
        if matches!(self.phase, AccessPhase::Locked) && !self.auto_spent && !anything_known {
            self.auto_spent = true;
            self.prompt = Some(PromptReason::NoPasswordKnown);
        }
    }

    /// A login conversation ended.
    pub fn logged_in(
        &mut self,
        window: LoginWindow,
        outcome: &super::login_attempt::LoginAttemptOutcome,
        was_typed: bool,
        now: Millis,
    ) {
        use super::login_attempt::LoginAttemptOutcome as Outcome;
        self.busy = false;
        if !was_typed {
            self.auto_spent = true;
        }
        let same_window = self.window == Some(window);
        match outcome {
            Outcome::Granted { tier, label, .. } => {
                if was_typed {
                    self.typed = None;
                }
                if same_window {
                    self.phase_window = Some(window);
                    self.phase = AccessPhase::Granted {
                        tier: *tier,
                        label: Some(label.clone()),
                    };
                }
                // A play grant does not answer "this needs edit".
                let answered =
                    *tier == Tier::Edit || !matches!(self.prompt, Some(PromptReason::NeedsEdit));
                if answered {
                    self.prompt = None;
                    self.last_refusal = None;
                } else {
                    self.prompt = Some(PromptReason::Refused { retry_after_ms: 0 });
                }
                self.retry_at = None;
            }
            Outcome::Refused { retry_after_ms } | Outcome::Busy { retry_after_ms } => {
                if was_typed {
                    self.typed = None;
                }
                let reason = PromptReason::Refused {
                    retry_after_ms: *retry_after_ms,
                };
                self.last_refusal = Some(reason.clone());
                self.prompt = Some(reason);
                self.retry_at = (*retry_after_ms > 0).then(|| Millis(now.0 + retry_after_ms));
                if same_window && !matches!(self.phase, AccessPhase::Granted { .. }) {
                    self.phase_window = Some(window);
                    self.phase = AccessPhase::Locked;
                }
                if same_window && matches!(self.phase, AccessPhase::LoggingIn) {
                    self.phase = AccessPhase::Locked;
                }
            }
            Outcome::NoPasswords => {
                if was_typed {
                    self.typed = None;
                }
                if same_window {
                    self.phase_window = Some(window);
                    self.phase = AccessPhase::Unreachable;
                }
                self.prompt = None;
            }
            Outcome::Failed(_) => {
                // The link failed under the conversation: keep a typed
                // password for the next window, and let the next window
                // check again.
                if same_window {
                    self.phase_window = None;
                    self.phase = AccessPhase::Unknown;
                }
            }
        }
    }

    /// The user typed a password in the sheet.
    pub fn type_password(&mut self, typed: TypedPassword) {
        self.typed = Some(typed);
        self.retry_at = None;
    }

    /// An edit was refused on this device's link.
    pub fn needs_edit(&mut self) {
        self.prompt = Some(PromptReason::NeedsEdit);
    }

    /// The user asked to log in (again).
    pub fn ask(&mut self) {
        if self.prompt.is_none() {
            self.prompt = Some(PromptReason::Asked);
        }
    }

    /// The user closed the sheet without logging in.
    pub fn dismiss(&mut self) {
        self.prompt = None;
        self.typed = None;
    }
}

/// The automatic tries for a connect: the account default, then the
/// remembered passwords most-recent first, without repeats, capped.
pub fn auto_candidates(default_password: Option<&str>, remembered: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for password in default_password
        .into_iter()
        .chain(remembered.iter().copied())
    {
        if password.is_empty() || out.iter().any(|known| known == password) {
            continue;
        }
        out.push(password.to_string());
        if out.len() == AUTO_LOGIN_ATTEMPTS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::login_attempt::LoginAttemptOutcome;
    use super::*;

    fn window(link: u64, at: u64) -> LoginWindow {
        LoginWindow {
            link: LinkId(link),
            hello_at: Millis(at),
        }
    }

    #[test]
    fn the_default_goes_first_then_remembered_without_repeats_capped_at_two() {
        assert_eq!(
            auto_candidates(Some("dflt"), &["dflt", "camp", "mine"]),
            ["dflt", "camp"]
        );
        assert_eq!(auto_candidates(None, &["camp"]), ["camp"]);
        assert!(auto_candidates(Some(""), &[]).is_empty());
    }

    #[test]
    fn a_locked_link_is_checked_then_logged_in_with_what_the_browser_knows() {
        let mut session = AccessSession::default();
        let w = window(1, 10);
        session.observe(Some(w));
        let step = session
            .next_step(Millis(10), Some("dflt"), &["camp"])
            .unwrap();
        assert_eq!(step, AccessStep::Check(w));
        session.started(&step);
        assert_eq!(session.phase, AccessPhase::Checking);
        session.checked(w, true, None, true);
        assert_eq!(session.phase, AccessPhase::Locked);
        let step = session
            .next_step(Millis(20), Some("dflt"), &["camp"])
            .unwrap();
        assert_eq!(
            step,
            AccessStep::Login {
                window: w,
                passwords: vec!["dflt".to_string(), "camp".to_string()],
                typed: None
            }
        );
        session.started(&step);
        assert_eq!(session.phase, AccessPhase::LoggingIn);
        session.logged_in(
            w,
            &LoginAttemptOutcome::Granted {
                tier: Tier::Play,
                label: "camp".to_string(),
                password_index: 1,
            },
            false,
            Millis(30),
        );
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("camp".to_string())
            }
        );
        assert_eq!(session.prompt, None);
        assert_eq!(session.next_step(Millis(40), Some("dflt"), &[]), None);
    }

    #[test]
    fn refused_automatic_tries_prompt_and_are_not_repeated_on_reconnect() {
        let mut session = AccessSession::default();
        let first = window(1, 10);
        session.observe(Some(first));
        session.started(&AccessStep::Check(first));
        session.checked(first, true, None, true);
        let step = session.next_step(Millis(11), Some("dflt"), &[]).unwrap();
        session.started(&step);
        session.logged_in(
            first,
            &LoginAttemptOutcome::Refused {
                retry_after_ms: 2_000,
            },
            false,
            Millis(20),
        );
        assert_eq!(
            session.prompt,
            Some(PromptReason::Refused {
                retry_after_ms: 2_000
            })
        );
        assert_eq!(session.phase, AccessPhase::Locked);

        // The board drops the unauthenticated link and it reconnects: the
        // new window is checked, but the refused passwords are not re-sent.
        session.observe(None);
        let second = window(2, 12_000);
        session.observe(Some(second));
        let step = session
            .next_step(Millis(12_000), Some("dflt"), &[])
            .unwrap();
        assert_eq!(step, AccessStep::Check(second));
        session.started(&step);
        session.checked(second, true, None, true);
        assert_eq!(session.next_step(Millis(12_001), Some("dflt"), &[]), None);
        assert!(session.prompt.is_some(), "the sheet stays up");
    }

    #[test]
    fn a_typed_password_waits_out_the_backoff_and_is_tried_on_the_next_window() {
        let mut session = AccessSession::default();
        let first = window(1, 10);
        session.observe(Some(first));
        session.started(&AccessStep::Check(first));
        session.checked(first, true, None, false);
        assert_eq!(session.prompt, Some(PromptReason::NoPasswordKnown));
        assert!(session.auto_spent, "nothing to try automatically");

        session.observe(None);
        session.type_password(TypedPassword {
            password: "s3cret".to_string(),
            remember: true,
        });
        assert_eq!(session.next_step(Millis(50), None, &[]), None, "no link up");
        let second = window(2, 900);
        session.observe(Some(second));
        let step = session.next_step(Millis(900), None, &[]).unwrap();
        assert!(
            matches!(&step, AccessStep::Login { passwords, typed: Some(_), .. } if passwords == &["s3cret".to_string()])
        );
        session.started(&step);
        session.logged_in(
            second,
            &LoginAttemptOutcome::Refused {
                retry_after_ms: 4_000,
            },
            true,
            Millis(1_000),
        );
        assert!(session.typed.is_none(), "a refused password is not retried");
        session.type_password(TypedPassword {
            password: "right".to_string(),
            remember: false,
        });
        assert!(
            session.next_step(Millis(1_500), None, &[]).is_some(),
            "a new password clears the wait: the user saw the backoff"
        );
    }

    #[test]
    fn an_open_device_is_play_with_no_prompt_until_an_edit_is_refused() {
        let mut session = AccessSession::default();
        let w = window(1, 10);
        session.observe(Some(w));
        session.started(&AccessStep::Check(w));
        session.checked(w, true, Some(Tier::Play), false);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: None
            }
        );
        assert_eq!(session.prompt, None);
        assert_eq!(session.next_step(Millis(20), Some("dflt"), &[]), None);

        session.needs_edit();
        assert_eq!(session.prompt, Some(PromptReason::NeedsEdit));
        session.type_password(TypedPassword {
            password: "edit-pw".to_string(),
            remember: true,
        });
        let step = session.next_step(Millis(30), None, &[]).unwrap();
        session.started(&step);
        session.logged_in(
            w,
            &LoginAttemptOutcome::Granted {
                tier: Tier::Edit,
                label: "mine".to_string(),
                password_index: 0,
            },
            true,
            Millis(40),
        );
        assert_eq!(session.prompt, None);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Edit,
                label: Some("mine".to_string())
            }
        );
    }

    #[test]
    fn a_trusted_link_needs_nothing() {
        let mut session = AccessSession::default();
        let w = window(1, 10);
        session.observe(Some(w));
        session.started(&AccessStep::Check(w));
        session.checked(w, false, Some(Tier::Edit), true);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Edit,
                label: None
            }
        );
        assert_eq!(session.next_step(Millis(20), Some("x"), &["y"]), None);
    }
}
