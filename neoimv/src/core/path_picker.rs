//! Path picker: type a path to open a file or folder, or search below it.
//!
//! Text up to the last separator selects the search root; the rest is an
//! fzf-style query (via `nucleo-matcher`, including `^prefix`, `suffix$`,
//! `'exact` and `!negate` atoms). With an empty query the root's direct children
//! are listed for browsing. With a query the root is also scanned recursively in
//! a background thread. A query typed without any separator additionally matches
//! zoxide's frecent folders when `zoxide` is installed.

use std::collections::VecDeque;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use natord::compare as natural_compare;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::core::drag_drop::is_image_extension;

/// Rows rendered in the result list; navigation scope still covers every match.
const MAX_VISIBLE_ROWS: usize = 200;
/// The recursive scan stops after collecting this many images…
const MAX_SCAN_IMAGES: usize = 50_000;
/// …or after visiting this many directory entries, whichever comes first.
const MAX_SCAN_VISITS: usize = 1_000_000;
/// Zoxide folders listed above local results.
const MAX_ZOXIDE_ROWS: usize = 5;

/// What the user chose to open.
#[derive(Debug, PartialEq)]
pub enum OpenRequest {
    /// Open a file or folder with its whole directory as the navigation list.
    Path(PathBuf),
    /// Open `images[index]` and navigate only within `images`.
    Scoped {
        images: Vec<PathBuf>,
        index: usize,
        label: String,
    },
}

struct Entry {
    name: String,
    lower: String,
    /// Path relative to the search root (absolute for zoxide folders); shown and fuzzy-matched.
    rel: String,
    path: PathBuf,
    is_dir: bool,
    /// Real (non-symlink) folder the recursive scan may descend into.
    walkable: bool,
    /// 1 for direct children of the search root.
    depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Local(usize),
    Zoxide(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Exact,
    Prefix,
    Substring,
    Fuzzy,
}

/// Sort key, lowest first: name tier, position or inverted fzf score, depth, folders first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    tier: Tier,
    rank: u32,
    depth: u32,
    is_file: bool,
}

struct Ranker {
    query: String,
    pattern: Pattern,
    show_hidden: bool,
    zoxide: bool,
}

