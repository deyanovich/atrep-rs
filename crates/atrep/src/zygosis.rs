//! Zygosis (spec v0.12): weaving witness kanons of one dialektos
//! along a shared milestone scheme into the zygoma — a document
//! of milestone-headed segments in the merged coordinate order,
//! each witness's slice riding a `zyg-<id>`-tagged paradiaphane.

use crate::dendron::{Annotations, Block, Document, Inline};
use crate::error::{Error, ErrorKind, Result};
use crate::sigil;

type StropheGroup = (Option<(String, Annotations)>, Vec<crate::dendron::Strophe>);
type InlineSlice = (Option<(String, Annotations)>, Vec<Inline>);

fn zyg_err(msg: String) -> Error {
    Error::new(ErrorKind::Syntax(format!("zygosis: {msg}")))
}

/// One witness's cut: the coordinate that opens the slice
/// (`None` for the proem) with the milestone's annotations, and
/// the slice's blocks.
struct Slice {
    coord: Option<(String, Annotations)>,
    blocks: Vec<Block>,
}

struct Slicer {
    scheme: String,
    slices: Vec<Slice>,
}

impl Slicer {
    fn new(scheme: &str) -> Slicer {
        Slicer {
            scheme: scheme.to_string(),
            slices: vec![Slice {
                coord: None,
                blocks: Vec::new(),
            }],
        }
    }

    fn push(&mut self, block: Block) {
        self.slices.last_mut().unwrap().blocks.push(block);
    }

    fn cut(&mut self, value: String, ann: Annotations) {
        self.slices.push(Slice {
            coord: Some((value, ann)),
            blocks: Vec::new(),
        });
    }

    /// Feed a block, cutting it at internal milestones of the
    /// scheme into well-formed siblings.
    fn feed(&mut self, block: Block) {
        match block {
            Block::Paragraph(inlines) => {
                for (coord, part) in slice_inlines(inlines, &self.scheme) {
                    if let Some((value, ann)) = coord {
                        self.cut(value, ann);
                    }
                    if !only_ws(&part) {
                        self.push(Block::Paragraph(part));
                    }
                }
            }
            Block::Para {
                symbol,
                taxis,
                lemma,
                children,
                hypograph,
                bracket_matching,
                ann,
            } => {
                let mut sub = Slicer::new(&self.scheme);
                for child in children {
                    sub.feed(child);
                }
                // Cuts before any content belong before the
                // container: a milestone that opens a speech
                // marks where the speech begins, and the prefix
                // must stay with the text that follows.
                let mut slices = sub.slices;
                while slices.len() > 1 && slices[0].blocks.is_empty() {
                    let first = slices.remove(0);
                    if let Some((value, mann)) = first.coord {
                        self.cut(value, mann);
                    }
                }
                if let Some((value, mann)) = slices[0].coord.take() {
                    self.cut(value, mann);
                }
                let n = slices.len();
                let mut hypograph = Some(hypograph);
                for (i, slice) in slices.into_iter().enumerate() {
                    if let Some((value, mann)) = slice.coord {
                        self.cut(value, mann);
                    }
                    if i == 0 {
                        // The container itself, with its pre-cut
                        // content; a heading or speech prefix does
                        // not repeat on continuations.
                        if !(slice.blocks.is_empty() && n > 1 && ann.onym.is_none()) {
                            self.push(Block::Para {
                                symbol: symbol.clone(),
                                taxis,
                                lemma: lemma.clone(),
                                children: slice.blocks,
                                hypograph: if n == 1 {
                                    hypograph.take().unwrap_or_default()
                                } else {
                                    Vec::new()
                                },
                                bracket_matching,
                                ann: ann.clone(),
                            });
                        }
                    } else {
                        // Continuations flow at the cutting level.
                        for b in slice.blocks {
                            self.push(b);
                        }
                        if i + 1 == n
                            && let Some(h) = hypograph.take()
                            && !h.is_empty()
                        {
                            self.push(Block::Paragraph(h));
                        }
                    }
                }
            }
            Block::Stichoi {
                symbol,
                taxis,
                lemma,
                strophes,
                hypograph,
                bracket_matching,
                ann,
            } => {
                // Cut at lines opening with a scheme milestone;
                // mid-line milestones split the line.
                let mut groups: Vec<StropheGroup> = vec![(None, Vec::new())];
                for strophe in strophes {
                    let mut current: Vec<Vec<Inline>> = Vec::new();
                    for line in strophe.0 {
                        let mut run: Vec<Inline> = Vec::new();
                        for inline in line {
                            match inline {
                                Inline::Milestone { scheme, value, ann }
                                    if scheme == self.scheme =>
                                {
                                    if !only_ws(&run) {
                                        current.push(std::mem::take(&mut run));
                                    } else {
                                        run.clear();
                                    }
                                    if !current.is_empty() {
                                        groups.last_mut().unwrap().1.push(crate::dendron::Strophe(
                                            std::mem::take(&mut current),
                                        ));
                                    }
                                    groups.push((Some((value, ann)), Vec::new()));
                                }
                                other => run.push(other),
                            }
                        }
                        if !only_ws(&run) {
                            current.push(run);
                        }
                    }
                    if !current.is_empty() {
                        groups
                            .last_mut()
                            .unwrap()
                            .1
                            .push(crate::dendron::Strophe(current));
                    }
                }
                let n = groups.len();
                for (i, (coord, strophes)) in groups.into_iter().enumerate() {
                    if let Some((value, mann)) = coord {
                        self.cut(value, mann);
                    }
                    if strophes.is_empty() {
                        continue;
                    }
                    self.push(Block::Stichoi {
                        symbol: symbol.clone(),
                        taxis,
                        lemma: lemma.clone(),
                        strophes,
                        hypograph: if i + 1 == n {
                            hypograph.clone()
                        } else {
                            Vec::new()
                        },
                        bracket_matching,
                        ann: if i == 0 {
                            ann.clone()
                        } else {
                            Annotations {
                                onym: None,
                                genoses: ann.genoses.clone(),
                            }
                        },
                    });
                }
            }
            Block::ParaDiaphane { children, ann } => {
                let mut sub = Slicer::new(&self.scheme);
                for child in children {
                    sub.feed(child);
                }
                let n = sub.slices.len();
                for (i, slice) in sub.slices.into_iter().enumerate() {
                    if let Some((value, mann)) = slice.coord {
                        self.cut(value, mann);
                    }
                    if slice.blocks.is_empty() {
                        continue;
                    }
                    self.push(Block::ParaDiaphane {
                        children: slice.blocks,
                        ann: if i == 0 {
                            ann.clone()
                        } else {
                            Annotations {
                                onym: None,
                                genoses: ann.genoses.clone(),
                            }
                        },
                    });
                }
                let _ = n;
            }
            other => self.push(other),
        }
    }
}

