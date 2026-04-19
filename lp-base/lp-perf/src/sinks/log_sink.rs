use crate::PerfEventKind;

#[inline(always)]
pub fn emit(name: &'static str, kind: PerfEventKind) {
    log::trace!("perf {} {:?}", name, kind);
}