impl Ranker {
    fn new(query: &str, zoxide: bool) -> Self {
        Self {
            query: query.to_lowercase(),
            pattern: Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart),
            show_hidden: query.starts_with('.'),
            zoxide: zoxide && !query.is_empty(),
        }
    }

    fn key(&self, entry: &Entry, matcher: &mut Matcher, buf: &mut Vec<char>) -> Option<Key> {
        if !self.show_hidden && entry.name.starts_with('.') {
            return None;
        }
        if self.query.is_empty() {
            return (entry.depth == 1).then_some(Key {
                tier: Tier::Prefix,
                rank: 0,
                depth: 1,
                is_file: !entry.is_dir,
            });
        }
        // Queries never contain a separator, so local entries match on their name.
        let score = self
            .pattern
            .score(Utf32Str::new(&entry.name, buf), matcher)?;
        let (tier, rank) = if entry.lower == self.query {
            (Tier::Exact, 0)
        } else if entry.lower.starts_with(&self.query) {
            (Tier::Prefix, 0)
        } else if let Some(pos) = entry.lower.find(&self.query) {
            (Tier::Substring, pos as u32)
        } else {
            (Tier::Fuzzy, u32::MAX - score)
        };
        Some(Key {
            tier,
            rank,
            depth: entry.depth,
            is_file: !entry.is_dir,
        })
    }

    /// zoxide's own rule: space-separated keywords appear in order (case-insensitive)
    /// and the last one lies in the final path component.
    fn zoxide_matches(&self, entry: &Entry) -> bool {
        let keywords: Vec<&str> = self.query.split_whitespace().collect();
        let Some((last, rest)) = keywords.split_last() else {
            return false;
        };
        let path = entry.rel.to_lowercase();
        let Some(idx) = path.rfind(last) else {
            return false;
        };
        if path[idx + last.len()..].contains(is_separator) {
            return false;
        }
        let mut head = &path[..idx];
        for keyword in rest.iter().rev() {
            match head.rfind(keyword) {
                Some(idx) => head = &head[..idx],
                None => return false,
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    Idle,
    Running,
    Done { truncated: bool },
}

enum ScanMsg {
    Batch(Vec<Entry>),
    Done { truncated: bool },
}

/// Background recursive scan; dropping it cancels the walk.
struct Scan {
    rx: Receiver<ScanMsg>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct PathPicker {
    open: bool,
    input: String,
    /// Folder that relative input resolves against.
    base_dir: PathBuf,
    use_zoxide: bool,
    repaint: Option<egui::Context>,
    /// Input that `matches` was computed for.
    ranked_input: Option<String>,
    /// Search root that `entries` belongs to.
    root: Option<PathBuf>,
    /// Direct children of `root`, then deeper entries in scan (breadth-first) order.
    entries: Vec<Entry>,
    scan: Option<Scan>,
    scan_state: ScanState,
    zoxide: Vec<Entry>,
    zoxide_rx: Option<Receiver<Vec<Entry>>>,
    ranker: Ranker,
    matcher: Matcher,
    local_hits: Vec<(Key, usize)>,
    zoxide_hits: Vec<usize>,
    matches: Vec<Hit>,
    selected: usize,
    /// Row the user moved to; kept selected while scan results reorder the list.
    pinned: Option<Hit>,
    scroll_to_selected: bool,
    move_cursor_to_end: bool,
    error: Option<String>,
}

impl PathPicker {
    pub fn new() -> Self {
        Self::with_zoxide(true)
    }

    fn with_zoxide(use_zoxide: bool) -> Self {
        Self {
            open: false,
            input: String::new(),
            base_dir: PathBuf::new(),
            use_zoxide,
            repaint: None,
            ranked_input: None,
            root: None,
            entries: Vec::new(),
            scan: None,
            scan_state: ScanState::Idle,
            zoxide: Vec::new(),
            zoxide_rx: None,
            ranker: Ranker::new("", false),
            matcher: Matcher::new(Config::DEFAULT),
            local_hits: Vec::new(),
            zoxide_hits: Vec::new(),
            matches: Vec::new(),
            selected: 0,
            pinned: None,
            scroll_to_selected: false,
            move_cursor_to_end: false,
            error: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Open the picker with `input` prefilled; relative input resolves against `base_dir`.
    pub fn open(&mut self, input: String, base_dir: PathBuf) {
        self.open = true;
        self.input = input;
        self.base_dir = base_dir;
        // Folders may have changed since last use: list and scan afresh.
        self.root = None;
        self.ranked_input = None;
        self.error = None;
        self.move_cursor_to_end = true;
        if self.use_zoxide {
            self.zoxide_rx = Some(spawn_zoxide(self.repaint.clone()));
        }
    }

    fn close(&mut self) {
        self.open = false;
        self.scan = None;
        self.scan_state = ScanState::Idle;
        self.root = None;
    }

    /// Render the picker; returns what to open once the user confirms.
    pub fn render(&mut self, ctx: &egui::Context) -> Option<OpenRequest> {
        if !self.open {
            return None;
        }
        if self.repaint.is_none() {
            self.repaint = Some(ctx.clone());
        }

        // Take navigation keys before the text field sees them.
        let (up, down, tab, browse, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });

        if escape {
            self.close();
            return None;
        }

        self.refresh();
        let visible = self.matches.len().min(MAX_VISIBLE_ROWS);
        if up && self.selected > 0 {
            self.select(self.selected - 1);
        }
        if down && self.selected + 1 < visible {
            self.select(self.selected + 1);
        }
        if tab {
            self.complete();
            self.refresh();
        }

        let mut confirm = enter || browse;
        let input_id = egui::Id::new("path_picker_input");
        let width = (ctx.screen_rect().width() - 32.0).clamp(200.0, 720.0);

        egui::Window::new("Open")
            .id(egui::Id::new("path_picker"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
            .show(ctx, |ui| {
                ui.set_width(width);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.input)
                        .id(input_id)
                        .lock_focus(true)
                        .hint_text("Path, folder, or fzf query")
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();
                if self.move_cursor_to_end {
                    self.move_cursor_to_end = false;
                    if let Some(mut state) = egui::TextEdit::load_state(ctx, input_id) {
                        let end = egui::text::CCursor::new(self.input.chars().count());
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(end)));
                        state.store(ctx, input_id);
                    }
                }
                if response.changed() {
                    self.error = None;
                    self.refresh();
                }

                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                ui.separator();

                let mut clicked = None;
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if self.matches.is_empty() {
                            ui.weak("No matching images or folders");
                        }
                        for (row, &hit) in self.matches.iter().take(MAX_VISIBLE_ROWS).enumerate() {
                            let entry = hit_entry(&self.entries, &self.zoxide, hit);
                            let mut label = entry.rel.clone();
                            if entry.is_dir {
                                label.push(MAIN_SEPARATOR);
                            }
                            if matches!(hit, Hit::Zoxide(_)) {
                                label.insert_str(0, "z  ");
                            }
                            let row_response = ui.selectable_label(row == self.selected, label);
                            if row == self.selected && self.scroll_to_selected {
                                row_response.scroll_to_me(None);
                            }
                            if row_response.clicked() {
                                clicked = Some(row);
                            }
                        }
                        if self.matches.len() > MAX_VISIBLE_ROWS {
                            ui.weak(format!(
                                "… {} more",
                                self.matches.len() - MAX_VISIBLE_ROWS
                            ));
                        }
                    });
                self.scroll_to_selected = false;
                if let Some(row) = clicked {
                    self.select(row);
                    confirm = true;
                }

                ui.separator();
                let scan = match self.scan_state {
                    ScanState::Running => " · scanning subfolders…",
                    ScanState::Done { truncated: true } => " · scan limit reached",
                    _ => "",
                };
                ui.weak(format!(
                    "{} matches{} · Enter open · Ctrl+Enter browse matches · Tab complete · Esc close",
                    self.matches.len(),
                    scan
                ));
            });

        if !confirm {
            return None;
        }
        self.refresh();
        match self.resolve(browse) {
            Ok(request) => {
                self.close();
                Some(request)
            }
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    fn select(&mut self, row: usize) {
        self.selected = row;
        self.pinned = self.matches.get(row).copied();
        self.scroll_to_selected = true;
    }

    /// Pull background results and re-rank what changed.
    fn refresh(&mut self) {
        let (new_from, zoxide_changed) = self.poll_background();

        if self.ranked_input.as_deref() != Some(self.input.as_str()) {
            let (root, query) = split_input(&self.input, &self.base_dir);
            let query = query.to_string();
            if self.root.as_deref() != Some(root.as_path()) {
                self.scan = None;
                self.scan_state = ScanState::Idle;
                self.entries = list_dir(&root);
                self.root = Some(root);
            }
            if !query.is_empty() && self.scan_state == ScanState::Idle {
                self.start_scan();
            }
            let typed_bare_name = !self.input.contains(is_separator);
            self.ranker = Ranker::new(&query, typed_bare_name);
            self.ranked_input = Some(self.input.clone());
            self.pinned = None;
            self.scroll_to_selected = true;
            self.local_hits.clear();
            self.score_from(0);
            self.score_zoxide();
        } else {
            if new_from.is_none() && !zoxide_changed {
                return;
            }
            if let Some(from) = new_from {
                self.score_from(from);
            }
            if zoxide_changed {
                self.score_zoxide();
            }
        }

        self.matches.clear();
        self.matches
            .extend(self.zoxide_hits.iter().map(|&i| Hit::Zoxide(i)));
        self.matches
            .extend(self.local_hits.iter().map(|&(_, i)| Hit::Local(i)));
        self.selected = self
            .pinned
            .and_then(|pinned| {
                self.matches
                    .iter()
                    .take(MAX_VISIBLE_ROWS)
                    .position(|&hit| hit == pinned)
            })
            .unwrap_or(0);
    }

    /// Returns the first new index in `entries` (if any arrived) and whether zoxide loaded.
    fn poll_background(&mut self) -> (Option<usize>, bool) {
        let mut zoxide_changed = false;
        if let Some(rx) = &self.zoxide_rx {
            if let Ok(entries) = rx.try_recv() {
                self.zoxide = entries;
                self.zoxide_rx = None;
                zoxide_changed = true;
            }
        }

        let mut new_from = None;
        let mut finished = false;
        if let Some(scan) = &self.scan {
            for msg in scan.rx.try_iter() {
                match msg {
                    ScanMsg::Batch(batch) => {
                        new_from.get_or_insert(self.entries.len());
                        self.entries.extend(batch);
                    }
                    ScanMsg::Done { truncated } => {
                        self.scan_state = ScanState::Done { truncated };
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.scan = None;
        }
        (new_from, zoxide_changed)
    }

    fn start_scan(&mut self) {
        let start: Vec<(PathBuf, String)> = self
            .entries
            .iter()
            .filter(|e| e.walkable && !e.name.starts_with('.'))
            .map(|e| (e.path.clone(), e.rel.clone()))
            .collect();
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let notify = || {
                if let Some(ctx) = &repaint {
                    ctx.request_repaint();
                }
            };
            let truncated = walk(start, &flag, |batch| {
                let sent = tx.send(ScanMsg::Batch(batch)).is_ok();
                notify();
                sent
            });
            let _ = tx.send(ScanMsg::Done { truncated });
            notify();
        });
        self.scan = Some(Scan { rx, cancel });
        self.scan_state = ScanState::Running;
    }

    fn score_from(&mut self, from: usize) {
        let mut buf = Vec::new();
        for idx in from..self.entries.len() {
            if let Some(key) = self
                .ranker
                .key(&self.entries[idx], &mut self.matcher, &mut buf)
            {
                self.local_hits.push((key, idx));
            }
        }
        sort_hits(&self.entries, &mut self.local_hits);
    }

    /// Zoxide folders keep zoxide's frecency order.
    fn score_zoxide(&mut self) {
        self.zoxide_hits.clear();
        if !self.ranker.zoxide {
            return;
        }
        for (idx, entry) in self.zoxide.iter().enumerate() {
            if self.zoxide_hits.len() == MAX_ZOXIDE_ROWS {
                break;
            }
            if self.ranker.zoxide_matches(entry) {
                self.zoxide_hits.push(idx);
            }
        }
    }

    /// Replace the typed query with the selected entry; folders gain a trailing separator.
    fn complete(&mut self) {
        let Some(&hit) = self.matches.get(self.selected) else {
            return;
        };
        let entry = hit_entry(&self.entries, &self.zoxide, hit);
        let completed = match hit {
            Hit::Local(_) => format!("{}{}", &self.input[..query_start(&self.input)], entry.rel),
            Hit::Zoxide(_) => entry.rel.clone(),
        };
        self.input = completed;
        if entry.is_dir {
            self.input.push(MAIN_SEPARATOR);
        }
        self.move_cursor_to_end = true;
    }

    /// `browse` restricts navigation to the matched images instead of the image's folder.
    fn resolve(&self, browse: bool) -> Result<OpenRequest, String> {
        let (dir, query) = split_input(&self.input, &self.base_dir);
        if query.is_empty() {
            return if dir.is_dir() {
                Ok(OpenRequest::Path(dir))
            } else {
                Err(format!("Not a folder: {}", dir.display()))
            };
        }

        let Some(&hit) = self.matches.get(self.selected) else {
            return Err("No matching images or folders".to_string());
        };
        let entry = hit_entry(&self.entries, &self.zoxide, hit);
        if entry.is_dir || !browse {
            return Ok(OpenRequest::Path(entry.path.clone()));
        }

        let images: Vec<PathBuf> = self
            .local_hits
            .iter()
            .map(|&(_, idx)| &self.entries[idx])
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        let index = images.iter().position(|p| *p == entry.path).unwrap_or(0);
        Ok(OpenRequest::Scoped {
            images,
            index,
            // Absolute, so reopening the picker restores it whatever the base folder is.
            label: dir.join(query).to_string_lossy().into_owned(),
        })
    }
}

fn hit_entry<'a>(entries: &'a [Entry], zoxide: &'a [Entry], hit: Hit) -> &'a Entry {
    match hit {
        Hit::Local(idx) => &entries[idx],
        Hit::Zoxide(idx) => &zoxide[idx],
    }
}

fn sort_hits(entries: &[Entry], hits: &mut [(Key, usize)]) {
    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| natural_compare(&entries[a.1].rel, &entries[b.1].rel))
    });
}

fn is_separator(c: char) -> bool {
    c == '/' || c == MAIN_SEPARATOR
}

/// Byte offset where the query starts (just after the last separator).
fn query_start(input: &str) -> usize {
    input.rfind(is_separator).map_or(0, |i| i + 1)
}

/// Split input into the search root and the query.
fn split_input<'a>(input: &'a str, base_dir: &Path) -> (PathBuf, &'a str) {
    let (dir_part, query) = input.split_at(query_start(input));
    (resolve_dir(dir_part, base_dir), query)
}

