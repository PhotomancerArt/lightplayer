//! The real-provider spend gate (Yona's policy — implemented exactly).
//!
//! Real-provider mode requires BOTH a provider environment configuration
//! (the evals convention: `ANTHROPIC_API_KEY`, or `LPA_EVAL_BASE_URL` +
//! `LPA_EVAL_MODEL`) AND `LPA_SPEND_OK=1`. Missing either, the runner
//! prints the enable instructions and exits cleanly WITHOUT constructing
//! any transport — the decision here is pure text over an env lookup;
//! `ReqwestTransport` is only reachable downstream of
//! [`ModeDecision::Run`] with [`RunMode::Real`].
//!
//! Nothing automated may set `LPA_SPEND_OK`: not justfile recipes (the
//! `agent-run` recipe passes the environment through, never sets it), not
//! tests, not CI.

use std::path::PathBuf;

use crate::eval_tasks::ProviderCfg;

/// The policy line printed by `--help` and every refusal.
pub const SPEND_POLICY_LINE: &str = "Policy: agents (Claude sessions) set LPA_SPEND_OK only with \
     Yona's explicit permission, granted per session. Nothing automated \
     (recipes, tests, CI) ever sets it.";

/// How to run: scripted (token-free) or against a real provider.
pub enum RunMode {
    /// Play back a script file through the P1 `ScriptedProvider` — no
    /// network, no spend, no gate.
    Scripted(PathBuf),
    /// A live provider resolved from the environment. Only constructed
    /// when the spend gate passed.
    Real(ProviderCfg),
}

/// The gate's outcome: run, or refuse with printable instructions.
pub enum ModeDecision {
    Run(RunMode),
    /// Print `message` and exit cleanly (status 0, no network).
    Refuse {
        message: String,
    },
}

/// Decide the run mode. `--scripted` short-circuits the gate (token-free);
/// otherwise BOTH provider env and `LPA_SPEND_OK=1` are required.
pub fn decide_mode(
    scripted: Option<PathBuf>,
    env: &dyn Fn(&str) -> Option<String>,
) -> ModeDecision {
    if let Some(script) = scripted {
        return ModeDecision::Run(RunMode::Scripted(script));
    }

    let provider = ProviderCfg::from_env_with(env);
    let spend_ok = env("LPA_SPEND_OK").is_some_and(|value| value == "1");
    match (provider, spend_ok) {
        (Some(cfg), true) => ModeDecision::Run(RunMode::Real(cfg)),
        (Some(_), false) => ModeDecision::Refuse {
            message: format!(
                "agent-run: provider configured, but LPA_SPEND_OK=1 is not set — refusing to \
                 spend tokens.\n\nTo enable a real-provider run:\n  LPA_SPEND_OK=1 just \
                 agent-run prompt=\"...\"\n\n{SPEND_POLICY_LINE}\n\nToken-free alternative: \
                 --scripted <script.json> needs no key and no gate."
            ),
        },
        (None, _) => ModeDecision::Refuse {
            message: format!(
                "agent-run: no provider configured — nothing to run against.\n\nConfigure one \
                 (the evals convention):\n  ANTHROPIC_API_KEY=...            # Anthropic\n  \
                 LPA_EVAL_BASE_URL=... LPA_EVAL_MODEL=...   # any OpenAI-compatible server\n\
                 then opt into spending:\n  LPA_SPEND_OK=1\n\n{SPEND_POLICY_LINE}\n\n\
                 Token-free alternative: --scripted <script.json> needs no key and no gate."
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn no_provider_env_refuses_with_enable_instructions() {
        let ModeDecision::Refuse { message } = decide_mode(None, &env_of(&[])) else {
            panic!("no env must refuse");
        };
        assert!(message.contains("ANTHROPIC_API_KEY"), "{message}");
        assert!(message.contains("LPA_SPEND_OK=1"), "{message}");
        assert!(message.contains(SPEND_POLICY_LINE), "{message}");
    }

    #[test]
    fn key_without_spend_ok_refuses_with_policy() {
        let env = env_of(&[("ANTHROPIC_API_KEY", "sk-test")]);
        let ModeDecision::Refuse { message } = decide_mode(None, &env) else {
            panic!("key without LPA_SPEND_OK must refuse");
        };
        assert!(message.contains("LPA_SPEND_OK=1"), "{message}");
        assert!(message.contains("Yona's explicit permission"), "{message}");
    }

    #[test]
    fn spend_ok_must_be_exactly_1() {
        let env = env_of(&[("ANTHROPIC_API_KEY", "sk-test"), ("LPA_SPEND_OK", "0")]);
        assert!(matches!(
            decide_mode(None, &env),
            ModeDecision::Refuse { .. }
        ));
        let env = env_of(&[("ANTHROPIC_API_KEY", "sk-test"), ("LPA_SPEND_OK", "yes")]);
        assert!(matches!(
            decide_mode(None, &env),
            ModeDecision::Refuse { .. }
        ));
    }

    #[test]
    fn key_plus_spend_ok_runs_real() {
        let env = env_of(&[("ANTHROPIC_API_KEY", "sk-test"), ("LPA_SPEND_OK", "1")]);
        assert!(matches!(
            decide_mode(None, &env),
            ModeDecision::Run(RunMode::Real(ProviderCfg::Anthropic { .. }))
        ));
    }

    #[test]
    fn scripted_mode_needs_no_env_and_no_gate() {
        let decision = decide_mode(Some(PathBuf::from("script.json")), &env_of(&[]));
        assert!(matches!(
            decision,
            ModeDecision::Run(RunMode::Scripted(path)) if path.ends_with("script.json")
        ));
    }
}