/// Split an inline run at scheme milestones, cutting through
/// nested endos (a quotation spanning a page boundary closes
/// and reopens across the cut; annotation onyms stay on the
/// first part).
fn slice_inlines(inlines: Vec<Inline>, scheme: &str) -> Vec<InlineSlice> {
    let mut out: Vec<InlineSlice> = vec![(None, Vec::new())];
    for inline in inlines {
        match inline {
            Inline::Milestone {
                scheme: s,
                value,
                ann,
            } if s == scheme => {
                out.push((Some((value, ann)), Vec::new()));
            }
            Inline::Endo {
                symbol,
                content,
                bracket_matching,
                ann,
            } => {
                let parts = slice_inlines(content, scheme);
                let n = parts.len();
                for (i, (coord, part)) in parts.into_iter().enumerate() {
                    if let Some(c) = coord {
                        out.push((Some(c), Vec::new()));
                    }
                    if part.is_empty() && n > 1 {
                        continue;
                    }
                    out.last_mut().unwrap().1.push(Inline::Endo {
                        symbol: symbol.clone(),
                        content: part,
                        bracket_matching,
                        ann: if i == 0 {
                            ann.clone()
                        } else {
                            Annotations {
                                onym: None,
                                genoses: ann.genoses.clone(),
                            }
                        },
                    });
                }
            }
            other => out.last_mut().unwrap().1.push(other),
        }
    }
    out
}

fn only_ws(inlines: &[Inline]) -> bool {
    inlines
        .iter()
        .all(|i| matches!(i, Inline::Text(t) if t.trim().is_empty()))
}

/// Weave witnesses along a shared scheme into the zygoma.
pub fn zygosis(witnesses: &[(String, Document)], scheme: &str) -> Result<Document> {
    zygosis_split(witnesses, scheme, None)
}

/// [`zygosis`] with segment subdivision: when `split` is given,
/// any coordinate whose largest witness slice exceeds that many
/// text characters is recursively halved — every witness's slice
/// is cut at the sentence boundary nearest its midpoint (at the
/// same sentence index when all witnesses count the same number
/// of sentences, the exact cut for sentence-parallel
/// translations; proportionally otherwise, the Gale–Church
/// length-correlation assumption) — until every sub-slice fits.
/// Continuation sub-slices carry no milestone: coordinates are
/// citable identity and are never synthesized; the subdivision
/// is presentation-grade grouping that exists only in the woven
/// zygoma, never in the witness sources.
pub fn zygosis_split(
    witnesses: &[(String, Document)],
    scheme: &str,
    split: Option<usize>,
) -> Result<Document> {
    if witnesses.len() < 2 {
        return Err(zyg_err("at least two witnesses are required".into()));
    }
    let dialect = witnesses[0].1.dialect_id.clone();
    let mut ids = std::collections::HashSet::new();
    for (id, doc) in witnesses {
        if doc.dialect_id != dialect {
            return Err(zyg_err(format!(
                "witness `{id}` is `{}`, expected `{dialect}` (one dialektos per weave)",
                doc.dialect_id
            )));
        }
        if !sigil::is_valid_genos(&format!("zyg-{id}")) {
            return Err(zyg_err(format!(
                "witness id `{id}` is not a valid identifier"
            )));
        }
        if !ids.insert(id.clone()) {
            return Err(zyg_err(format!("duplicate witness id `{id}`")));
        }
    }

    // Slice every witness.
    let mut sliced: Vec<(String, Vec<Slice>)> = Vec::new();
    for (id, doc) in witnesses {
        let mut slicer = Slicer::new(scheme);
        for block in doc.blocks.clone() {
            slicer.feed(block);
        }
        sliced.push((id.clone(), slicer.slices));
    }

    // Traces and order-consistent merge with witness precedence.
    let traces: Vec<Vec<String>> = sliced
        .iter()
        .map(|(_, slices)| {
            slices
                .iter()
                .filter_map(|s| s.coord.as_ref().map(|(v, _)| v.clone()))
                .collect()
        })
        .collect();
    let mut pointers = vec![0usize; traces.len()];
    let positions: Vec<std::collections::HashMap<&str, usize>> = traces
        .iter()
        .map(|t| t.iter().enumerate().map(|(i, v)| (v.as_str(), i)).collect())
        .collect();
    let total: usize = traces
        .iter()
        .flat_map(|t| t.iter())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let mut order: Vec<String> = Vec::new();
    while order.len() < total {
        let mut emitted = false;
        'witness: for (i, trace) in traces.iter().enumerate() {
            if pointers[i] >= trace.len() {
                continue;
            }
            let cand = &trace[pointers[i]];
            for (j, pos) in positions.iter().enumerate() {
                if let Some(&p) = pos.get(cand.as_str())
                    && p != pointers[j]
                {
                    continue 'witness;
                }
            }
            // Emittable: advance every witness that has it.
            for (j, pos) in positions.iter().enumerate() {
                if pos.contains_key(cand.as_str()) {
                    pointers[j] += 1;
                }
            }
            order.push(cand.clone());
            emitted = true;
            break;
        }
        if !emitted {
            let heads: Vec<String> = traces
                .iter()
                .enumerate()
                .filter_map(|(i, t)| t.get(pointers[i]).cloned())
                .collect();
            return Err(zyg_err(format!(
                "witnesses disagree on coordinate order near {heads:?}"
            )));
        }
    }

    // Assemble the zygoma.
    let tag = |id: &str, blocks: Vec<Block>| Block::ParaDiaphane {
        children: blocks,
        ann: Annotations {
            onym: None,
            genoses: vec![format!("zyg-{id}")],
        },
    };
    let mut out: Vec<Block> = Vec::new();
    // Proems.
    for (id, slices) in &sliced {
        if let Some(proem) = slices.iter().find(|s| s.coord.is_none())
            && !proem.blocks.is_empty()
        {
            out.push(tag(id, proem.blocks.clone()));
        }
    }
    for coord in &order {
        // The milestone head, annotated from the first witness
        // that carries it.
        let ann = sliced
            .iter()
            .flat_map(|(_, slices)| slices.iter())
            .find_map(|s| match &s.coord {
                Some((v, a)) if v == coord => Some(a.clone()),
                _ => None,
            })
            .unwrap_or_default();
        out.push(Block::Paragraph(vec![Inline::Milestone {
            scheme: scheme.to_string(),
            value: coord.clone(),
            ann,
        }]));
        let parts: Vec<(String, Vec<Block>)> = sliced
            .iter()
            .filter_map(|(id, slices)| {
                slices
                    .iter()
                    .find(|s| matches!(&s.coord, Some((v, _)) if v == coord))
                    .filter(|s| !s.blocks.is_empty())
                    .map(|s| (id.clone(), s.blocks.clone()))
            })
            .collect();
        let rounds = match split {
            Some(threshold) => subdivide(parts, threshold),
            None => vec![parts],
        };
        for round in rounds {
            for (id, blocks) in round {
                out.push(tag(&id, blocks));
            }
        }
    }
    Ok(Document {
        dialect_id: dialect,
        dialect_version: witnesses[0].1.dialect_version.clone(),
        blocks: out,
    })
}

