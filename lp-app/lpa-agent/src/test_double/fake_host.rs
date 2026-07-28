//! [`FakeHost`]: the canonical scripted `AgentHost` test double.

use std::cell::RefCell;
use std::rc::Rc;

use lps_probe::LedPoint;

use crate::tool::iterate_host::{
    AgentHost, EngineVerdict, HostError, HostFuture, ParamDefRecord, ParamUpsert, ShaderContext,
};

/// One fake host for every agent suite: source staging with a failure knob,
/// scripted param records and engine verdicts, and full call recording
/// (staged sources, upserts, verdict budgets). Recording state is
/// `Rc`-shared so callers can keep a handle and read the final state even
/// after a session takes ownership of the host (the eval-harness pattern).
///
/// Defaults mirror the minimal stub: no params (`shader_params` → `None`,
/// no def diff in results), no engine (`await_engine_verdict` → `None`, no
/// `engine` section), no LEDs, empty [`ShaderContext`]. Tests flip only the
/// knobs they exercise.
pub struct FakeHost {
    /// Current source; `stage_source` replaces it.
    pub source: Rc<RefCell<String>>,
    /// Every source staged, in order.
    pub staged: Rc<RefCell<Vec<String>>>,
    /// When set, `stage_source` refuses like a failed overlay write.
    pub fail_stage: bool,
    /// Def-side param records (`None` = host without def knowledge).
    pub params: Option<Vec<ParamDefRecord>>,
    /// Every recorded param upsert, in order.
    pub upserts: Rc<RefCell<Vec<ParamUpsert>>>,
    /// When set, `upsert_param` refuses like a failed overlay write.
    pub fail_upsert: bool,
    /// Scripted engine verdict (`None` = host without an engine link).
    pub verdict: Option<EngineVerdict>,
    /// Budgets `await_engine_verdict` was called with.
    pub verdict_budgets: Rc<RefCell<Vec<u32>>>,
    /// The fixture's LED sample points.
    pub leds: Vec<LedPoint>,
    /// System-prompt context.
    pub context: ShaderContext,
}

impl FakeHost {
    /// A host serving `source` with every knob at its default.
    pub fn new(source: &str) -> Self {
        Self {
            source: Rc::new(RefCell::new(source.to_string())),
            staged: Rc::new(RefCell::new(Vec::new())),
            fail_stage: false,
            params: None,
            upserts: Rc::new(RefCell::new(Vec::new())),
            fail_upsert: false,
            verdict: None,
            verdict_budgets: Rc::new(RefCell::new(Vec::new())),
            leds: Vec::new(),
            context: ShaderContext::default(),
        }
    }
}

impl AgentHost for FakeHost {
    fn current_source(&self) -> Result<String, HostError> {
        Ok(self.source.borrow().clone())
    }

    fn stage_source<'a>(&'a mut self, source: &'a str) -> HostFuture<'a, Result<(), HostError>> {
        Box::pin(async move {
            if self.fail_stage {
                return Err(HostError::new("overlay write refused"));
            }
            self.staged.borrow_mut().push(source.to_string());
            *self.source.borrow_mut() = source.to_string();
            Ok(())
        })
    }

    fn await_engine_verdict(&mut self, budget_ms: u32) -> HostFuture<'_, Option<EngineVerdict>> {
        self.verdict_budgets.borrow_mut().push(budget_ms);
        let verdict = self.verdict.clone();
        Box::pin(async move { verdict })
    }

    fn shader_params(&mut self) -> HostFuture<'_, Option<Vec<ParamDefRecord>>> {
        let params = self.params.clone();
        Box::pin(async move { params })
    }

    fn upsert_param<'a>(
        &'a mut self,
        upsert: &'a ParamUpsert,
    ) -> HostFuture<'a, Result<(), HostError>> {
        Box::pin(async move {
            if self.fail_upsert {
                return Err(HostError::new("overlay write refused"));
            }
            self.upserts.borrow_mut().push(upsert.clone());
            Ok(())
        })
    }

    fn led_points(&self) -> Vec<LedPoint> {
        self.leds.clone()
    }

    fn shader_context(&self) -> ShaderContext {
        self.context.clone()
    }
}
