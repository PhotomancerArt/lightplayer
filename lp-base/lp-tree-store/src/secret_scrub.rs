//! Collecting a replaced secret promptly (FORMAT.md "Secret files"). A
//! replaced or deleted file's records are garbage, and garbage stays on
//! flash, readable in a dump of the partition, until GC happens to collect
//! its sector — which on a store far from full can be never. For a
//! write-only file (an access file, the network file: the Wi-Fi password)
//! that leaves every old secret on the chip. So the writer of such a file
//! collects the sectors that held its old records right after the write
//! commits: their live records are copied (and read back) and the sectors
//! killed and erased — GC's own step (I2), run on purpose.

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_copy::collect_sector;
use crate::node_read::leaf_list;
use crate::object_hasher::ObjectHasher;
use crate::object_id::ObjectId;
use crate::tree_store::{Res, TreeStore, Txn};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// The node id of the file at `path` (`None` when there is none): what
    /// a caller keeps before a write so it can [`Self::scrub`] what the
    /// write replaced.
    pub fn file_node(&mut self, path: &str) -> Res<Option<ObjectId>, F> {
        Ok(self.lookup(path)?.map(|fe| fe.id))
    }

    /// Erase the sectors that hold file node `old`'s records — its top and
    /// its chunks — once nothing live names them: a mark, then each such
    /// sector collected (its live records copied, then killed and erased),
    /// a head renewed. A record still live (the same bytes in another file)
    /// keeps its sector. Outside a transaction only: inside one this does
    /// nothing (scrub after the commit). Best effort: with no free sector to
    /// copy into, the rest is left to GC; a cut leaves the store as GC's
    /// own cut would (I2), and what it had not reached for ordinary GC.
    pub fn scrub(&mut self, old: ObjectId) -> Res<(), F> {
        if self.txn != Txn::None || self.log.index.get(old).is_none() {
            return Ok(());
        }
        let mut ids: Vec<ObjectId> = leaf_list(&mut self.log, old)?
            .iter()
            .map(|l| l.id)
            .collect();
        ids.push(old);
        let mut sectors: Vec<u32> = ids
            .iter()
            .filter_map(|&id| self.log.index.get(id).map(|l| l.sector))
            .collect();
        sectors.sort_unstable();
        sectors.dedup();
        let lens = self.mark_and_prune(true)?;
        let pass_from = self.log.next_sector_seq;
        for s in sectors {
            let still_live = ids
                .iter()
                .any(|&id| self.log.index.get(id).is_some_and(|l| l.sector == s));
            if still_live || self.log.sectors.is_retired(s) || !self.log.sectors.is_written(s) {
                continue;
            }
            if self.log.free_count() == 0 {
                break;
            }
            let pack = !self.log.is_head(s);
            collect_sector(&mut self.log, s, pack, &lens, pass_from)?;
        }
        // A copy that failed its read-back retired a sector: persist that
        // in a root, as a write's commit would (no root when nothing moved).
        self.commit_inner()
    }
}
