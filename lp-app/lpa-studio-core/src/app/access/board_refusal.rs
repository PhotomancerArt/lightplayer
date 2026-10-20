//! A board's refusal of an access write, in plain words.
//!
//! The board answers a refused `AccessAdd` with its own sentence, built by
//! `lpa-server`'s access store from `lpc_access::AccessFileError`'s
//! `Display` ("cannot add access: 17 secrets in one access file (at most
//! 16)"). That is a developer's line; the panel and the card say these
//! instead. The wire carries no code for it (a new one would be a wire
//! change), so the two prefixes the store writes are matched here and pinned
//! by tests against the real store and the real error type.
//!
//! | the board says | Studio says |
//! |---|---|
//! | the file is at its cap (`AccessFileError::TooManySecrets`) | [`FULL_SENTENCE`] |
//! | any other refused add (a zero salt or iteration count; Studio never sends one) | [`WONT_TAKE_SENTENCE`] |
//! | the store could not be written | [`CANT_SAVE_SENTENCE`] |
//! | anything else (an older firmware's own words) | as it said |

use lpc_access::MAX_SECRETS_PER_FILE;

use super::device_access_ops::FULL_SENTENCE;

/// What a board that refused a key for a reason Studio cannot act on says.
pub const WONT_TAKE_SENTENCE: &str = "The board wouldn't take that key.";
/// What a board that could not save its key list says.
pub const CANT_SAVE_SENTENCE: &str = "The board couldn't save its list of keys — try again.";

/// The board's refusal text (`ServerMsgBody::Error`) said for a person.
pub fn board_refusal_words(error: &str) -> String {
    if let Some(why) = error.strip_prefix("cannot add access: ") {
        // `TooManySecrets` reads "<n> secrets in one access file (at most 16)".
        let cap = format!(" secrets in one access file (at most {MAX_SECRETS_PER_FILE})");
        return if why.ends_with(&cap) {
            FULL_SENTENCE
        } else {
            WONT_TAKE_SENTENCE
        }
        .to_string();
    }
    if error.starts_with("cannot write the device's access list: ") {
        return CANT_SAVE_SENTENCE.to_string();
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::AccessFileError;

    #[test]
    fn the_boards_cap_refusal_is_the_full_sentence() {
        let said = format!(
            "cannot add access: {}",
            AccessFileError::TooManySecrets(MAX_SECRETS_PER_FILE + 1)
        );
        assert_eq!(board_refusal_words(&said), FULL_SENTENCE);
    }

    #[test]
    fn other_refused_adds_and_a_failed_write_get_their_own_words() {
        let zero_salt = format!(
            "cannot add access: {}",
            AccessFileError::ZeroSalt {
                label: "Mine".into()
            }
        );
        assert_eq!(board_refusal_words(&zero_salt), WONT_TAKE_SENTENCE);
        assert_eq!(
            board_refusal_words("cannot write the device's access list: no space"),
            CANT_SAVE_SENTENCE
        );
    }

    #[test]
    fn what_it_does_not_know_is_said_as_the_board_said_it() {
        assert_eq!(board_refusal_words("unknown request"), "unknown request");
    }
}