fn resolve_dir(dir_part: &str, base_dir: &Path) -> PathBuf {
    if let Some(rest) = dir_part.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with(is_separator) {
            if let Some(dirs) = directories::BaseDirs::new() {
                return dirs.home_dir().join(rest.trim_start_matches(is_separator));
            }
        }
    }
    let path = Path::new(dir_part);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

/// Folder or supported image, or `None` for anything else.
fn make_entry(dir_entry: std::fs::DirEntry, parent_rel: &str, depth: u32) -> Option<Entry> {
    let name = dir_entry.file_name().into_string().ok()?;
    let path = dir_entry.path();
    let file_type = dir_entry.file_type().ok()?;
    let (is_dir, walkable) = if file_type.is_symlink() {
        (path.is_dir(), false)
    } else {
        (file_type.is_dir(), file_type.is_dir())
    };
    if !is_dir {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        if !is_image_extension(&ext) {
            return None;
        }
    }
    let rel = if parent_rel.is_empty() {
        name.clone()
    } else {
        format!("{}{}{}", parent_rel, MAIN_SEPARATOR, name)
    };
    Some(Entry {
        lower: name.to_lowercase(),
        name,
        rel,
        path,
        is_dir,
        walkable,
        depth,
    })
}

/// Direct children of `dir`; unreadable folders list as empty.
fn list_dir(dir: &Path) -> Vec<Entry> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read_dir
        .filter_map(Result::ok)
        .filter_map(|dir_entry| make_entry(dir_entry, "", 1))
        .collect()
}