// -------------------------------------------------------------------
// Segment subdivision: recursive proportional halving of
// oversized coordinate slices. Presentation-grade only — the
// sub-slices carry no coordinates.
// -------------------------------------------------------------------

/// Sentence-final punctuation, Greek included (ano teleia, the
/// middle dot some digitizations use for it, and the Greek
/// question mark alongside the semicolon that usually encodes
/// it).
const SENTENCE_ENDS: &[char] = &['.', '!', '?', '…', ';', '\u{00B7}', '\u{0387}', '\u{037E}'];

fn inline_chars(inline: &Inline) -> usize {
    match inline {
        Inline::Text(t) => t.chars().count(),
        Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
            content.iter().map(inline_chars).sum()
        }
        _ => 0,
    }
}

fn blocks_chars(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(inlines) => inlines.iter().map(inline_chars).sum(),
            Block::ParaDiaphane { children, .. } => blocks_chars(children),
            Block::Para { children, .. } => blocks_chars(children),
            _ => 0,
        })
        .sum()
}

/// A cut point inside a paragraph: the top-level inline index and
/// the char offset within that `Inline::Text`.
#[derive(Clone, Copy)]
struct Cut {
    inline: usize,
    offset: usize,
}

/// Candidate cut points after the given punctuation characters
/// (each cut lands on the first non-whitespace character that
/// follows), with the cumulative char position of each cut. Cuts
/// are only taken inside top-level text runs — never inside an
/// endo.
fn cuts_after(inlines: &[Inline], ends: &[char]) -> Vec<(Cut, usize)> {
    let mut cuts = Vec::new();
    let mut cum = 0usize;
    for (i, inline) in inlines.iter().enumerate() {
        if let Inline::Text(t) = inline {
            let chars: Vec<char> = t.chars().collect();
            let mut j = 0;
            while j < chars.len() {
                if ends.contains(&chars[j]) {
                    let mut k = j + 1;
                    while k < chars.len() && chars[k].is_whitespace() {
                        k += 1;
                    }
                    if k > j + 1 && k < chars.len() {
                        cuts.push((
                            Cut {
                                inline: i,
                                offset: k,
                            },
                            cum + k,
                        ));
                    }
                    j = k;
                } else {
                    j += 1;
                }
            }
        }
        cum += inline_chars(inline);
    }
    cuts
}

/// Split a paragraph's inlines at the cut, trimming the seam.
fn split_inlines(inlines: &[Inline], cut: Cut) -> (Vec<Inline>, Vec<Inline>) {
    let mut left: Vec<Inline> = inlines[..cut.inline].to_vec();
    let mut right: Vec<Inline> = Vec::new();
    if let Inline::Text(t) = &inlines[cut.inline] {
        let chars: Vec<char> = t.chars().collect();
        let head: String = chars[..cut.offset].iter().collect();
        let tail: String = chars[cut.offset..].iter().collect();
        let head = head.trim_end().to_string();
        let tail = tail.trim_start().to_string();
        if !head.is_empty() {
            left.push(Inline::Text(head));
        }
        if !tail.is_empty() {
            right.push(Inline::Text(tail));
        }
    }
    right.extend_from_slice(&inlines[cut.inline + 1..]);
    (left, right)
}

/// Choose this witness's cut: at `same_index` among the sentence
/// cuts when the round agreed on one, otherwise nearest the char
/// midpoint, falling back from sentence to comma to whitespace
/// boundaries.
fn choose_cut(inlines: &[Inline], same_index: Option<usize>) -> Option<Cut> {
    let total: usize = inlines.iter().map(inline_chars).sum();
    let sentence = cuts_after(inlines, SENTENCE_ENDS);
    if let Some(idx) = same_index
        && let Some((cut, _)) = sentence.get(idx)
    {
        return Some(*cut);
    }
    for tier in [
        sentence,
        cuts_after(inlines, &[',']),
        cuts_after(inlines, &[' ']),
    ] {
        if let Some((cut, _)) = tier.iter().min_by_key(|(_, pos)| pos.abs_diff(total / 2)) {
            return Some(*cut);
        }
    }
    None
}

