//! What the window is showing: the section, the search, the timeline groups and the
//! selection. Plain data, no widgets, so the rules can be tested.

use crate::grid::Span;
use crate::library::Library;
use crate::metadata::{days_from_civil, days_to_ymd, month_abbr, ymd};
use crate::models::{PhotoItem, format_bytes};
use std::collections::{HashMap, HashSet};

pub const LIBRARY_SECTIONS: &[(&str, &str, &str)] = &[
    ("all_photos", "All photos", "image-x-generic-symbolic"),
    ("favorites", "Favorites", "non-starred-symbolic"),
    ("recent", "Recently added", "document-open-recent-symbolic"),
    ("folders", "Folders", "folder-symbolic"),
    ("duplicates", "Possible duplicates", "edit-copy-symbolic"),
];

pub const MEDIA_SECTIONS: &[(&str, &str, &str)] = &[
    ("photos", "Photos", "camera-photo-symbolic"),
    ("screenshots", "Screenshots", "nyx-photos-screenshot-symbolic"),
    ("videos", "Videos", "video-x-generic-symbolic"),
];

const DAY: i64 = 86400;

#[derive(Default)]
pub struct State {
    pub lib: Library,
    pub section: String,
    pub query: String,
    pub view_mode: String,
    /// Indices into `lib.photos`, in the order the grid shows them.
    pub shown: Vec<usize>,
    pub spans: Vec<Span>,
    /// Selected photo ids, oldest pick first.
    pub selected: Vec<String>,
    /// The photo the inspector and lightbox are about.
    pub cursor: Option<String>,
    /// Where a Shift range starts: the last photo picked without Shift.
    anchor: Option<String>,
    /// Sets of identical files, once they have been looked for.
    pub duplicates: Option<Vec<Vec<String>>>,
    /// Now, on the local wall clock (for "last month" and Recently added). Zero means
    /// the real time.
    pub now: i64,
    index: HashMap<String, usize>,
}

impl State {
    pub fn new(lib: Library, view_mode: String) -> State {
        State { lib, view_mode, section: "all_photos".into(), ..Default::default() }
    }

    pub fn photo(&self, shown_idx: usize) -> Option<&PhotoItem> {
        self.shown.get(shown_idx).and_then(|&i| self.lib.photos.get(i))
    }

    pub fn cursor_index(&self) -> Option<usize> {
        self.cursor.as_ref().and_then(|id| self.index.get(id).copied())
    }

