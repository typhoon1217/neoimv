use std::collections::VecDeque;

pub struct Mutation {
    pub pixels: Vec<u8>,  // RGBA8, core-owned copy
    pub width: u32,
    pub height: u32,
    pub plugin_name: String,
}

impl Mutation {
    pub fn byte_size(&self) -> usize {
        self.pixels.len()
    }
}

pub struct UndoStack {
    undo: VecDeque<Mutation>,
    redo: Vec<Mutation>,
    max_depth: usize,
    memory_cap_bytes: usize,
    current_memory: usize,
}

impl UndoStack {
    pub fn new(max_depth: usize, memory_cap_mb: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            max_depth,
            memory_cap_bytes: memory_cap_mb * 1024 * 1024,
            current_memory: 0,
        }
    }

    pub fn push(&mut self, mutation: Mutation) {
        // Clear redo stack on new mutation — reclaim memory
        for m in self.redo.drain(..) {
            self.current_memory = self.current_memory.saturating_sub(m.byte_size());
        }

        let new_size = mutation.byte_size();
        self.current_memory += new_size;

        self.undo.push_back(mutation);

        // Enforce depth limit
        while self.undo.len() > self.max_depth {
            if let Some(old) = self.undo.pop_front() {
                self.current_memory = self.current_memory.saturating_sub(old.byte_size());
            }
        }

        // Enforce memory cap
        while self.current_memory > self.memory_cap_bytes && !self.undo.is_empty() {
            if let Some(old) = self.undo.pop_front() {
                self.current_memory = self.current_memory.saturating_sub(old.byte_size());
            }
        }
    }

    pub fn undo(&mut self) -> Option<&Mutation> {
        if let Some(mutation) = self.undo.pop_back() {
            // Memory stays tracked — mutation moves from undo to redo but still in RAM
            self.redo.push(mutation);
            self.undo.back()
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&Mutation> {
        if let Some(mutation) = self.redo.pop() {
            self.undo.push_back(mutation);
            // Enforce memory cap after redo pushes back onto undo stack
            while self.current_memory > self.memory_cap_bytes && self.undo.len() > 1 {
                if let Some(old) = self.undo.pop_front() {
                    self.current_memory = self.current_memory.saturating_sub(old.byte_size());
                }
            }
            self.undo.back()
        } else {
            None
        }
    }

    pub fn current(&self) -> Option<&Mutation> {
        self.undo.back()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.current_memory = 0;
    }

    pub fn depth(&self) -> usize {
        self.undo.len()
    }

    pub fn memory_usage_bytes(&self) -> usize {
        self.current_memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mutation(size: usize, name: &str) -> Mutation {
        Mutation {
            pixels: vec![0u8; size],
            width: 1,
            height: (size / 4) as u32,
            plugin_name: name.to_string(),
        }
    }

    #[test]
    fn test_push_and_undo() {
        let mut stack = UndoStack::new(10, 1024);
        stack.push(make_mutation(100, "a"));
        stack.push(make_mutation(100, "b"));
        assert_eq!(stack.depth(), 2);

        stack.undo();
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.current().unwrap().plugin_name, "a");
    }

    #[test]
    fn test_redo() {
        let mut stack = UndoStack::new(10, 1024);
        stack.push(make_mutation(100, "a"));
        stack.push(make_mutation(100, "b"));
        stack.undo();

        stack.redo();
        assert_eq!(stack.depth(), 2);
        assert_eq!(stack.current().unwrap().plugin_name, "b");
    }

    #[test]
    fn test_new_mutation_clears_redo() {
        let mut stack = UndoStack::new(10, 1024);
        stack.push(make_mutation(100, "a"));
        stack.push(make_mutation(100, "b"));
        stack.undo();

        stack.push(make_mutation(100, "c"));
        // redo should be empty now
        stack.redo(); // should be no-op
        assert_eq!(stack.current().unwrap().plugin_name, "c");
    }

    #[test]
    fn test_max_depth_enforced() {
        let mut stack = UndoStack::new(3, 1024);
        for i in 0..5 {
            stack.push(make_mutation(10, &format!("m{}", i)));
        }
        assert_eq!(stack.depth(), 3);
    }

    #[test]
    fn test_memory_cap_enforced() {
        // Cap at 1 MB
        let mut stack = UndoStack::new(100, 1);
        let half_mb = 512 * 1024;
        stack.push(make_mutation(half_mb, "a"));
        stack.push(make_mutation(half_mb, "b"));
        stack.push(make_mutation(half_mb, "c"));
        // Should have evicted oldest to stay under 1MB
        assert!(stack.memory_usage_bytes() <= 1024 * 1024);
    }

    #[test]
    fn test_clear() {
        let mut stack = UndoStack::new(10, 1024);
        stack.push(make_mutation(100, "a"));
        stack.clear();
        assert_eq!(stack.depth(), 0);
        assert_eq!(stack.memory_usage_bytes(), 0);
    }

    #[test]
    fn test_can_undo_redo() {
        let mut stack = UndoStack::new(10, 1024);
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());

        stack.push(make_mutation(100, "a"));
        assert!(stack.can_undo());
        assert!(!stack.can_redo());

        stack.undo();
        assert!(!stack.can_undo());
        assert!(stack.can_redo());

        stack.redo();
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }
}
