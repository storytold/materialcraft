use crate::snapshot::Snapshot;

/// Snapshot stacks and gesture boundaries shared by active and inactive documents
#[derive(Default)]
pub(crate) struct DocumentHistory {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    merge_key: Option<String>,
}

impl DocumentHistory {
    pub(crate) fn record(&mut self, snapshot: Snapshot, key: Option<&str>, limit: usize) {
        self.mark_saved_state(&snapshot);
        // Retain the state before a gesture, while each edit still has its own rollback snapshot
        if key.is_none() || self.merge_key.as_deref() != key || self.undo.is_empty() {
            self.undo.push(snapshot);
        }

        self.redo.clear();
        self.trim(limit);
        self.merge_key = key.map(str::to_owned);
    }

    /// A clean incoming state is the one on disk, so every stored state now differs from the file
    fn mark_saved_state(&mut self, incoming: &Snapshot) {
        if !incoming.modified {
            for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
                snapshot.modified = true;
            }
        }
    }

    pub(crate) fn trim(&mut self, limit: usize) {
        // Both stacks pop from the end, so discard their most distant states first
        self.undo.drain(..self.undo.len().saturating_sub(limit));
        self.redo.drain(..self.redo.len().saturating_sub(limit));
    }

    pub(crate) fn finish_edit(&mut self) {
        self.merge_key = None;
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn labels(&self) -> Vec<String> {
        self.undo.iter().map(|snapshot| snapshot.label.clone()).collect()
    }

    pub(crate) fn undo(&mut self, mut current: Snapshot, limit: usize) -> Option<Snapshot> {
        self.mark_saved_state(&current);
        let snapshot = self.undo.pop()?;
        current.label.clone_from(&snapshot.label);
        self.redo.push(current);
        self.trim(limit);
        self.finish_edit();
        Some(snapshot)
    }

    pub(crate) fn redo(&mut self, mut current: Snapshot, limit: usize) -> Option<Snapshot> {
        self.mark_saved_state(&current);
        let snapshot = self.redo.pop()?;
        current.label.clone_from(&snapshot.label);
        self.undo.push(current);
        self.trim(limit);
        self.finish_edit();
        Some(snapshot)
    }

    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.finish_edit();
    }
}