/// Breadth-first walk below `start` folders (depth 2+), skipping dotfiles and not
/// following symlinks. `emit` returns false once the receiver is gone. Returns
/// whether a scan limit stopped the walk.
fn walk(
    start: Vec<(PathBuf, String)>,
    cancel: &AtomicBool,
    mut emit: impl FnMut(Vec<Entry>) -> bool,
) -> bool {
    let mut queue: VecDeque<(PathBuf, String, u32)> = start
        .into_iter()
        .map(|(path, rel)| (path, rel, 2))
        .collect();
    let mut images = 0;
    let mut visits = 0;
    while let Some((dir, rel, depth)) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut batch = Vec::new();
        for dir_entry in read_dir.filter_map(Result::ok) {
            visits += 1;
            if visits > MAX_SCAN_VISITS || images >= MAX_SCAN_IMAGES {
                emit(batch);
                return true;
            }
            let Some(entry) = make_entry(dir_entry, &rel, depth) else {
                continue;
            };
            if entry.name.starts_with('.') {
                continue;
            }
            if entry.walkable {
                queue.push_back((entry.path.clone(), entry.rel.clone(), depth + 1));
            }
            if !entry.is_dir {
                images += 1;
            }
            batch.push(entry);
        }
        if !batch.is_empty() && !emit(batch) {
            return false;
        }
    }
    false
}