/// Halve one witness's slice: multi-block slices split at the
/// block boundary nearest the char midpoint; single-paragraph
/// slices split inside the paragraph. Anything else (verse, a
/// lone structured sim) is unsplittable.
fn split_blocks(blocks: &[Block], same_index: Option<usize>) -> Option<(Vec<Block>, Vec<Block>)> {
    if blocks.len() > 1 {
        let total = blocks_chars(blocks);
        let mut best: Option<(usize, usize)> = None;
        let mut cum = 0usize;
        for (i, block) in blocks.iter().enumerate().take(blocks.len() - 1) {
            cum += blocks_chars(std::slice::from_ref(block));
            let dist = cum.abs_diff(total / 2);
            if best.is_none_or(|(_, d)| dist < d) {
                best = Some((i + 1, dist));
            }
        }
        let (at, _) = best?;
        return Some((blocks[..at].to_vec(), blocks[at..].to_vec()));
    }
    match blocks {
        [Block::Paragraph(inlines)] => {
            let cut = choose_cut(inlines, same_index)?;
            let (l, r) = split_inlines(inlines, cut);
            if l.is_empty() || r.is_empty() {
                return None;
            }
            Some((vec![Block::Paragraph(l)], vec![Block::Paragraph(r)]))
        }
        _ => None,
    }
}

/// Recursively halve a coordinate's witness slices until every
/// one fits the threshold. If any witness's slice cannot be
/// split, the round is emitted whole — subdivision degrades
/// gracefully, it never fails the weave.
fn subdivide(parts: Vec<(String, Vec<Block>)>, threshold: usize) -> Vec<Vec<(String, Vec<Block>)>> {
    let widest = parts
        .iter()
        .map(|(_, blocks)| blocks_chars(blocks))
        .max()
        .unwrap_or(0);
    if widest <= threshold || parts.is_empty() {
        return vec![parts];
    }
    // When every witness is a single paragraph with the same
    // number of sentence cuts, cut all at the middle index — the
    // exact alignment for sentence-parallel translations.
    let counts: Vec<Option<usize>> = parts
        .iter()
        .map(|(_, blocks)| match blocks.as_slice() {
            [Block::Paragraph(inlines)] => Some(cuts_after(inlines, SENTENCE_ENDS).len()),
            _ => None,
        })
        .collect();
    let same_index = match counts.as_slice() {
        [Some(first), rest @ ..] if *first > 0 && rest.iter().all(|c| *c == Some(*first)) => {
            Some(first / 2)
        }
        _ => None,
    };
    let mut lefts: Vec<(String, Vec<Block>)> = Vec::new();
    let mut rights: Vec<(String, Vec<Block>)> = Vec::new();
    for (id, blocks) in &parts {
        match split_blocks(blocks, same_index) {
            Some((l, r)) => {
                lefts.push((id.clone(), l));
                rights.push((id.clone(), r));
            }
            None => return vec![parts],
        }
    }
    let mut rounds = subdivide(lefts, threshold);
    rounds.extend(subdivide(rights, threshold));
    rounds
}

// -------------------------------------------------------------------
// Quasialign: in-file subdivision. Where zygo --split subdivides
// inside the woven zygoma, quasialign writes the cuts into the
// witness documents themselves as quasi-milestones — derived,
// non-citable coordinates named by the opening anchor plus a
// binary path (`17a|1`, then `17a|0.1` / `17a|1.1` in the
// halves), each carrying the `quasi` genos. The label depends
// only on the opening coordinate and the recursion structure, so
// it is witness-invariant even when coverage differs; the files
// stay milestone-aligned and weave with plain zygo. Cuts are
// chosen exactly as in subdivision: sentence boundary nearest
// the midpoint, same sentence index when every witness counts
// the same number of sentences, comma and whitespace as
// fallback tiers; paragraph heads count as sentence-strength
// candidates. Insertion is inline-only (top-level paragraphs);
// a segment whose midpoint machinery cannot find a cut in some
// witness is left whole.
// -------------------------------------------------------------------

/// What quasialign did: quasi-milestones inserted per document
/// (parallel to the input order), and coordinates skipped
/// because a witness already carries quasi cuts under them.
pub struct QuasialignReport {
    pub inserted: Vec<usize>,
    pub skipped: Vec<String>,
}

/// A candidate insertion point in one document: the path to the
/// target paragraph, inline index within that paragraph, char
/// offset within that text inline (offset 0 = before the
/// inline), and the cumulative char position within the segment.
#[derive(Clone)]
struct Site {
    /// Child-index path from the document root to the target
    /// paragraph: `[block]` for a top-level paragraph, or
    /// `[block, child, …]` descending through nested
    /// para-simmeres / para-diaphanes — quasialign descends into
    /// enclosing divisions so chapter-wrapped prose still cuts.
    path: Vec<usize>,
    inline: usize,
    offset: usize,
    pos: usize,
}

/// One document's view of one real coordinate's segment.
struct SegView {
    len: usize,
    /// Paragraph-head sites — preferred cut tier: a milestone
    /// placed here opens a paragraph (and thus a line).
    head: Vec<Site>,
    sentence: Vec<Site>,
    clause: Vec<Site>,
    word: Vec<Site>,
    has_quasi: bool,
}

/// Char offsets of cut points after any of `ends` (landing on
/// the first following non-whitespace character, interior only).
fn text_cut_offsets(t: &str, ends: &[char]) -> Vec<usize> {
    let chars: Vec<char> = t.chars().collect();
    let mut offsets = Vec::new();
    let mut j = 0;
    while j < chars.len() {
        if ends.contains(&chars[j]) {
            let mut k = j + 1;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if k > j + 1 && k < chars.len() {
                offsets.push(k);
            }
            j = k;
        } else {
            j += 1;
        }
    }
    offsets
}

