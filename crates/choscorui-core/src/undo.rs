//! Reusable undo history for application actions.
//!
//! Decision: text editing keeps its own snapshot history inside
//! [`crate::TextBuffer`], so caret and selection are restored exactly and
//! IME marked text never enters history (only committed text does). Other
//! application actions use this stack. An editor that must interleave text
//! edits with other actions records each committed text change as one
//! entry here, holding the buffer snapshot it needs to restore.

/// Entries applied together and undone together, with a menu label such
/// as "Typing" or "Delete Rows".
#[derive(Clone, Debug, PartialEq)]
pub struct UndoGroup<T> {
    pub label: String,
    pub entries: Vec<T>,
}

/// Bounded undo/redo history. The stack stores what happened; the caller
/// applies each returned group (reverting entries in reverse order for
/// undo, re-applying in order for redo).
#[derive(Clone, Debug)]
pub struct UndoStack<T> {
    groups: Vec<UndoGroup<T>>,
    /// Number of groups currently applied; groups past it are redoable.
    applied: usize,
    limit: usize,
    /// `applied` value at the last save, if still reachable.
    clean: Option<usize>,
    open: Option<(UndoGroup<T>, usize)>,
    /// Whether the newest group may absorb a merge.
    mergeable: bool,
}

impl<T> UndoStack<T> {
    /// Keeps at most `limit` groups; older groups are dropped first.
    pub fn new(limit: usize) -> Self {
        Self {
            groups: Vec::new(),
            applied: 0,
            limit: limit.max(1),
            clean: Some(0),
            open: None,
            mergeable: false,
        }
    }

    /// Records an action that has already been applied. Discards redo
    /// history. Inside [`begin_group`](Self::begin_group) it joins the group.
    pub fn push(&mut self, label: impl Into<String>, entry: T) {
        if let Some((group, _)) = &mut self.open {
            group.entries.push(entry);
            return;
        }
        self.commit(UndoGroup {
            label: label.into(),
            entries: vec![entry],
        });
        self.mergeable = true;
    }

    /// Like [`push`](Self::push), but first offers `entry` to the newest
    /// entry (for example consecutive typing). `merge` returns true when it
    /// absorbed `entry`. Merging never crosses an undo, a save point, or a
    /// [`break_merge`](Self::break_merge).
    pub fn push_merging(
        &mut self,
        label: impl Into<String>,
        entry: T,
        merge: impl FnOnce(&mut T, &T) -> bool,
    ) {
        let label = label.into();
        let can_merge = self.open.is_none()
            && self.mergeable
            && self.applied == self.groups.len()
            && self.clean != Some(self.applied);
        if can_merge
            && let Some(last) = self.groups.last_mut()
            && last.label == label
            && let Some(previous) = last.entries.last_mut()
            && merge(previous, &entry)
        {
            return;
        }
        self.push(label, entry);
    }

    /// Ends merging so the next push starts a new group.
    pub fn break_merge(&mut self) {
        self.mergeable = false;
    }

    /// Starts a compound action; nested calls are counted.
    pub fn begin_group(&mut self, label: impl Into<String>) {
        match &mut self.open {
            Some((_, depth)) => *depth += 1,
            None => {
                self.open = Some((
                    UndoGroup {
                        label: label.into(),
                        entries: Vec::new(),
                    },
                    1,
                ));
            }
        }
    }

    /// Closes the innermost group. An empty outermost group records nothing.
    pub fn end_group(&mut self) {
        let Some((_, depth)) = &mut self.open else {
            return;
        };
        *depth -= 1;
        if *depth > 0 {
            return;
        }
        let (group, _) = self.open.take().expect("open group");
        if !group.entries.is_empty() {
            self.commit(group);
            self.mergeable = false;
        }
    }

    fn commit(&mut self, group: UndoGroup<T>) {
        self.groups.truncate(self.applied);
        if self.clean.is_some_and(|clean| clean > self.applied) {
            self.clean = None;
        }
        self.groups.push(group);
        self.applied += 1;
        if self.groups.len() > self.limit {
            self.groups.remove(0);
            self.applied -= 1;
            self.clean = match self.clean {
                Some(0) | None => None,
                Some(clean) => Some(clean - 1),
            };
        }
    }

    /// Moves back one group and returns it for the caller to revert.
    pub fn undo(&mut self) -> Option<&UndoGroup<T>> {
        if self.open.is_some() || self.applied == 0 {
            return None;
        }
        self.applied -= 1;
        self.mergeable = false;
        self.groups.get(self.applied)
    }

    /// Moves forward one group and returns it for the caller to re-apply.
    pub fn redo(&mut self) -> Option<&UndoGroup<T>> {
        if self.open.is_some() || self.applied == self.groups.len() {
            return None;
        }
        self.applied += 1;
        self.mergeable = false;
        self.groups.get(self.applied - 1)
    }

    pub fn can_undo(&self) -> bool {
        self.open.is_none() && self.applied > 0
    }
    pub fn can_redo(&self) -> bool {
        self.open.is_none() && self.applied < self.groups.len()
    }
    /// Label for an "Undo …" menu item.
    pub fn undo_label(&self) -> Option<&str> {
        self.can_undo()
            .then(|| self.groups[self.applied - 1].label.as_str())
    }
    /// Label for a "Redo …" menu item.
    pub fn redo_label(&self) -> Option<&str> {
        self.can_redo()
            .then(|| self.groups[self.applied].label.as_str())
    }

    /// Marks the current state as saved.
    pub fn set_clean(&mut self) {
        self.clean = Some(self.applied);
        self.mergeable = false;
    }
    /// True when the document matches the last save.
    pub fn is_clean(&self) -> bool {
        self.clean == Some(self.applied) && self.open.is_none()
    }

    pub fn clear(&mut self) {
        self.groups.clear();
        self.applied = 0;
        self.clean = Some(0);
        self.open = None;
        self.mergeable = false;
    }
}
