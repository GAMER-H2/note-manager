use serde::Serialize;

/// Guard on the LCS table (`base.len() * other.len()`). Beyond this the
/// quadratic table costs more memory than it's worth; callers degrade to
/// "treat as a full rewrite", which for diffing means showing every line as
/// changed and for merging means declaring a conflict.
const MAX_MATRIX_CELLS: usize = 4_000_000;

#[derive(Debug, Serialize)]
pub struct DiffLine {
    /// "same" | "add" | "remove"
    pub op: &'static str,
    pub text: String,
    /// 1-based line number on the left/right side, when present.
    pub a: Option<usize>,
    pub b: Option<usize>,
}

pub struct Merge {
    pub text: String,
    /// True when both sides changed the same region differently. Sync treats
    /// this as "keep both" rather than writing conflict markers into the note.
    pub conflicted: bool,
}

fn split_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect()
}

/// Indices of lines common to both sides, as `(a_index, b_index)` pairs in
/// increasing order — a longest common subsequence.
///
/// Shared prefixes and suffixes are matched directly before building the DP
/// table, which is what keeps ordinary edits (a few lines changed in a long
/// note) cheap rather than quadratic in the whole file.
fn lcs_pairs(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();

    let mut start = 0usize;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        pairs.push((start, start));
        start += 1;
    }

    let mut end = 0usize;
    while end < a.len() - start.min(a.len())
        && end < b.len() - start.min(b.len())
        && a[a.len() - 1 - end] == b[b.len() - 1 - end]
    {
        end += 1;
    }

    let a_mid = &a[start..a.len() - end];
    let b_mid = &b[start..b.len() - end];

    if !a_mid.is_empty() && !b_mid.is_empty() {
        if a_mid.len().saturating_mul(b_mid.len()) <= MAX_MATRIX_CELLS {
            let rows = a_mid.len() + 1;
            let cols = b_mid.len() + 1;
            let mut table = vec![0u32; rows * cols];

            for i in (0..a_mid.len()).rev() {
                for j in (0..b_mid.len()).rev() {
                    table[i * cols + j] = if a_mid[i] == b_mid[j] {
                        table[(i + 1) * cols + (j + 1)] + 1
                    } else {
                        table[(i + 1) * cols + j].max(table[i * cols + (j + 1)])
                    };
                }
            }

            let (mut i, mut j) = (0usize, 0usize);
            while i < a_mid.len() && j < b_mid.len() {
                if a_mid[i] == b_mid[j] {
                    pairs.push((start + i, start + j));
                    i += 1;
                    j += 1;
                } else if table[(i + 1) * cols + j] >= table[i * cols + (j + 1)] {
                    i += 1;
                } else {
                    j += 1;
                }
            }
        }
        // Over the size guard: no middle matches, so the whole middle reads as
        // a rewrite. Correct, just coarser than ideal.
    }

    for k in 0..end {
        pairs.push((a.len() - end + k, b.len() - end + k));
    }

    pairs
}

/// Line-level diff from `before` to `after`.
pub fn diff_lines(before: &str, after: &str) -> Vec<DiffLine> {
    let a = split_lines(before);
    let b = split_lines(after);
    let pairs = lcs_pairs(&a, &b);

    let mut out = Vec::new();
    let (mut ai, mut bi) = (0usize, 0usize);

    let emit_gap = |out: &mut Vec<DiffLine>, ai: &mut usize, bi: &mut usize, ta: usize, tb: usize| {
        while *ai < ta {
            out.push(DiffLine {
                op: "remove",
                text: a[*ai].clone(),
                a: Some(*ai + 1),
                b: None,
            });
            *ai += 1;
        }
        while *bi < tb {
            out.push(DiffLine {
                op: "add",
                text: b[*bi].clone(),
                a: None,
                b: Some(*bi + 1),
            });
            *bi += 1;
        }
    };

    for (pa, pb) in &pairs {
        emit_gap(&mut out, &mut ai, &mut bi, *pa, *pb);
        out.push(DiffLine {
            op: "same",
            text: a[*pa].clone(),
            a: Some(*pa + 1),
            b: Some(*pb + 1),
        });
        ai = pa + 1;
        bi = pb + 1;
    }
    emit_gap(&mut out, &mut ai, &mut bi, a.len(), b.len());

    out
}