/// Build every document's per-coordinate segment views. Only
/// paragraphs contribute candidate sites; container blocks
/// contribute length alone. Without a witness prefix the walk
/// covers top-level content; with one (`--prefix zyg-grc`) it
/// covers only paradiaphanes carrying a matching genos — the
/// chosen witness's stream in a multi-witness file — while
/// top-level milestones still delimit segments.
fn segment_views(doc: &Document, scheme: &str, prefix: Option<&str>) -> Vec<(String, SegView)> {
    let mut w = ViewWalker {
        scheme,
        views: Vec::new(),
        current: None,
    };
    if let Some(p) = prefix {
        // Witness-scoped (zygoma): the original flat walk — the
        // witness's stream is exactly the matched paradiaphanes'
        // children; top-level paragraphs only delimit.
        for (bi, block) in doc.blocks.iter().enumerate() {
            match block {
                Block::Paragraph(inlines) => {
                    w.scan_paragraph(&[bi], inlines, false);
                }
                Block::ParaDiaphane { children, ann }
                    if ann.genoses.iter().any(|g| g.starts_with(p)) =>
                {
                    for (ci, child) in children.iter().enumerate() {
                        if let Block::Paragraph(inlines) = child {
                            w.head_candidate(&[bi, ci]);
                            w.scan_paragraph(&[bi, ci], inlines, true);
                        } else if let Some(ci_) = w.current {
                            w.views[ci_].1.len += blocks_chars(std::slice::from_ref(child));
                        }
                    }
                }
                _ => {}
            }
        }
    } else {
        // Whole-document walk: descend into enclosing
        // para-simmeres and para-diaphanes so paragraphs inside
        // divisions (chapters, scenes) are cuttable too.
        let mut path = Vec::new();
        walk_blocks(&mut w, &doc.blocks, &mut path);
    }
    w.views
}

/// Depth-first walk for the whole-document (non-witness) case:
/// paragraphs at any depth are head candidates and cuttable
/// content; enclosing blocks contribute their lemma/hypograph
/// text to the running segment length; stichoi and other leaf
/// blocks are atomic (their length counts, no cuts inside).
fn walk_blocks(w: &mut ViewWalker, blocks: &[Block], path: &mut Vec<usize>) {
    for (bi, block) in blocks.iter().enumerate() {
        path.push(bi);
        match block {
            Block::Paragraph(inlines) => {
                w.head_candidate(path);
                w.scan_paragraph(path, inlines, true);
            }
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                if let Some(ci) = w.current {
                    w.views[ci].1.len += lemma.iter().map(inline_chars).sum::<usize>();
                }
                walk_blocks(w, children, path);
                if let Some(ci) = w.current {
                    w.views[ci].1.len += hypograph.iter().map(inline_chars).sum::<usize>();
                }
            }
            Block::ParaDiaphane { children, .. } => {
                walk_blocks(w, children, path);
            }
            other => {
                if let Some(ci) = w.current {
                    w.views[ci].1.len += blocks_chars(std::slice::from_ref(other));
                }
            }
        }
        path.pop();
    }
}

struct ViewWalker<'a> {
    scheme: &'a str,
    views: Vec<(String, SegView)>,
    current: Option<usize>,
}

impl ViewWalker<'_> {
    /// A paragraph head inside a running segment is a
    /// sentence-strength candidate.
    fn head_candidate(&mut self, path: &[usize]) {
        if let Some(ci) = self.current
            && self.views[ci].1.len > 0
        {
            let site = Site {
                path: path.to_vec(),
                inline: 0,
                offset: 0,
                pos: self.views[ci].1.len,
            };
            self.views[ci].1.head.push(site);
        }
    }

    /// Scan one paragraph: milestones always track segments;
    /// text contributes length and candidate sites only when
    /// `content` (the paragraph belongs to the scanned stream).
    fn scan_paragraph(&mut self, path: &[usize], inlines: &[Inline], content: bool) {
        let blank = || SegView {
            len: 0,
            head: Vec::new(),
            sentence: Vec::new(),
            clause: Vec::new(),
            word: Vec::new(),
            has_quasi: false,
        };
        for (ii, inline) in inlines.iter().enumerate() {
            match inline {
                Inline::Milestone {
                    scheme: s, value, ..
                } if s == self.scheme
                    || (value.starts_with(&format!("{}:", self.scheme))
                        && value.contains('|')) =>
                {
                    if value.contains('|') {
                        // An existing quasi cut: its segment is
                        // already subdivided — hands off.
                        if let Some(ci) = self.current {
                            self.views[ci].1.has_quasi = true;
                        }
                    } else {
                        self.views.push((value.clone(), blank()));
                        self.current = Some(self.views.len() - 1);
                    }
                }
                Inline::Text(t) if content => {
                    if let Some(ci) = self.current {
                        let view = &mut self.views[ci].1;
                        for (tier, ends) in [
                            (0usize, SENTENCE_ENDS),
                            (1, [','].as_slice()),
                            (2, [' '].as_slice()),
                        ] {
                            for offset in text_cut_offsets(t, ends) {
                                let site = Site {
                                    path: path.to_vec(),
                                    inline: ii,
                                    offset,
                                    pos: view.len + offset,
                                };
                                match tier {
                                    0 => view.sentence.push(site),
                                    1 => view.clause.push(site),
                                    _ => view.word.push(site),
                                }
                            }
                        }
                        view.len += t.chars().count();
                    }
                }
                other => {
                    if content && let Some(ci) = self.current {
                        self.views[ci].1.len += inline_chars(other);
                    }
                }
            }
        }
    }
}

/// Choose one witness's cut inside the `(lo, hi)` window:
/// `same_index` among the window's sentence sites when the round
/// agreed on one, otherwise nearest the window midpoint across
/// the tiers.
fn window_cut(view: &SegView, lo: usize, hi: usize, same_index: Option<usize>) -> Option<Site> {
    let in_window = |sites: &[Site]| -> Vec<Site> {
        sites
            .iter()
            .cloned()
            .filter(|s| s.pos > lo && s.pos < hi)
            .collect()
    };
    let sentence = in_window(&view.sentence);
    if let Some(idx) = same_index
        && let Some(site) = sentence.get(idx)
    {
        return Some(site.clone());
    }
    let mid = lo + (hi - lo) / 2;
    for tier in [
        in_window(&view.head),
        sentence,
        in_window(&view.clause),
        in_window(&view.word),
    ] {
        if let Some(site) = tier.iter().min_by_key(|s| s.pos.abs_diff(mid)) {
            return Some(site.clone());
        }
    }
    None
}

