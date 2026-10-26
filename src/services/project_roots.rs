//! User-starred project "root" folders: projects whose path lies under a root
//! are folded into it for the Projects tab. A display preference only — the
//! cached per-project data is never rewritten.

use std::collections::HashMap;

use crate::services::Aggregator;
use crate::types::{DailySummary, ProjectUsage, NO_PROJECT};

/// Star `path` as a root, or unstar it if already starred. Returns whether it
/// is a root afterwards.
pub fn toggle(roots: &mut Vec<String>, path: &str) -> bool {
    let original_len = roots.len();
    roots.retain(|root| root != path);
    if roots.len() != original_len {
        return false;
    }
    roots.push(path.to_string());
    true
}

/// The deepest root that `key` equals or lies under, if any.
pub fn root_for<'a>(key: &str, roots: &'a [String]) -> Option<&'a str> {
    if roots.is_empty() || key == NO_PROJECT {
        return None;
    }
    let key = normalize(key);
    roots
        .iter()
        .map(|root| (root, normalize(root)))
        .filter(|(_, root)| {
            key == *root
                || key
                    .strip_prefix(root.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|(_, root)| root.len())
        .map(|(root, _)| root.as_str())
}

/// Comparable form of a path: `/` separators, no trailing separator, and
/// lowercased for Windows-style paths (drive letter or backslash), whose
/// filesystems are case-insensitive.
fn normalize(path: &str) -> String {
    let windows = path.contains('\\') || path.as_bytes().get(1) == Some(&b':');
    let unified = path.replace('\\', "/");
    let trimmed = unified.trim_end_matches('/');
    if windows {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Key a project is shown under: its root, or itself when not under one.
fn display_key<'a>(key: &'a str, roots: &'a [String]) -> &'a str {
    root_for(key, roots).unwrap_or(key)
}

/// Re-key a rolled-up project map onto roots, merging folded projects.
pub fn collapse_usage(
    map: &HashMap<String, ProjectUsage>,
    roots: &[String],
) -> HashMap<String, ProjectUsage> {
    if roots.is_empty() {
        return map.clone();
    }
    let mut out: HashMap<String, ProjectUsage> = HashMap::new();
    for (key, usage) in map {
        out.entry(display_key(key, roots).to_string())
            .or_default()
            .merge(usage);
    }
    out
}

/// Re-key per-project daily summaries onto roots, merging same-date days.
pub fn collapse_daily(
    map: &HashMap<String, Vec<DailySummary>>,
    roots: &[String],
) -> HashMap<String, Vec<DailySummary>> {
    if roots.is_empty() {
        return map.clone();
    }
    let mut grouped: HashMap<String, Vec<DailySummary>> = HashMap::new();
    for (key, days) in map {
        grouped
            .entry(display_key(key, roots).to_string())
            .or_default()
            .extend(days.iter().cloned());
    }
    grouped
        .into_iter()
        .map(|(key, days)| (key, Aggregator::merge_by_date(days)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn roots(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    fn usage(input: u64, cost: f64) -> ProjectUsage {
        ProjectUsage {
            input_tokens: input,
            cost_usd: cost,
            count: 1,
            ..Default::default()
        }
    }

    fn day(d: u32, input: u64) -> DailySummary {
        DailySummary {
            date: NaiveDate::from_ymd_opt(2026, 1, d).unwrap(),
            total_input_tokens: input,
            total_output_tokens: 0,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
            total_reasoning_tokens: 0,
            total_cache_creation_5m_tokens: 0,
            total_cache_creation_1h_tokens: 0,
            total_web_search_requests: 0,
            total_cost_usd: 0.0,
            models: HashMap::new(),
            projects: HashMap::new(),
        }
    }

    #[test]
    fn root_for_matches_itself_and_descendants() {
        let r = roots(&["/w/repo"]);
        assert_eq!(root_for("/w/repo", &r), Some("/w/repo"));
        assert_eq!(root_for("/w/repo/a", &r), Some("/w/repo"));
        assert_eq!(root_for("/w/repo/a/b", &r), Some("/w/repo"));
        assert_eq!(root_for("/w/other", &r), None);
    }

    #[test]
    fn root_for_respects_component_boundary() {
        let r = roots(&["/w/repo"]);
        assert_eq!(root_for("/w/repo-two", &r), None);
        assert_eq!(root_for("/w/rep", &r), None);
    }

    #[test]
    fn root_for_deepest_root_wins() {
        let r = roots(&["/w", "/w/big"]);
        assert_eq!(root_for("/w/big/sub", &r), Some("/w/big"));
        assert_eq!(root_for("/w/small", &r), Some("/w"));
    }

    #[test]
    fn root_for_ignores_trailing_separator() {
        let r = roots(&["/w/repo/"]);
        assert_eq!(root_for("/w/repo", &r), Some("/w/repo/"));
        assert_eq!(root_for("/w/repo/a", &r), Some("/w/repo/"));
    }

    #[test]
    fn root_for_windows_paths_are_case_and_separator_insensitive() {
        let r = roots(&["G:\\Work\\Repo"]);
        assert_eq!(root_for("g:\\work\\repo\\sub", &r), Some("G:\\Work\\Repo"));
        assert_eq!(root_for("G:/Work/Repo/sub", &r), Some("G:\\Work\\Repo"));
    }

    #[test]
    fn root_for_unix_paths_stay_case_sensitive() {
        let r = roots(&["/w/Repo"]);
        assert_eq!(root_for("/w/repo/sub", &r), None);
    }

    #[test]
    fn root_for_filesystem_root() {
        let r = roots(&["/"]);
        assert_eq!(root_for("/w/repo", &r), Some("/"));
    }

    #[test]
    fn root_for_windows_drive_root_unc_and_trailing_backslash() {
        let r = roots(&[r"C:\"]);
        assert_eq!(root_for(r"C:\work\repo", &r), Some(r"C:\"));

        let r = roots(&[r"\\srv\share"]);
        assert_eq!(root_for(r"\\srv\share\x", &r), Some(r"\\srv\share"));
        assert_eq!(root_for(r"\\srv\shared", &r), None);

        let r = roots(&[r"G:\work\"]);
        assert_eq!(root_for("G:/work/a", &r), Some(r"G:\work\"));
    }

    #[test]
    fn root_for_never_matches_no_project() {
        let r = roots(&[NO_PROJECT]);
        assert_eq!(root_for(NO_PROJECT, &r), None);
    }

    #[test]
    fn collapse_usage_merges_descendants_into_root() {
        let mut map = HashMap::new();
        map.insert("/w/repo/a".to_string(), usage(10, 0.1));
        map.insert("/w/repo/b".to_string(), usage(20, 0.2));
        map.insert("/w/other".to_string(), usage(5, 0.05));

        let out = collapse_usage(&map, &roots(&["/w/repo"]));
        assert_eq!(out.len(), 2);
        let root = out.get("/w/repo").unwrap();
        assert_eq!(root.input_tokens, 30);
        assert_eq!(root.count, 2);
        assert!((root.cost_usd - 0.3).abs() < 1e-9);
        assert_eq!(out.get("/w/other").unwrap().input_tokens, 5);
    }

    #[test]
    fn collapse_daily_keeps_per_model_breakdown() {
        let model = |input| crate::types::ModelUsage {
            input_tokens: input,
            ..Default::default()
        };
        let mut a = day(1, 10);
        a.models.insert("opus".to_string(), model(10));
        let mut b = day(1, 5);
        b.models.insert("opus".to_string(), model(3));
        b.models.insert("sonnet".to_string(), model(2));
        let mut map = HashMap::new();
        map.insert("/w/repo/a".to_string(), vec![a]);
        map.insert("/w/repo/b".to_string(), vec![b]);

        let out = collapse_daily(&map, &roots(&["/w/repo"]));
        let models = &out.get("/w/repo").unwrap()[0].models;
        assert_eq!(models.get("opus").unwrap().input_tokens, 13);
        assert_eq!(models.get("sonnet").unwrap().input_tokens, 2);
    }

    #[test]
    fn collapse_usage_without_roots_is_identity() {
        let mut map = HashMap::new();
        map.insert("/w/a".to_string(), usage(10, 0.1));
        let out = collapse_usage(&map, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("/w/a").unwrap().input_tokens, 10);
    }

    #[test]
    fn collapse_daily_merges_same_date_and_sorts() {
        let mut map = HashMap::new();
        map.insert("/w/repo/a".to_string(), vec![day(1, 10), day(3, 30)]);
        map.insert("/w/repo/b".to_string(), vec![day(1, 5), day(2, 20)]);
        map.insert("/w/other".to_string(), vec![day(1, 1)]);

        let out = collapse_daily(&map, &roots(&["/w/repo"]));
        assert_eq!(out.len(), 2);
        let days: Vec<(u32, u64)> = out
            .get("/w/repo")
            .unwrap()
            .iter()
            .map(|d| (chrono::Datelike::day(&d.date), d.total_input_tokens))
            .collect();
        assert_eq!(days, vec![(1, 15), (2, 20), (3, 30)]);
        assert_eq!(out.get("/w/other").unwrap().len(), 1);
    }
}