/// Load zoxide's folders (frecency order) off the UI thread; empty if zoxide is missing.
fn spawn_zoxide(repaint: Option<egui::Context>) -> Receiver<Vec<Entry>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new("zoxide")
            .args(["query", "--list"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let entries = output
            .ok()
            .filter(|out| out.status.success())
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
                    .map(|path| {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        Entry {
                            lower: name.to_lowercase(),
                            name,
                            rel: path.to_string_lossy().into_owned(),
                            path,
                            is_dir: true,
                            walkable: false,
                            depth: 0,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let _ = tx.send(entries);
        if let Some(ctx) = repaint {
            ctx.request_repaint();
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn entry(rel: &str, is_dir: bool) -> Entry {
        let name = rel.rsplit('/').next().unwrap().to_string();
        Entry {
            lower: name.to_lowercase(),
            name,
            rel: rel.to_string(),
            path: PathBuf::from("/x").join(rel),
            is_dir,
            walkable: is_dir,
            depth: rel.matches('/').count() as u32 + 1,
        }
    }

    fn ranked<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a str> {
        let ranker = Ranker::new(query, false);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        let mut hits: Vec<(Key, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| ranker.key(e, &mut matcher, &mut buf).map(|k| (k, i)))
            .collect();
        sort_hits(entries, &mut hits);
        hits.into_iter()
            .map(|(_, i)| entries[i].rel.as_str())
            .collect()
    }

    #[test]
    fn rank_orders_prefix_then_substring_then_fuzzy_shallow_first() {
        let entries = [
            entry("b_i_o_card-.png", false),
            entry("old-biocard-1.png", false),
            entry("sub/biocard-3.png", false),
            entry("biocard-10.png", false),
            entry("biocard-2.png", false),
            entry("unrelated.png", false),
            entry("biocard-", false),
        ];
        assert_eq!(
            ranked(&entries, "BioCard-"),
            [
                "biocard-",
                "biocard-2.png",
                "biocard-10.png",
                "sub/biocard-3.png",
                "old-biocard-1.png",
                "b_i_o_card-.png",
            ]
        );
    }

    #[test]
    fn rank_empty_query_lists_direct_children_folders_first() {
        let entries = [
            entry("b.png", false),
            entry("a/deep.png", false),
            entry("z", true),
            entry(".hidden.png", false),
        ];
        assert_eq!(ranked(&entries, ""), ["z", "b.png"]);
        assert_eq!(ranked(&entries, ".h"), [".hidden.png"]);
    }

    #[test]
    fn zoxide_keywords_follow_zoxide_rules() {
        let ranker = |q: &str| Ranker::new(q, true);
        let dir = |path: &str| Entry {
            name: String::new(),
            lower: String::new(),
            rel: path.to_string(),
            path: PathBuf::from(path),
            is_dir: true,
            walkable: false,
            depth: 0,
        };
        let ws = dir("/home/u/Documents/ws");
        assert!(ranker("ws").zoxide_matches(&ws));
        assert!(ranker("DOC ws").zoxide_matches(&ws));
        assert!(!ranker("ws doc").zoxide_matches(&ws));
        // Scattered letters (u/…/.ssh) do not count; the last keyword must sit in the last component.
        assert!(!ranker("ws").zoxide_matches(&dir("/home/uw/.ssh")));
        assert!(!ranker("doc").zoxide_matches(&ws));
    }

    #[test]
    fn rank_supports_fzf_extended_syntax() {
        let entries = [
            entry("cat.png", false),
            entry("cat.jpg", false),
            entry("dog.png", false),
        ];
        assert_eq!(ranked(&entries, "png$ !dog"), ["cat.png"]);
    }

    #[test]
    fn split_input_resolves_relative_absolute_and_trailing_separator() {
        let base = Path::new("/base");
        assert_eq!(split_input("cat", base), (PathBuf::from("/base"), "cat"));
        assert_eq!(
            split_input("sub/ca", base),
            (PathBuf::from("/base/sub"), "ca")
        );
        assert_eq!(
            split_input("/tmp/biocard-", base),
            (PathBuf::from("/tmp"), "biocard-")
        );
        assert_eq!(split_input("/tmp/", base), (PathBuf::from("/tmp"), ""));
        let home = directories::BaseDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf();
        assert_eq!(split_input("~/pi", base), (home, "pi"));
    }

    fn fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("neoimv_picker_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("biocard-dir/deeper")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        for file in [
            "biocard-2.png",
            "biocard-10.jpg",
            "biocard-.txt",
            "other.png",
            "biocard-dir/deeper/biocard-9.png",
            ".hidden/biocard-h.png",
        ] {
            fs::write(dir.join(file), b"").unwrap();
        }
        dir
    }

    /// Picker after its recursive scan finished.
    fn scanned(dir: &Path, input: &str) -> PathPicker {
        let mut picker = PathPicker::with_zoxide(false);
        picker.open(input.to_string(), dir.to_path_buf());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            picker.refresh();
            if picker.scan_state != ScanState::Running || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        picker
    }

    fn rows(picker: &PathPicker) -> Vec<&str> {
        picker
            .matches
            .iter()
            .map(|&hit| hit_entry(&picker.entries, &picker.zoxide, hit).rel.as_str())
            .collect()
    }

    #[test]
    fn query_scans_subfolders_but_skips_hidden_ones() {
        let dir = fixture("scan");
        let picker = scanned(&dir, "biocard-");
        let deep = format!("biocard-dir{0}deeper{0}biocard-9.png", MAIN_SEPARATOR);
        assert_eq!(
            rows(&picker),
            [
                "biocard-dir",
                "biocard-2.png",
                "biocard-10.jpg",
                deep.as_str()
            ]
        );
        assert_eq!(picker.scan_state, ScanState::Done { truncated: false });
    }

    #[test]
    fn resolve_opens_images_in_their_folder_and_browses_matches_on_request() {
        let dir = fixture("resolve");
        let mut picker = scanned(&dir, "biocard-");
        assert_eq!(
            picker.resolve(false),
            Ok(OpenRequest::Path(dir.join("biocard-dir")))
        );

        picker.selected = 3;
        let deep = dir.join("biocard-dir/deeper/biocard-9.png");
        assert_eq!(picker.resolve(false), Ok(OpenRequest::Path(deep.clone())));
        assert_eq!(
            picker.resolve(true),
            Ok(OpenRequest::Scoped {
                images: vec![dir.join("biocard-2.png"), dir.join("biocard-10.jpg"), deep],
                index: 2,
                label: dir.join("biocard-").to_string_lossy().into_owned(),
            })
        );

        let picker = scanned(&dir, &format!("{}/", dir.display()));
        assert_eq!(picker.resolve(false), Ok(OpenRequest::Path(dir.clone())));

        // Typed paths open even inside hidden folders.
        let picker = scanned(&dir, ".hidden/biocard-h.png");
        assert_eq!(
            picker.resolve(false),
            Ok(OpenRequest::Path(dir.join(".hidden/biocard-h.png")))
        );

        let picker = scanned(&dir, "zzz");
        assert!(picker.resolve(false).is_err());
    }

    #[test]
    fn complete_uses_relative_path_and_appends_separator_for_folders() {
        let dir = fixture("complete");
        let mut picker = scanned(&dir, "deeper");
        picker.complete();
        assert_eq!(
            picker.input,
            format!("biocard-dir{0}deeper{0}", MAIN_SEPARATOR)
        );
    }
}