    pub fn cursor_photo(&self) -> Option<&PhotoItem> {
        self.cursor_index().and_then(|i| self.photo(i))
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    pub fn selected_indices(&self) -> HashSet<usize> {
        self.selected.iter().filter_map(|id| self.index.get(id).copied()).collect()
    }

    /// The photos an action applies to: the selection, or just the cursor.
    pub fn target_ids(&self) -> Vec<String> {
        if !self.selected.is_empty() { self.selected.clone() } else { self.cursor.iter().cloned().collect() }
    }

    /// The library's photos with these ids.
    pub fn photos_by_id(&self, ids: &[String]) -> Vec<&PhotoItem> {
        ids.iter().filter_map(|id| self.lib.photos.iter().find(|p| &p.id == id)).collect()
    }

    pub fn album_id(&self) -> Option<&str> {
        self.lib.album(&self.section).map(|a| a.id.as_str())
    }

    pub fn section_title(&self) -> String {
        if let Some(a) = self.lib.album(&self.section) {
            return a.name.clone();
        }
        LIBRARY_SECTIONS
            .iter()
            .chain(MEDIA_SECTIONS)
            .find(|(id, _, _)| *id == self.section)
            .map(|(_, t, _)| t.to_string())
            .unwrap_or_else(|| "All photos".into())
    }

    fn now(&self) -> i64 {
        if self.now != 0 { self.now } else { crate::metadata::now_naive() }
    }

    fn recent_since(&self) -> u64 {
        let unix_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        unix_now.saturating_sub((crate::prefs::get().recent_days.clamp(1, 365) as i64 * DAY) as u64)
    }

    /// The number shown beside a section in the sidebar (`None`: no number).
    pub fn count(&self, section: &str) -> Option<usize> {
        let photos = &self.lib.photos;
        Some(match section {
            "favorites" => photos.iter().filter(|p| p.is_favorite).count(),
            "photos" | "screenshots" | "videos" => photos.iter().filter(|p| p.media_type == section).count(),
            "recent" => {
                let since = self.recent_since();
                photos.iter().filter(|p| p.added >= since).count()
            }
            "all_photos" => photos.len(),
            "folders" => photos.iter().map(|p| p.folder()).collect::<HashSet<_>>().len(),
            "duplicates" => return self.duplicates.as_ref().map(|d| d.iter().map(|s| s.len()).sum()),
            id => self.lib.album(id).map(|a| photos.iter().filter(|p| a.photo_ids.contains(&p.id)).count()).unwrap_or(0),
        })
    }

    /// Work out what is shown from the section, the search and the grouping, and
    /// keep the selection to photos that are still there.
    pub fn recompute(&mut self) {
        let query = Query::parse(&self.query, self.now());
        let album = self.lib.album(&self.section).cloned();
        let since = self.recent_since();
        let mut shown: Vec<usize> = self
            .lib
            .photos
            .iter()
            .enumerate()
            .filter(|(_, p)| match (&album, self.section.as_str()) {
                (Some(a), _) => a.photo_ids.contains(&p.id),
                (None, "favorites") => p.is_favorite,
                (None, "recent") => p.added >= since,
                (None, s @ ("photos" | "screenshots" | "videos")) => p.media_type == s,
                _ => true,
            })
            .filter(|(_, p)| query.matches(p))
            .map(|(i, _)| i)
            .collect();
        self.spans = match self.section.as_str() {
            "duplicates" => self.duplicate_groups(&mut shown),
            "folders" => {
                let photos = &self.lib.photos;
                shown.sort_by(|&a, &b| photos[a].folder().cmp(photos[b].folder()));
                let mut spans =
                    self.group(&mut shown, |p| (crate::paths::pretty(std::path::Path::new(p.folder())), folder_name(p)));
                // Grouped by the whole path; headed by the folder's name, with where it is
                // beside the count.
                for span in &mut spans {
                    let path = std::path::Path::new(&span.title);
                    if let (Some(name), Some(parent)) = (path.file_name(), path.parent()) {
                        span.sub = format!("{} · {}", parent.display(), span.sub);
                        span.title = name.to_string_lossy().into_owned();
                    }
                }
                spans
            }
            "recent" => {
                let photos = &self.lib.photos;
                shown.sort_by(|&a, &b| photos[b].added.cmp(&photos[a].added));
                self.group(&mut shown, |p| {
                    let (y, m, d) = ymd(crate::metadata::local_naive(p.added as i64));
                    (format!("Added {} {d}, {y}", month_abbr(m)), format!("{} {y}", month_abbr(m)))
                })
            }
            _ => self.group_by_date(&mut shown),
        };
        self.shown = shown;
        self.index = self.shown.iter().enumerate().map(|(n, &i)| (self.lib.photos[i].id.clone(), n)).collect();
        self.selected.retain(|id| self.index.contains_key(id));
        if self.cursor.as_ref().is_none_or(|id| !self.index.contains_key(id)) {
            self.cursor = self.shown.first().map(|&i| self.lib.photos[i].id.clone());
            self.selected = self.cursor.iter().cloned().collect();
            self.anchor = self.cursor.clone();
        }
    }

    fn group_by_date(&self, shown: &mut Vec<usize>) -> Vec<Span> {
        if shown.is_empty() {
            return Vec::new();
        }
        let short = |p: &PhotoItem| format!("{} {}", month_abbr(p.month), p.year);
        match self.view_mode.as_str() {
            "all" => {
                let shorts = shown.iter().map(|&i| short(&self.lib.photos[i])).collect();
                vec![Span { title: String::new(), sub: String::new(), short: String::new(), shorts, start: 0, end: shown.len() }]
            }
            "years" => self.group(shown, |p| (p.year.to_string(), p.year.to_string())),
            "months" => self.group(shown, |p| (format!("{} {}", month_name(p.month), p.year), short(p))),
            _ => {
                let today = self.now().div_euclid(DAY);
                let this_year = ymd(self.now()).0;
                self.group(shown, |p| (day_title(p, today, this_year), short(p)))
            }
        }
    }

    /// Reorder `shown` so each group is contiguous (first appearance first) and describe the
    /// groups. `key` gives a photo's heading and its short label for the date scrubber.
    fn group(&self, shown: &mut Vec<usize>, key: impl Fn(&PhotoItem) -> (String, String)) -> Vec<Span> {
        let mut order: Vec<(String, String)> = Vec::new();
        let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in shown.iter() {
            let (title, short) = key(&self.lib.photos[i]);
            if !buckets.contains_key(&title) {
                order.push((title.clone(), short));
            }
            buckets.entry(title).or_default().push(i);
        }
        let mut spans = Vec::new();
        shown.clear();
        for (title, short) in order {
            let items = buckets.remove(&title).unwrap_or_default();
            let start = shown.len();
            let videos = items.iter().filter(|&&i| self.lib.photos[i].is_video()).count();
            let sub = count_line(items.len() - videos, videos);
            shown.extend(items);
            spans.push(Span {
                title,
                sub,
                short,
                shorts: Vec::new(),
                start,
                end: shown.len(),
            });
        }
        spans
    }

    /// One group per set of identical files, in the order they were found.
    fn duplicate_groups(&self, shown: &mut Vec<usize>) -> Vec<Span> {
        let Some(sets) = &self.duplicates else {
            shown.clear();
            return Vec::new();
        };
        let keep: HashSet<usize> = shown.iter().copied().collect();
        let at: HashMap<&str, usize> = self.lib.photos.iter().enumerate().map(|(i, p)| (p.id.as_str(), i)).collect();
        shown.clear();
        let mut spans = Vec::new();
        for set in sets {
            let members: Vec<usize> =
                set.iter().filter_map(|id| at.get(id.as_str()).copied()).filter(|i| keep.contains(i)).collect();
            if members.len() < 2 {
                continue;
            }
            let first = &self.lib.photos[members[0]];
            let start = shown.len();
            shown.extend(&members);
            spans.push(Span {
                title: first.file_name.clone(),
                sub: format!("{} copies · {} each", members.len(), format_bytes(first.file_size)),
                short: first.file_name.clone(),
                shorts: Vec::new(),
                start,
                end: shown.len(),
            });
        }
        spans
    }

    fn pick(&mut self, idx: usize) -> Option<String> {
        self.photo(idx).map(|p| p.id.clone())
    }

    pub fn select_single(&mut self, idx: usize) {
        if let Some(id) = self.pick(idx) {
            self.selected = vec![id.clone()];
            self.anchor = Some(id.clone());
            self.cursor = Some(id);
        }
    }

    pub fn toggle(&mut self, idx: usize) {
        let Some(id) = self.pick(idx) else { return };
        if let Some(at) = self.selected.iter().position(|s| *s == id) {
            self.selected.remove(at);
            if self.cursor.as_ref() == Some(&id) {
                self.cursor = self.selected.last().cloned();
            }
        } else {
            self.selected.push(id.clone());
            self.cursor = Some(id.clone());
        }
        self.anchor = Some(id);
    }

    /// Select from the anchor (the last photo picked without Shift) to `idx`, and move
    /// the cursor there. Moving back shrinks the range again.
    pub fn select_range(&mut self, idx: usize) {
        let from = self.anchor.as_ref().and_then(|id| self.index.get(id).copied()).or(self.cursor_index()).unwrap_or(0);
        let (lo, hi) = (from.min(idx), from.max(idx));
        self.selected = (lo..=hi).filter_map(|n| self.photo(n).map(|p| p.id.clone())).collect();
        if self.anchor.is_none() {
            self.anchor = self.pick(from);
        }
        self.cursor = self.pick(idx);
    }

    pub fn select_all(&mut self) {
        self.selected = self.shown.iter().map(|&i| self.lib.photos[i].id.clone()).collect();
    }

    /// Back to just the first photo selected.
    pub fn clear_selection(&mut self) {
        match self.shown.first() {
            Some(_) => self.select_single(0),
            None => {
                self.selected.clear();
                self.cursor = None;
                self.anchor = None;
            }
        }
    }

    /// Move the cursor to `idx` (clamped) and select just it, or with `extend` grow the
    /// selection from the anchor to it.
    pub fn move_to(&mut self, idx: isize, extend: bool) {
        if self.shown.is_empty() {
            return;
        }
        let idx = idx.clamp(0, self.shown.len() as isize - 1) as usize;
        if extend { self.select_range(idx) } else { self.select_single(idx) }
    }

    /// Step to the next or previous photo, wrapping round (for the lightbox).
    pub fn step_wrapping(&mut self, delta: isize) {
        let n = self.shown.len() as isize;
        if n == 0 {
            return;
        }
        let cur = self.cursor_index().unwrap_or(0) as isize;
        self.select_single((cur + delta).rem_euclid(n) as usize);
    }
}

fn folder_name(p: &PhotoItem) -> String {
    std::path::Path::new(p.folder()).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// A search: words to find in a photo's details, plus filters such as `is:video`,
/// `2024`, `2023-2024`, `2024-05`, `before:2024`, `last month`.
#[derive(Debug, Default)]
pub struct Query {
    terms: Vec<String>,
    is: Vec<String>,
    /// Wall-clock seconds, from (inclusive) and to (exclusive).
    ranges: Vec<(i64, i64)>,
}

impl Query {
    pub fn parse(text: &str, now: i64) -> Query {
        let words: Vec<String> = text.to_lowercase().split_whitespace().map(String::from).collect();
        let today = now.div_euclid(DAY) * DAY;
        let (y, m, _) = ymd(now);
        let mut q = Query::default();
        let mut i = 0;
        while i < words.len() {
            let w = words[i].as_str();
            let next = words.get(i + 1).map(String::as_str);
            let pair = match (w, next) {
                ("this" | "last", Some("week")) => {
                    let monday = today - (today / DAY + 3).rem_euclid(7) * DAY;
                    Some(if w == "this" { (monday, today + DAY) } else { (monday - 7 * DAY, monday) })
                }
                ("this", Some("month")) => Some(month_range(y, m)),
                ("last", Some("month")) => Some(if m == 1 { month_range(y - 1, 12) } else { month_range(y, m - 1) }),
                ("this", Some("year")) => Some(year_range(y, y)),
                ("last", Some("year")) => Some(year_range(y - 1, y - 1)),
                _ => None,
            };
            if let Some(r) = pair {
                q.ranges.push(r);
                i += 2;
                continue;
            }
            i += 1;
            match w {
                "today" => q.ranges.push((today, today + DAY)),
                "yesterday" => q.ranges.push((today - DAY, today)),
                _ => {}
            }
            if matches!(w, "today" | "yesterday") {
                continue;
            }
            if let Some(kind) = w.strip_prefix("is:") {
                q.is.push(kind.to_string());
            } else if let Some(d) = w.strip_prefix("before:").and_then(date_range) {
                q.ranges.push((i64::MIN, d.0));
            } else if let Some(d) = w.strip_prefix("after:").or_else(|| w.strip_prefix("since:")).and_then(date_range) {
                q.ranges.push((d.0, i64::MAX));
            } else if let Some(r) = date_range(w) {
                q.ranges.push(r);
            } else {
                q.terms.push(w.to_string());
            }
        }
        q
    }

    pub fn matches(&self, p: &PhotoItem) -> bool {
        let is = |kind: &str| match kind {
            "fav" | "favorite" | "favourite" | "starred" => p.is_favorite,
            "video" | "videos" => p.media_type == "videos",
            "photo" | "photos" => p.media_type == "photos",
            "screenshot" | "screenshots" => p.media_type == "screenshots",
            "edited" => p.file_name.contains("(edited"),
            _ => false,
        };
        let fields = [
            Some(p.file_name.as_str()),
            p.camera_make.as_deref(),
            p.camera_model.as_deref(),
            p.lens_model.as_deref(),
            Some(p.date_display.as_str()),
            Some(p.format.as_str()),
            Some(p.folder()),
        ];
        self.is.iter().all(|k| is(k))
            && self.ranges.iter().all(|&(from, to)| p.taken >= from && p.taken < to)
            && self.terms.iter().all(|term| fields.iter().flatten().any(|f| f.to_lowercase().contains(term)))
    }
}

fn year_range(from: i32, to: i32) -> (i64, i64) {
    (days_from_civil(from, 1, 1) * DAY, days_from_civil(to + 1, 1, 1) * DAY)
}

fn month_range(y: i32, m: u32) -> (i64, i64) {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (days_from_civil(y, m, 1) * DAY, days_from_civil(ny, nm, 1) * DAY)
}

/// `2024`, `2023-2024`, `2023..2024`, `2024-05` or `2024-05-01` as a span of time.
fn date_range(w: &str) -> Option<(i64, i64)> {
    let year = |s: &str| s.parse::<i32>().ok().filter(|y| s.len() == 4 && (1800..=3000).contains(y));
    let num = |s: &str, max: u32| s.parse::<u32>().ok().filter(|n| (1..=max).contains(n) && s.len() <= 2);
    if let Some((a, b)) = w.split_once("..") {
        return Some(year_range(year(a)?, year(b)?)).filter(|r| r.0 < r.1);
    }
    let parts: Vec<&str> = w.split('-').collect();
    match parts.as_slice() {
        [y] => year(y).map(|y| year_range(y, y)),
        [a, b] if b.len() == 4 => Some(year_range(year(a)?, year(b)?)).filter(|r| r.0 < r.1),
        [y, m] => Some(month_range(year(y)?, num(m, 12)?)),
        [y, m, d] => {
            let start = days_from_civil(year(y)?, num(m, 12)?, num(d, 31)?) * DAY;
            let (_, mm, _) = days_to_ymd(start / DAY);
            (mm == num(m, 12)?).then_some((start, start + DAY))
        }
        _ => None,
    }
}

/// "11 photos", "3 photos, 1 video", "1 video".
pub fn count_line(photos: usize, videos: usize) -> String {
    let part = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match (photos, videos) {
        (p, 0) => part(p, "photo", "photos"),
        (0, v) => part(v, "video", "videos"),
        (p, v) => format!("{}, {}", part(p, "photo", "photos"), part(v, "video", "videos")),
    }
}

/// A day's heading: "Today", "Yesterday", "Saturday 8 November", or with its year
/// when it isn't this year.
fn day_title(p: &PhotoItem, today: i64, this_year: i32) -> String {
    const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    let day = p.taken.div_euclid(DAY);
    match today - day {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ => {
            // Day 0, 1 January 1970, was a Thursday.
            let weekday = WEEKDAYS[day.rem_euclid(7) as usize];
            if p.year == this_year {
                format!("{weekday} {} {}", p.day, month_name(p.month))
            } else {
                format!("{weekday} {} {} {}", p.day, month_name(p.month), p.year)
            }
        }
    }
}

fn month_name(m: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    NAMES.get((m as usize).wrapping_sub(1)).copied().unwrap_or("January")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::metadata::naive;

    pub(crate) fn photo(id: &str, name: &str, year: i32, month: u32, day: u32) -> PhotoItem {
        PhotoItem {
            id: id.into(),
            file_name: name.into(),
            path: format!("/p/{name}"),
            file_size: 1,
            file_size_formatted: "1 B".into(),
            format: "JPG".into(),
            taken: naive(year, month, day, 12, 0, 0),
            date_display: format!("{} {day}, {year}", month_abbr(month)),
            year,
            month,
            day,
            width: 1,
            height: 1,
            thumbnail_path: String::new(),
            mtime: 0,
            added: 0,
            camera_make: None,
            camera_model: Some("X100".into()),
            lens_model: None,
            focal_length: None,
            aperture: None,
            shutter_speed: None,
            iso: None,
            latitude: None,
            longitude: None,
            duration: None,
            is_favorite: false,
            media_type: "photos".into(),
        }
    }

    fn state(mode: &str) -> State {
        let mut s = State { view_mode: mode.into(), section: "all_photos".into(), ..Default::default() };
        s.lib.photos = vec![photo("a", "a.jpg", 2024, 5, 1), photo("b", "b.jpg", 2023, 1, 2), photo("c", "c.jpg", 2024, 6, 3)];
        s.now = naive(2024, 6, 10, 9, 0, 0);
        s.recompute();
        s
    }

    fn shown_ids(s: &State) -> Vec<String> {
        (0..s.shown.len()).map(|i| s.photo(i).unwrap().id.clone()).collect()
    }

    #[test]
    fn groups_by_year_and_keeps_order_contiguous() {
        let s = state("years");
        assert_eq!(s.spans.len(), 2);
        assert_eq!(s.spans[0].title, "2024");
        assert_eq!((s.spans[0].start, s.spans[0].end), (0, 2));
        assert_eq!(s.photo(2).unwrap().id, "b");
    }

    #[test]
    fn days_are_headed_with_the_date_and_what_they_hold() {
        let s = state("days");
        assert_eq!(s.spans[0].title, "Wednesday 1 May");
        assert_eq!(s.spans[0].sub, "1 photo");
        assert_eq!(s.spans[1].title, "Monday 2 January 2023");
        assert_eq!(count_line(3, 1), "3 photos, 1 video");
        assert_eq!(count_line(0, 2), "2 videos");
    }

    #[test]
    fn folders_count_folders() {
        let mut s = state("days");
        s.lib.photos[1].path = "/q/b.jpg".into();
        assert_eq!(s.count("folders"), Some(2));
        assert_eq!(s.count("all_photos"), Some(3));
    }

    #[test]
    fn all_mode_has_one_headless_group() {
        let s = state("all");
        assert_eq!(s.spans.len(), 1);
        assert!(s.spans[0].title.is_empty());
        assert_eq!(s.spans[0].shorts.len(), 3);
    }

    #[test]
    fn query_filters_and_selection_survives() {
        let mut s = state("all");
        s.select_single(1);
        s.query = "c.jpg".into();
        s.recompute();
        assert_eq!(s.shown.len(), 1);
        assert_eq!(s.cursor.as_deref(), Some("c"));
    }

    #[test]
    fn search_understands_dates_and_kinds() {
        let mut s = state("all");
        let find = |s: &mut State, q: &str| {
            s.query = q.into();
            s.recompute();
            let mut ids = shown_ids(s);
            ids.sort();
            ids
        };
        assert_eq!(find(&mut s, "2023"), ["b"]);
        assert_eq!(find(&mut s, "2023-2024"), ["a", "b", "c"]);
        assert_eq!(find(&mut s, "2024-05"), ["a"]);
        assert_eq!(find(&mut s, "2024-06-03"), ["c"]);
        assert_eq!(find(&mut s, "before:2024"), ["b"]);
        assert_eq!(find(&mut s, "after:2024-06"), ["c"]);
        assert_eq!(find(&mut s, "this month"), ["c"]);
        assert_eq!(find(&mut s, "last month"), ["a"]);
        assert_eq!(find(&mut s, "last year"), ["b"]);
        assert_eq!(find(&mut s, "is:video"), Vec::<String>::new());
        s.lib.photos[1].is_favorite = true;
        assert_eq!(find(&mut s, "is:favorite"), ["b"]);
        assert_eq!(find(&mut s, "x100 may"), ["a"]);
        assert_eq!(find(&mut s, "/p"), ["a", "b", "c"]);
    }

    #[test]
    fn range_selects_between_cursor_and_target() {
        let mut s = state("all");
        s.select_single(0);
        s.select_range(2);
        assert_eq!(s.selected.len(), 3);
        assert_eq!(s.cursor_index(), Some(2));
        // Shift+Left from there shrinks the range back towards the anchor.
        s.move_to(1, true);
        assert_eq!(s.selected.len(), 2);
        assert_eq!(s.cursor_index(), Some(1));
    }

    #[test]
    fn stepping_wraps() {
        let mut s = state("all");
        s.select_single(2);
        s.step_wrapping(1);
        assert_eq!(s.cursor_index(), Some(0));
        s.step_wrapping(-1);
        assert_eq!(s.cursor_index(), Some(2));
    }

    #[test]
    fn duplicates_show_as_groups() {
        let mut s = state("all");
        s.section = "duplicates".into();
        s.recompute();
        assert!(s.shown.is_empty());
        s.duplicates = Some(vec![vec!["a".into(), "c".into()]]);
        s.recompute();
        assert_eq!(shown_ids(&s), ["a", "c"]);
        assert_eq!(s.spans.len(), 1);
        assert_eq!(s.count("duplicates"), Some(2));
    }
}

#[cfg(test)]
mod favorites_flow {
    use super::*;

    #[test]
    fn a_favourited_photo_shows_in_the_favorites_section() {
        let dir = std::env::temp_dir().join(format!("nyx-photos-fav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: only this test touches the environment, before anything reads it.
        unsafe { std::env::set_var("XDG_DATA_HOME", &dir) };
        let mut s = State::new(Library::default(), "all".into());
        let make = |id: &str| {
            let mut p = tests::photo(id, &format!("{id}.jpg"), 2024, 5, 1);
            p.is_favorite = false;
            p
        };
        s.lib.set_photos(vec![make("a"), make("b"), make("c")]);
        s.recompute();
        assert_eq!(s.count("favorites"), Some(0));
        assert!(s.lib.toggle_favorite("b").unwrap());
        assert_eq!(s.count("favorites"), Some(1));
        s.section = "favorites".into();
        s.recompute();
        assert_eq!(s.shown.len(), 1, "the favourite is listed");
        assert_eq!(s.photo(0).unwrap().id, "b");
        // Deleting a photo takes it out of the library and its favourites.
        s.lib.remove_photos(&["b".to_string()]).unwrap();
        s.recompute();
        assert_eq!(s.lib.photos.len(), 2);
        assert_eq!(s.count("favorites"), Some(0));
        std::fs::remove_dir_all(&dir).ok();
    }
}