/// Three-way merge of two edits against their common ancestor (diff3).
///
/// Regions both sides left alone, or that only one side touched, merge
/// silently. A region both sides changed *differently* is a conflict; the
/// returned text is then only a best-effort (it favours `mine`) because the
/// caller is expected to keep both versions instead of using it.
pub fn merge3(base: &str, mine: &str, theirs: &str) -> Merge {
    let base_lines = split_lines(base);
    let mine_lines = split_lines(mine);
    let theirs_lines = split_lines(theirs);

    if mine_lines == theirs_lines {
        return Merge {
            text: mine.to_string(),
            conflicted: false,
        };
    }
    if base_lines == mine_lines {
        return Merge {
            text: theirs.to_string(),
            conflicted: false,
        };
    }
    if base_lines == theirs_lines {
        return Merge {
            text: mine.to_string(),
            conflicted: false,
        };
    }

    // Anchors are base lines that survived unchanged in *both* edits — the
    // stable ground between them is what can be reasoned about independently.
    let to_mine: std::collections::HashMap<usize, usize> =
        lcs_pairs(&base_lines, &mine_lines).into_iter().collect();
    let to_theirs: std::collections::HashMap<usize, usize> =
        lcs_pairs(&base_lines, &theirs_lines).into_iter().collect();

    let mut anchors: Vec<(usize, usize, usize)> = to_mine
        .iter()
        .filter_map(|(b, m)| to_theirs.get(b).map(|t| (*b, *m, *t)))
        .collect();
    anchors.sort_unstable();

    let mut merged: Vec<String> = Vec::new();
    let mut conflicted = false;
    let (mut pb, mut pm, mut pt) = (0usize, 0usize, 0usize);

    let steps = anchors
        .iter()
        .copied()
        .chain(std::iter::once((
            base_lines.len(),
            mine_lines.len(),
            theirs_lines.len(),
        )));

    for (bi, mi, ti) in steps {
        // Guard against anchors that don't advance monotonically in every
        // sequence (possible when the two LCS runs disagree about ordering).
        if bi < pb || mi < pm || ti < pt {
            continue;
        }

        let chunk_base = &base_lines[pb..bi];
        let chunk_mine = &mine_lines[pm..mi];
        let chunk_theirs = &theirs_lines[pt..ti];

        if chunk_mine == chunk_theirs {
            merged.extend_from_slice(chunk_mine);
        } else if chunk_base == chunk_mine {
            merged.extend_from_slice(chunk_theirs);
        } else if chunk_base == chunk_theirs {
            merged.extend_from_slice(chunk_mine);
        } else {
            conflicted = true;
            merged.extend_from_slice(chunk_mine);
        }

        if bi < base_lines.len() {
            merged.push(base_lines[bi].clone());
        }
        pb = bi + 1;
        pm = mi + 1;
        pt = ti + 1;
    }

    Merge {
        text: merged.join("\n"),
        conflicted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_edits_to_different_regions() {
        let base = "title\nalpha\nbeta\ngamma";
        let mine = "title\nALPHA\nbeta\ngamma";
        let theirs = "title\nalpha\nbeta\nGAMMA";
        let out = merge3(base, mine, theirs);
        assert!(!out.conflicted);
        assert_eq!(out.text, "title\nALPHA\nbeta\nGAMMA");
    }

    #[test]
    fn flags_same_region_changed_differently() {
        let base = "title\nalpha";
        let mine = "title\none";
        let theirs = "title\ntwo";
        assert!(merge3(base, mine, theirs).conflicted);
    }

    #[test]
    fn identical_edits_are_not_a_conflict() {
        let base = "a\nb";
        let both = "a\nc";
        let out = merge3(base, both, both);
        assert!(!out.conflicted);
        assert_eq!(out.text, both);
    }

    #[test]
    fn one_sided_change_wins() {
        let base = "a\nb\nc";
        let mine = "a\nb\nc";
        let theirs = "a\nB\nc";
        let out = merge3(base, mine, theirs);
        assert!(!out.conflicted);
        assert_eq!(out.text, theirs);
    }

    #[test]
    fn merges_pure_insertions_from_both_sides() {
        let base = "a\nb";
        let mine = "a\nmine\nb";
        let theirs = "a\nb\ntheirs";
        let out = merge3(base, mine, theirs);
        assert!(!out.conflicted);
        assert_eq!(out.text, "a\nmine\nb\ntheirs");
    }

    #[test]
    fn diff_reports_added_and_removed_lines() {
        let d = diff_lines("a\nb\nc", "a\nx\nc");
        let ops: Vec<_> = d.iter().map(|l| l.op).collect();
        assert_eq!(ops, vec!["same", "remove", "add", "same"]);
    }

    #[test]
    fn diff_of_identical_text_is_all_same() {
        let d = diff_lines("a\nb", "a\nb");
        assert!(d.iter().all(|l| l.op == "same"));
    }
}