/// Lockstep recursion: plan the cuts for one coordinate across
/// every witness that carries it. `windows` holds each witness's
/// `(lo, hi)` char window; `path` is the binary address of the
/// fragment being cut.
fn plan_cuts(
    views: &[&SegView],
    windows: &[(usize, usize)],
    max_segment: usize,
    path: &str,
    plans: &mut [Vec<(String, Site)>],
) {
    let widest = windows.iter().map(|(lo, hi)| hi - lo).max().unwrap_or(0);
    if widest <= max_segment {
        return;
    }
    let counts: Vec<usize> = views
        .iter()
        .zip(windows)
        .map(|(v, (lo, hi))| {
            v.sentence
                .iter()
                .filter(|s| s.pos > *lo && s.pos < *hi)
                .count()
        })
        .collect();
    let same_index = match counts.as_slice() {
        [first, rest @ ..] if *first > 0 && rest.iter().all(|c| c == first) => Some(first / 2),
        _ => None,
    };
    let label = if path.is_empty() {
        "1".to_string()
    } else {
        format!("{path}.1")
    };
    let mut cuts: Vec<Site> = Vec::new();
    for (view, (lo, hi)) in views.iter().zip(windows) {
        match window_cut(view, *lo, *hi, same_index) {
            Some(site) => cuts.push(site),
            // One witness without a cut leaves the whole round
            // unsplit — the labels must stay witness-invariant.
            None => return,
        }
    }
    for (plan, cut) in plans.iter_mut().zip(&cuts) {
        plan.push((label.clone(), cut.clone()));
    }
    let (lp, rp) = if path.is_empty() {
        ("0".to_string(), "1".to_string())
    } else {
        (format!("{path}.0"), format!("{path}.1"))
    };
    let lefts: Vec<(usize, usize)> = windows
        .iter()
        .zip(&cuts)
        .map(|((lo, _), c)| (*lo, c.pos))
        .collect();
    let rights: Vec<(usize, usize)> = windows
        .iter()
        .zip(&cuts)
        .map(|((_, hi), c)| (c.pos, *hi))
        .collect();
    plan_cuts(views, &lefts, max_segment, &lp, plans);
    plan_cuts(views, &rights, max_segment, &rp, plans);
}

/// Insert planned quasi-milestones into one document. All of a
/// document's cuts are applied in one descending pass, so earlier
/// sites stay valid as later ones split paragraphs (a split
/// inserts the continuation AFTER the cut point, and all
/// later-positioned cuts were already applied).
///
/// A quasi-milestone always OPENS A LINE: a paragraph-head cut
/// prepends the milestone to the paragraph; a mid-paragraph cut
/// splits the paragraph into two siblings with the milestone
/// heading the continuation.
fn apply_cuts(
    doc: &mut Document,
    scheme: &str,
    cut_ns: Option<&str>,
    mut cuts: Vec<(String, Site)>,
) {
    cuts.sort_by(|(_, a), (_, b)| {
        (&b.path, b.inline, b.offset).cmp(&(&a.path, a.inline, a.offset))
    });
    'cuts: for (value, site) in cuts {
        let last = *site.path.last().unwrap();
        let parent: &mut Vec<Block> = {
            let mut blocks = &mut doc.blocks;
            for &idx in &site.path[..site.path.len() - 1] {
                blocks = match &mut blocks[idx] {
                    Block::Para { children, .. } | Block::ParaDiaphane { children, .. } => children,
                    _ => continue 'cuts,
                };
            }
            blocks
        };
        // Positional cuts are witness-namespaced (e.g.
        // litogram:ch:1|0.1) unless the segmentation scheme IS
        // the namespace already.
        let (ms_scheme, ms_value) = match cut_ns {
            Some(ns) if ns != scheme => {
                (ns.to_string(), format!("{scheme}:{value}"))
            }
            _ => (scheme.to_string(), value),
        };
        let ms = Inline::Milestone {
            scheme: ms_scheme,
            value: ms_value,
            ann: Annotations {
                onym: None,
                genoses: Vec::new(),
            },
        };
        let Block::Paragraph(inlines) = &mut parent[last] else {
            continue;
        };
        if site.inline == 0 && site.offset == 0 {
            inlines.insert(0, ms);
            continue;
        }
        let tail: Vec<Inline> = if site.offset == 0 {
            inlines.split_off(site.inline)
        } else if let Inline::Text(t) = &inlines[site.inline] {
            let chars: Vec<char> = t.chars().collect();
            let head_txt: String = chars[..site.offset].iter().collect();
            let tail_txt: String = chars[site.offset..].iter().collect();
            let mut rest = inlines.split_off(site.inline + 1);
            inlines[site.inline] = Inline::Text(head_txt.trim_end().to_string());
            rest.insert(0, Inline::Text(tail_txt));
            rest
        } else {
            continue;
        };
        let mut second = vec![ms];
        second.extend(tail);
        parent.insert(last + 1, Block::Paragraph(second));
    }
}

/// Quasialign a set of documents sharing a milestone scheme:
/// insert quasi-milestones at statistical midpoints so that no
/// segment exceeds `max_segment` characters in any document,
/// leaving the files milestone-aligned for a plain weave.
pub fn quasialign(
    docs: &mut [Document],
    scheme: &str,
    max_segment: usize,
    prefix: Option<&str>,
    cut_prefix: Option<&str>,
) -> Result<QuasialignReport> {
    if max_segment == 0 {
        return Err(zyg_err("--max-segment must be positive".into()));
    }
    let all_views: Vec<Vec<(String, SegView)>> = docs
        .iter()
        .map(|d| segment_views(d, scheme, prefix))
        .collect();
    // The union of real coordinates in first-seen order.
    let mut order: Vec<String> = Vec::new();
    for views in &all_views {
        for (coord, _) in views {
            if !order.contains(coord) {
                order.push(coord.clone());
            }
        }
    }
    let mut skipped: Vec<String> = Vec::new();
    let mut all_cuts: Vec<Vec<(String, Site)>> = vec![Vec::new(); docs.len()];
    for coord in &order {
        let holders: Vec<usize> = all_views
            .iter()
            .enumerate()
            .filter(|(_, views)| views.iter().any(|(c, _)| c == coord))
            .map(|(i, _)| i)
            .collect();
        let views: Vec<&SegView> = holders
            .iter()
            .map(|&i| &all_views[i].iter().find(|(c, _)| c == coord).unwrap().1)
            .collect();
        if views.iter().any(|v| v.has_quasi) {
            skipped.push(coord.clone());
            continue;
        }
        let windows: Vec<(usize, usize)> = views.iter().map(|v| (0, v.len)).collect();
        let mut plans: Vec<Vec<(String, Site)>> = vec![Vec::new(); holders.len()];
        plan_cuts(&views, &windows, max_segment, "", &mut plans);
        for (slot, plan) in holders.iter().zip(plans) {
            all_cuts[*slot].extend(
                plan.into_iter()
                    .map(|(label, site)| (format!("{coord}|{label}"), site)),
            );
        }
    }
    let inserted: Vec<usize> = all_cuts.iter().map(Vec::len).collect();
    let cut_ns = cut_prefix.map(|p| p.trim_end_matches(':'));
    for (doc, cuts) in docs.iter_mut().zip(all_cuts) {
        apply_cuts(doc, scheme, cut_ns, cuts);
    }
    Ok(QuasialignReport { inserted, skipped })
}

// -------------------------------------------------------------------
// Collation: deriving an apparatus criticus from the weave. The
// witnesses are sliced exactly as for zygosis; each non-base
// witness's segment is word-diffed against the base's, and every
// divergence becomes a manuscript-note (`^!`) anchored by deixis
// after the last word of its lemma in the base text. The output
// is the base document itself, apparatus threaded through it.
// -------------------------------------------------------------------

/// One divergence: base words `[start, end)` against the
/// witness's replacement (empty = omission; `start == end` =
/// addition).
struct Hunk {
    start: usize,
    end: usize,
    variant: Vec<String>,
}

/// Word-level LCS diff.
fn diff_words(a: &[String], b: &[String]) -> Vec<Hunk> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut hunks = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        let start = i;
        let mut variant = Vec::new();
        loop {
            if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                i += 1;
            } else if j < m {
                variant.push(b[j].clone());
                j += 1;
            }
            if (i == n && j == m) || (i < n && j < m && a[i] == b[j]) {
                break;
            }
        }
        hunks.push(Hunk {
            start,
            end: i,
            variant,
        });
    }
    hunks
}

fn words_of_inlines(inlines: &[Inline], out: &mut Vec<String>) {
    for inl in inlines {
        match inl {
            Inline::Text(t) => out.extend(t.split_whitespace().map(str::to_string)),
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                words_of_inlines(content, out)
            }
            _ => {}
        }
    }
}

fn words_of_blocks(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(inl) => words_of_inlines(inl, out),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                words_of_inlines(lemma, out);
                words_of_blocks(children, out);
                words_of_inlines(hypograph, out);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                words_of_inlines(lemma, out);
                for s in strophes {
                    for line in &s.0 {
                        words_of_inlines(line, out);
                    }
                }
                words_of_inlines(hypograph, out);
            }
            Block::ParaDiaphane { children, .. } => words_of_blocks(children, out),
            _ => {}
        }
    }
}

/// Threads deixes through the base text: words are counted per
/// segment in traversal order, and when a pending anchor's
/// ordinal is reached the deixis is emitted right after that
/// word, its note body queued for the end of the enclosing
/// top-level block.
/// coord -> queue of (anchor word ordinal, onym, entry text),
/// sorted by anchor.
type Pending =
    std::collections::HashMap<String, std::collections::VecDeque<(usize, String, String)>>;

struct Inserter {
    scheme: String,
    coord: Option<String>,
    word: usize,
    pending: Pending,
    ready: Vec<Block>,
}

impl Inserter {
    fn note(onym: &str, text: &str) -> Block {
        Block::Para {
            symbol: "^!".into(),
            taxis: None,
            lemma: Vec::new(),
            children: vec![Block::Paragraph(vec![Inline::Text(text.to_string())])],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations {
                onym: Some(onym.to_string()),
                genoses: Vec::new(),
            },
        }
    }

    fn split_text(&mut self, t: String, out: &mut Vec<Inline>) {
        let Some(coord) = self.coord.clone() else {
            out.push(Inline::Text(t));
            return;
        };
        let mut spans: Vec<usize> = Vec::new();
        let mut in_word = false;
        for (i, c) in t.char_indices() {
            if c.is_whitespace() {
                if in_word {
                    spans.push(i);
                    in_word = false;
                }
            } else {
                in_word = true;
            }
        }
        if in_word {
            spans.push(t.len());
        }
        let mut last = 0usize;
        for end in spans {
            self.word += 1;
            let mut hit = false;
            if let Some(queue) = self.pending.get_mut(&coord) {
                while queue.front().is_some_and(|(a, _, _)| *a == self.word) {
                    let (_, onym, text) = queue.pop_front().unwrap();
                    if !hit {
                        out.push(Inline::Text(t[last..end].to_string()));
                        last = end;
                        hit = true;
                    }
                    out.push(Inline::Deixis {
                        symbol: "^!".into(),
                        onym: onym.clone(),
                        ann: Annotations {
                            onym: None,
                            genoses: Vec::new(),
                        },
                    });
                    self.ready.push(Self::note(&onym, &text));
                }
            }
        }
        if last == 0 {
            out.push(Inline::Text(t));
        } else if last < t.len() {
            out.push(Inline::Text(t[last..].to_string()));
        }
    }

    fn walk_inlines(&mut self, inlines: Vec<Inline>) -> Vec<Inline> {
        let mut out = Vec::new();
        for inl in inlines {
            match inl {
                Inline::Milestone { scheme, value, ann } => {
                    if scheme == self.scheme {
                        self.coord = Some(value.clone());
                        self.word = 0;
                    }
                    out.push(Inline::Milestone { scheme, value, ann });
                }
                Inline::Text(t) => self.split_text(t, &mut out),
                Inline::Endo {
                    symbol,
                    content,
                    bracket_matching,
                    ann,
                } => {
                    let content = self.walk_inlines(content);
                    out.push(Inline::Endo {
                        symbol,
                        content,
                        bracket_matching,
                        ann,
                    });
                }
                Inline::EndoDiaphane { content, ann } => {
                    let content = self.walk_inlines(content);
                    out.push(Inline::EndoDiaphane { content, ann });
                }
                other => out.push(other),
            }
        }
        out
    }

    fn walk_block(&mut self, b: Block) -> Block {
        match b {
            Block::Paragraph(inl) => Block::Paragraph(self.walk_inlines(inl)),
            Block::Para {
                symbol,
                taxis,
                lemma,
                children,
                hypograph,
                bracket_matching,
                ann,
            } => {
                let lemma = self.walk_inlines(lemma);
                // Drain queued notes after each child so the
                // apparatus entry stays on its anchor's page.
                let mut walked = Vec::new();
                for c in children {
                    let w = self.walk_block(c);
                    walked.push(w);
                    walked.append(&mut self.ready);
                }
                let children = walked;
                let hypograph = self.walk_inlines(hypograph);
                Block::Para {
                    symbol,
                    taxis,
                    lemma,
                    children,
                    hypograph,
                    bracket_matching,
                    ann,
                }
            }
            Block::Stichoi {
                symbol,
                taxis,
                lemma,
                strophes,
                hypograph,
                bracket_matching,
                ann,
            } => {
                let lemma = self.walk_inlines(lemma);
                let strophes = strophes
                    .into_iter()
                    .map(|s| {
                        crate::dendron::Strophe(
                            s.0.into_iter().map(|l| self.walk_inlines(l)).collect(),
                        )
                    })
                    .collect();
                let hypograph = self.walk_inlines(hypograph);
                Block::Stichoi {
                    symbol,
                    taxis,
                    lemma,
                    strophes,
                    hypograph,
                    bracket_matching,
                    ann,
                }
            }
            Block::ParaDiaphane { children, ann } => {
                let mut walked = Vec::new();
                for c in children {
                    let w = self.walk_block(c);
                    walked.push(w);
                    walked.append(&mut self.ready);
                }
                Block::ParaDiaphane {
                    children: walked,
                    ann,
                }
            }
            other => other,
        }
    }
}

/// Collate witnesses against `base`: slice on `scheme`, word-diff
/// every shared segment, and return the base document with the
/// derived apparatus criticus threaded through it as
/// manuscript-notes.
pub fn collation(witnesses: &[(String, Document)], scheme: &str, base: &str) -> Result<Document> {
    if witnesses.len() < 2 {
        return Err(zyg_err("at least two witnesses are required".into()));
    }
    if !witnesses.iter().any(|(id, _)| id == base) {
        return Err(zyg_err(format!(
            "base `{base}` is not among the witness ids"
        )));
    }
    let base_doc = witnesses
        .iter()
        .find(|(id, _)| id == base)
        .map(|(_, d)| d.clone())
        .unwrap();

    // Per-witness segment word lists.
    let mut base_order: Vec<String> = Vec::new();
    let mut maps: Vec<(String, std::collections::HashMap<String, Vec<String>>)> = Vec::new();
    for (id, doc) in witnesses {
        let mut slicer = Slicer::new(scheme);
        for block in doc.blocks.clone() {
            slicer.feed(block);
        }
        let mut map = std::collections::HashMap::new();
        for slice in &slicer.slices {
            if let Some((coord, _)) = &slice.coord {
                let mut words = Vec::new();
                words_of_blocks(&slice.blocks, &mut words);
                map.insert(coord.clone(), words);
                if id == base {
                    base_order.push(coord.clone());
                }
            }
        }
        maps.push((id.clone(), map));
    }
    let base_map = maps
        .iter()
        .find(|(id, _)| id == base)
        .map(|(_, m)| m.clone())
        .unwrap();

    // Accumulate divergences: (coord index, start, end) -> the
    // lemma with each witness's reading, in witness order.
    #[allow(clippy::type_complexity)]
    let mut acc: std::collections::BTreeMap<
        (usize, usize, usize),
        (Vec<String>, Vec<(String, Vec<String>)>),
    > = std::collections::BTreeMap::new();
    for (id, map) in &maps {
        if id == base {
            continue;
        }
        for (ci, coord) in base_order.iter().enumerate() {
            let Some(wit_words) = map.get(coord) else {
                continue;
            };
            let base_words = &base_map[coord];
            for h in diff_words(base_words, wit_words) {
                if h.start == 0 && h.end == 0 {
                    continue; // unanchorable leading addition
                }
                acc.entry((ci, h.start, h.end))
                    .or_insert_with(|| (base_words[h.start..h.end].to_vec(), Vec::new()))
                    .1
                    .push((id.clone(), h.variant));
            }
        }
    }

    // Compose the entries and their anchors.
    let mut pending: Pending = std::collections::HashMap::new();
    let mut counters: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for ((ci, start, end), (lemma, readings)) in &acc {
        let coord = &base_order[*ci];
        let n = counters.entry(*ci).or_insert(0);
        *n += 1;
        let onym: String = format!(
            "app-{}-{n}",
            coord
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
        );
        let (anchor, mut text) = if lemma.is_empty() {
            // Pure addition: anchored after the preceding word.
            let prev = &base_map[coord][start - 1];
            let (wid, var) = &readings[0];
            let mut t = format!("{coord} post {prev} add. {} {wid}", var.join(" "));
            for (wid, var) in &readings[1..] {
                t.push_str(&format!(" : add. {} {wid}", var.join(" ")));
            }
            (*start, t)
        } else {
            (*end, format!("{coord} {} {base}", lemma.join(" ")))
        };
        if !lemma.is_empty() {
            for (wid, var) in readings {
                if var.is_empty() {
                    text.push_str(&format!(" : om. {wid}"));
                } else {
                    text.push_str(&format!(" : {} {wid}", var.join(" ")));
                }
            }
        }
        pending
            .entry(coord.clone())
            .or_default()
            .push_back((anchor, onym, text));
    }
    for queue in pending.values_mut() {
        queue.make_contiguous().sort_by_key(|(a, _, _)| *a);
    }

    // Thread the apparatus through the base document.
    let mut ins = Inserter {
        scheme: scheme.to_string(),
        coord: None,
        word: 0,
        pending,
        ready: Vec::new(),
    };
    let mut blocks = Vec::new();
    for block in base_doc.blocks {
        let walked = ins.walk_block(block);
        blocks.push(walked);
        blocks.append(&mut ins.ready);
    }
    Ok(Document {
        dialect_id: base_doc.dialect_id,
        dialect_version: base_doc.dialect_version,
        blocks,
    })
}
