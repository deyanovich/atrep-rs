//! Atramento: the core endomorphosis into litogramma.
//!
//! Text-to-text: sugar compiles away, litogramma passes through.
//! The compiler is a projection (idempotent), and canonical
//! litogramma input passes through unchanged — the superset
//! property. Spec: the atramento README (2026-07 rewrite).
//!
//! Pilot limitations (documented, deterministic): inline asides
//! are rejected; inline notes must close on the line they open;
//! explicit division episims are matched against sugar-opened
//! divisions top-of-stack only.

use crate::error::{Error, ErrorKind, Result};

/// Compile atramento source to a litogramma deltos.
pub fn atramento_to_litogramma(src: &str) -> Result<String> {
    Compiler::default().compile(src)
}

const DIV_SYMS: [&str; 6] = ["==", "===", "#", "##", "###", "####"];
const DIV_EPISYMS: [&str; 6] = ["==@", "===@", "#@", "##@", "###@", "####@"];
/// Note families: marker, episim, generated-onym prefix.
const NOTE_FAMILIES: [(&str, &str, &str); 4] = [
    ("@^!", "!^@", "atr-mn"),
    ("@^^^", "^^^@", "atr-en"),
    ("@^^", "^^@", "atr-ce"),
    ("@^", "^@", "atr-fn"),
];
/// Emission order of the trailing notes region.
const NOTE_REGION_ORDER: [usize; 4] = [3, 2, 1, 0];

#[derive(Debug, Clone, PartialEq)]
enum Open {
    Div { rung: usize },
    Act,
    Scene,
    Dialogue,
    VerseDialogue,
}

impl Open {
    fn episim(&self) -> &'static str {
        match self {
            Open::Div { rung } => DIV_EPISYMS[*rung],
            Open::Act => "=:@",
            Open::Scene => "#:@",
            Open::Dialogue => ":@",
            Open::VerseDialogue => "~:@",
        }
    }
}

#[derive(Debug, Clone)]
struct NoteDef {
    family: usize,
    name: String,
    content: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct PendingList {
    genos: Option<String>,
    param: Option<String>,
}

#[derive(Default)]
struct Compiler {
    out: Vec<String>,
    /// Rung index of one `#` (default: section).
    base: Option<usize>,
    saw_heading: bool,
    div_counters: [u32; 6],
    act_n: u32,
    scene_n: u32,
    open: Vec<Open>,
    notes: Vec<NoteDef>,
    /// (family, name) in order of first reference.
    callout_order: Vec<(usize, String)>,
    inline_note_n: [u32; 4],
    aside_n: u32,
}

impl Compiler {
    fn compile(mut self, src: &str) -> Result<String> {
        let lines: Vec<&str> = src.lines().collect();
        self.block_pass(&lines)?;
        self.close_all();
        self.emit_notes_region();
        let mut out = self.out.join("\n");
        out.push('\n');
        if !out
            .lines()
            .find(|l| !l.trim().is_empty())
            .is_some_and(|l| l.starts_with("@@@!"))
        {
            out = format!("@@@!litogramma\n\n{out}");
        }
        Ok(out)
    }

    fn push_inline(&mut self, s: &str) -> Result<()> {
        let text = self.inline(s)?.text;
        self.out.push(text);
        Ok(())
    }

    // ----- implied closure ---------------------------------------

    fn close_one(&mut self) {
        if let Some(top) = self.open.pop() {
            // Episims sit directly after the content, before any
            // blank lines separating it from what follows.
            let mut p = self.out.len();
            while p > 0 && self.out[p - 1].is_empty() {
                p -= 1;
            }
            self.out.insert(p, top.episim().to_string());
        }
    }

    fn close_dialogue(&mut self) {
        while matches!(
            self.open.last(),
            Some(Open::Dialogue) | Some(Open::VerseDialogue)
        ) {
            self.close_one();
        }
    }

    fn close_scene(&mut self) {
        self.close_dialogue();
        while matches!(self.open.last(), Some(Open::Scene)) {
            self.close_one();
        }
    }

    fn close_act(&mut self) {
        self.close_scene();
        while matches!(self.open.last(), Some(Open::Act)) {
            self.close_one();
        }
    }

    fn close_for_heading(&mut self, rung: usize) {
        self.close_act();
        while matches!(self.open.last(), Some(Open::Div { rung: r }) if *r >= rung) {
            self.close_one();
        }
    }

    fn close_all(&mut self) {
        while !self.open.is_empty() {
            self.close_one();
        }
    }

    /// Taxis of the innermost sugar divisions plus `own`.
    fn hier_taxis(&self, own: u32, rung: usize) -> String {
        let mut parts: Vec<String> = self
            .open
            .iter()
            .filter_map(|o| match o {
                Open::Div { rung: r } if *r < rung => Some(self.div_counters[*r].to_string()),
                _ => None,
            })
            .collect();
        parts.push(own.to_string());
        parts.join(".")
    }

    // ----- block layer -------------------------------------------

    fn block_pass(&mut self, lines: &[&str]) -> Result<()> {
        let mut i = 0;
        while i < lines.len() {
            i = self.block_step(lines, i)?;
        }
        Ok(())
    }

    fn block_step(&mut self, lines: &[&str], i: usize) -> Result<usize> {
        let line = lines[i];
        let trimmed = line.trim_end();

        // Blank lines pass through.
        if trimmed.trim().is_empty() {
            self.out.push(String::new());
            return Ok(i + 1);
        }

        // Explicit episim of a sugar-opened block: the author
        // closed it; pop without inserting.
        if let Some(top) = self.open.last() {
            if trimmed == top.episim() {
                self.open.pop();
                self.out.push(trimmed.to_string());
                return Ok(i + 1);
            }
        }

        // Verse dialogue content: stichoi until a boundary.
        if matches!(self.open.last(), Some(Open::VerseDialogue))
            && !is_drama_boundary(trimmed)
            && !is_atx_heading(trimmed)
        {
            self.push_inline(line)?;
            return Ok(i + 1);
        }

        // Englossis blocks pass through verbatim.
        if trimmed.starts_with("@@@!(") {
            let mut j = i;
            while j < lines.len() {
                self.out.push(lines[j].to_string());
                if lines[j].trim_end() == "!@@@" {
                    return Ok(j + 1);
                }
                j += 1;
            }
            return Err(syntax("unterminated englossis block"));
        }

        // Dialektos declaration and block enlexis pass through.
        if trimmed.starts_with("@@@!") {
            self.out.push(line.to_string());
            return Ok(i + 1);
        }
        if trimmed.starts_with("@@@\"") {
            let mut j = i;
            self.out.push(lines[j].to_string());
            j += 1;
            while j < lines.len() {
                self.out.push(lines[j].to_string());
                if lines[j].trim_start().starts_with("\"@@@") {
                    return Ok(j + 1);
                }
                j += 1;
            }
            return Err(syntax("unterminated block enlexis"));
        }

        // The heading-base directive.
        if let Some(rest) = trimmed.strip_prefix("@@#(") {
            let value = rest.strip_suffix(')').ok_or_else(|| {
                syntax("malformed heading-base directive (expected `@@#(part|chapter|section)`)")
            })?;
            let base = match value {
                "part" => 0,
                "chapter" => 1,
                "section" => 2,
                other => {
                    return Err(syntax(&format!(
                        "unknown heading base `{other}` (expected part, chapter, or section)"
                    )));
                }
            };
            if self.saw_heading {
                return Err(syntax(
                    "the heading base must be declared before the first heading",
                ));
            }
            if self.base.replace(base).is_some() {
                return Err(syntax("duplicate heading-base directive"));
            }
            // The directive vanishes with its separating blank.
            if lines.get(i + 1).is_some_and(|l| l.trim().is_empty()) {
                return Ok(i + 2);
            }
            return Ok(i + 1);
        }

        // Code fences.
        if let Some(rest) = fence_rest(trimmed) {
            let lang = rest.trim().to_string();
            let run = trimmed.len() - trimmed.trim_start_matches('`').len();
            self.out.push("@@@\"".to_string());
            let mut j = i + 1;
            while j < lines.len() {
                let l = lines[j].trim_end();
                if fence_rest(l).is_some_and(|r| r.trim().is_empty())
                    && l.len() - l.trim_start_matches('`').len() >= run
                {
                    let close = if lang.is_empty() {
                        "\"@@@".to_string()
                    } else {
                        format!("\"@@@.{lang}")
                    };
                    self.out.push(close);
                    return Ok(j + 1);
                }
                self.out.push(lines[j].to_string());
                j += 1;
            }
            return Err(syntax("unterminated code fence"));
        }

        // Verse fences.
        if trimmed == "~" || trimmed.starts_with("~ ") {
            let lemma = trimmed.strip_prefix('~').unwrap_or("").trim();
            self.out.push(if lemma.is_empty() {
                "@~".to_string()
            } else {
                format!("@~ {lemma}")
            });
            let mut j = i + 1;
            while j < lines.len() {
                let l = lines[j].trim_end();
                if l == "~" || l.starts_with("~ ") {
                    let hypo = l.strip_prefix('~').unwrap_or("").trim();
                    self.out.push(if hypo.is_empty() {
                        "~@".to_string()
                    } else {
                        format!("~@ {hypo}")
                    });
                    return Ok(j + 1);
                }
                self.push_inline(&lines[j].replace('\t', "    "))?;
                j += 1;
            }
            return Err(syntax("unterminated verse fence"));
        }

        // Display math passes through verbatim.
        if trimmed.starts_with("@$$") {
            let mut j = i;
            while j < lines.len() {
                self.out.push(lines[j].to_string());
                if j > i && lines[j].trim_start().starts_with("$$@") {
                    return Ok(j + 1);
                }
                j += 1;
            }
            return Err(syntax("unterminated display math"));
        }

        // ATX headings.
        if is_atx_heading(trimmed) {
            return self.heading(lines, i);
        }

        // Blockquotes.
        if trimmed.starts_with('>') {
            return self.blockquote(lines, i);
        }

        // Figures.
        if let Some((caption, path, lemma)) = figure_parts(trimmed) {
            self.out.push(match lemma {
                Some(l) => format!("@<() {l}"),
                None => "@<()".to_string(),
            });
            self.out.push(format!("@@@@({path})"));
            self.push_inline(&caption)?;
            self.out.push("<@".to_string());
            return Ok(i + 1);
        }

        // Sugar aside block: `@|kind` … `|@`.
        if let Some(kind) = aside_kind(trimmed) {
            self.aside_n += 1;
            let onym = format!("atr-as-{}", self.aside_n);
            self.out.push(format!("@|<({onym})"));
            self.out.push(String::new());
            self.out.push("@|".to_string());
            let mut j = i + 1;
            while j < lines.len() {
                if lines[j].trim_end() == "|@" {
                    self.out.push(format!("|@({onym}).{kind}"));
                    return Ok(j + 1);
                }
                self.push_inline(lines[j])?;
                j += 1;
            }
            return Err(syntax("unterminated aside"));
        }

        // Lists (sugar heads and items).
        if is_list_head_sugar(trimmed) || is_list_item(trimmed) {
            return self.list(lines, i);
        }

        // Note definition sugar: a paragraph starting with a
        // named callout.
        if let Some((family, name, first)) = note_def_parts(trimmed) {
            let mut content = Vec::new();
            content.push(self.inline(&first)?.text);
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim().is_empty() && !self.is_block_start(lines[j]) {
                content.push(self.inline(lines[j])?.text);
                j += 1;
            }
            self.notes.push(NoteDef {
                family,
                name,
                content,
            });
            // The definition vanishes with its separating blank.
            if self.out.last().is_some_and(|l| l.is_empty())
                && lines.get(j).is_some_and(|l| l.trim().is_empty())
            {
                return Ok(j + 1);
            }
            return Ok(j);
        }

        // Drama.
        if trimmed.starts_with("@:") {
            return self.drama(lines, i);
        }

        // Top matter tail elision.
        if let Some(done) = self.top_matter(trimmed)? {
            self.out.push(done);
            return Ok(i + 1);
        }

        // Plain paragraph: gather and inline-process as a unit so
        // emphasis may wrap across lines.
        let mut j = i;
        let mut para = Vec::new();
        while j < lines.len() && !lines[j].trim().is_empty() && !self.is_block_start(lines[j]) {
            para.push(lines[j]);
            j += 1;
        }
        let processed = self.inline(&para.join("\n"))?;
        for l in processed.text.split('\n') {
            self.out.push(l.to_string());
        }
        Ok(j)
    }

    /// Would this line start a block construct (ending the
    /// current paragraph)?
    fn is_block_start(&self, line: &str) -> bool {
        let t = line.trim_end();
        is_atx_heading(t)
            || t.starts_with('>')
            || fence_rest(t).is_some()
            || t == "~"
            || t.starts_with("~ ")
            || is_list_item(t)
            || is_list_head_sugar(t)
            || t.starts_with("@:")
            || t.starts_with("@$$")
            || t.starts_with("@@@")
            || figure_parts(t).is_some()
            || aside_kind(t).is_some()
            || note_def_parts(t).is_some()
            || self.open.last().is_some_and(|top| t == top.episim())
    }

    fn heading(&mut self, lines: &[&str], i: usize) -> Result<usize> {
        let trimmed = lines[i].trim_end();
        let hashes = trimmed.len() - trimmed.trim_start_matches('#').len();
        let rest = &trimmed[hashes..];
        let (unnumbered, title) = match rest.strip_prefix('!') {
            Some(r) => (true, r.trim()),
            None => (false, rest.trim()),
        };
        let base = self.base.unwrap_or(2);
        let rung = base + hashes - 1;
        if rung >= DIV_SYMS.len() {
            return Err(syntax(&format!(
                "heading `{}` is deeper than the division ladder",
                "#".repeat(hashes)
            )));
        }
        self.saw_heading = true;
        self.close_for_heading(rung);
        let mut title = title.to_string();
        if unnumbered {
            self.open.push(Open::Div { rung });
            title = self.inline(&title)?.text;
            self.out.push(format!("@{} {title}", DIV_SYMS[rung]));
        } else {
            self.div_counters[rung] += 1;
            for c in self.div_counters[rung + 1..].iter_mut() {
                *c = 0;
            }
            let own = self.div_counters[rung];
            // The taxis carries the division's own ordinal only:
            // hierarchical taxis values await the upstream taxis
            // grammar (F3). The ordinal sim, being content, may
            // still render the full dotted form.
            let hier = self.hier_taxis(own, rung);
            title = title
                .replace("@..@", &format!("@.{hier}.@"))
                .replace("@.@", &format!("@.{own}.@"));
            self.open.push(Open::Div { rung });
            title = self.inline(&title)?.text;
            self.out.push(format!("@{}({own}) {title}", DIV_SYMS[rung]));
        }
        Ok(i + 1)
    }

    fn blockquote(&mut self, lines: &[&str], i: usize) -> Result<usize> {
        let mut j = i;
        let mut body: Vec<String> = Vec::new();
        let mut hypograph = None;
        while j < lines.len() && lines[j].trim_end().starts_with('>') {
            let l = lines[j].trim_end();
            let content = l[1..].strip_prefix(' ').unwrap_or(&l[1..]);
            body.push(content.to_string());
            j += 1;
        }
        if let Some(last) = body.last() {
            if let Some(attr) = last.strip_prefix("--") {
                hypograph = Some(attr.trim().to_string());
                body.pop();
            }
        }
        while body.last().is_some_and(|l| l.trim().is_empty()) {
            body.pop();
        }
        self.out.push("@\"".to_string());
        for l in &body {
            self.push_inline(l)?;
        }
        self.out.push(match hypograph {
            Some(a) if !a.is_empty() => format!("\"@ {a}"),
            _ => "\"@".to_string(),
        });
        Ok(j)
    }

    fn list(&mut self, lines: &[&str], i: usize) -> Result<usize> {
        let mut j = i;
        let mut pending = PendingList::default();
        let t = lines[j].trim_end();
        if is_list_head_sugar(t) {
            if let Some(param) = t.strip_prefix("@.. ") {
                pending.param = Some(param.trim().to_string());
            } else {
                for (head, _) in [("@--", "--@"), ("@..", "..@"), ("@::;", ";::@")] {
                    if let Some(g) = t.strip_prefix(head) {
                        if !g.is_empty() {
                            pending.genos = Some(g.to_string());
                        }
                    }
                }
            }
            j += 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j >= lines.len() || !is_list_item(lines[j].trim_end()) {
                return Err(syntax("list head sugar with no list following"));
            }
        }

        // Collect items: (marker line, continuation lines).
        let first = lines[j].trim_end();
        let kind = list_kind(first).ok_or_else(|| syntax("expected a list item"))?;
        let mut items: Vec<(String, Vec<String>)> = Vec::new();
        while j < lines.len() {
            let t = lines[j].trim_end();
            if list_kind(t) == Some(kind) {
                items.push((t.to_string(), Vec::new()));
                j += 1;
                continue;
            }
            // Continuations: indented lines (and blank lines that
            // precede further indented content).
            if !items.is_empty() {
                if t.trim().is_empty() {
                    let mut k = j + 1;
                    while k < lines.len() && lines[k].trim().is_empty() {
                        k += 1;
                    }
                    if k < lines.len() && is_indented(lines[k]) {
                        items.last_mut().unwrap().1.push(String::new());
                        j += 1;
                        continue;
                    }
                    break;
                }
                if is_indented(lines[j]) {
                    items.last_mut().unwrap().1.push(dedent(lines[j]));
                    j += 1;
                    continue;
                }
            }
            break;
        }

        match kind {
            ListKind::Unordered => self.emit_unordered(&items, &pending)?,
            ListKind::Ordered => self.emit_ordered(&items, &pending)?,
            ListKind::Definition => self.emit_definition(&items, &pending)?,
        }
        Ok(j)
    }

    fn emit_item_body(&mut self, first: &str, cont: &[String]) -> Result<()> {
        // Nested sugar lists inside continuations recurse through
        // the block layer; plain lines are paragraphs.
        let text = self.inline(first)?.text;
        self.out.push(text);
        let mut k = 0;
        while k < cont.len() {
            let t = cont[k].trim_end();
            if is_list_item(t) {
                let rest: Vec<&str> = cont[k..].iter().map(|s| s.as_str()).collect();
                let consumed = self.list(&rest, 0)?;
                k += consumed;
                continue;
            }
            if t.trim().is_empty() {
                self.out.push(String::new());
            } else {
                self.push_inline(t)?;
            }
            k += 1;
        }
        Ok(())
    }

    fn emit_unordered(
        &mut self,
        items: &[(String, Vec<String>)],
        pending: &PendingList,
    ) -> Result<()> {
        self.out.push("@--".to_string());
        for (marker, cont) in items {
            let text = marker.strip_prefix("- ").unwrap_or(marker);
            self.out.push("@-".to_string());
            self.emit_item_body(text, cont)?;
            self.out.push("-@".to_string());
        }
        self.out.push(match &pending.genos {
            Some(g) => format!("--@.{g}"),
            None => "--@".to_string(),
        });
        Ok(())
    }

    fn emit_ordered(
        &mut self,
        items: &[(String, Vec<String>)],
        pending: &PendingList,
    ) -> Result<()> {
        // Category and start: the head parameter wins; else the
        // first item's ordinal; else numeric from 1.
        let first_ord = items
            .first()
            .and_then(|(m, _)| m.split('.').next())
            .unwrap_or("")
            .trim()
            .to_string();
        let param = pending.param.clone().or_else(|| {
            if first_ord.is_empty() {
                None
            } else {
                Some(first_ord)
            }
        });
        let (alpha, start) = match param.as_deref() {
            Some(p) if p.chars().all(|c| c.is_ascii_uppercase()) && !p.is_empty() => {
                (true, alpha_to_n(p))
            }
            Some(p) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (
                false,
                p.parse::<u32>()
                    .map_err(|_| syntax("ordinal out of range"))?,
            ),
            Some(p) => {
                return Err(syntax(&format!("unrecognized list ordinal `{p}`")));
            }
            None => (false, 1),
        };
        self.out.push("@..".to_string());
        for (n, (marker, cont)) in items.iter().enumerate() {
            let text = marker
                .split_once(". ")
                .map(|(_, t)| t)
                .unwrap_or_else(|| marker.strip_prefix(". ").unwrap_or(marker));
            let value = start + n as u32;
            let taxis = if alpha {
                n_to_alpha(value)
            } else {
                value.to_string()
            };
            self.out.push(format!("@.-({taxis})"));
            self.emit_item_body(text, cont)?;
            self.out.push("-.@".to_string());
        }
        self.out.push(match &pending.genos {
            Some(g) => format!("..@.{g}"),
            None => "..@".to_string(),
        });
        Ok(())
    }

    fn emit_definition(
        &mut self,
        items: &[(String, Vec<String>)],
        pending: &PendingList,
    ) -> Result<()> {
        self.out.push("@::;".to_string());
        for (marker, cont) in items {
            let body = marker.strip_prefix(": ").unwrap_or(marker);
            let (lemma, first_def) = match body.split_once(" :: ") {
                Some((l, d)) => (l.trim(), Some(d.trim())),
                None => (body.trim_end_matches(" ::").trim(), None),
            };
            self.out.push(format!("@:: {lemma}"));
            // Definitions: the inline one, plus `::`-introduced
            // continuations, plus indented paragraphs appended to
            // the last definition.
            let mut defs: Vec<Vec<String>> = Vec::new();
            if let Some(d) = first_def {
                defs.push(vec![d.to_string()]);
            }
            for c in cont {
                let t = c.trim_start();
                if let Some(d) = t.strip_prefix(":: ") {
                    defs.push(vec![d.to_string()]);
                } else if t.is_empty() {
                    if let Some(last) = defs.last_mut() {
                        last.push(String::new());
                    }
                } else if let Some(last) = defs.last_mut() {
                    last.push(t.to_string());
                } else {
                    defs.push(vec![t.to_string()]);
                }
            }
            for def in &defs {
                self.out.push("@;".to_string());
                for l in def {
                    if l.is_empty() {
                        self.out.push(String::new());
                    } else {
                        self.push_inline(l)?;
                    }
                }
                self.out.push(";@".to_string());
            }
            self.out.push("::@".to_string());
        }
        self.out.push(match &pending.genos {
            Some(g) => format!(";::@.{g}"),
            None => ";::@".to_string(),
        });
        Ok(())
    }

    fn drama(&mut self, lines: &[&str], i: usize) -> Result<usize> {
        let trimmed = lines[i].trim_end();

        // Act.
        if let Some(rest) = trimmed.strip_prefix("@:= ") {
            self.close_act();
            self.act_n += 1;
            self.scene_n = 0;
            let lemma = rest
                .replace("@.@", &format!("@.{}.@", self.act_n))
                .replace("@..@", &format!("@.{}.@", self.act_n));
            let lemma = self.inline(&lemma)?.text;
            self.out.push(format!("@:=({}) {lemma}", self.act_n));
            self.open.push(Open::Act);
            return Ok(i + 1);
        }
        if trimmed.starts_with("@:=(") {
            self.close_act();
            self.push_inline(trimmed)?;
            self.open.push(Open::Act);
            return Ok(i + 1);
        }

        // Scene.
        if let Some(rest) = trimmed.strip_prefix("@:# ") {
            self.close_scene();
            self.scene_n += 1;
            let lemma = rest.replace("@.@", &format!("@.{}.@", self.scene_n));
            let lemma = self.inline(&lemma)?.text;
            self.out.push(format!("@:#({}) {lemma}", self.scene_n));
            self.open.push(Open::Scene);
            return Ok(i + 1);
        }
        if trimmed.starts_with("@:#(") {
            self.close_scene();
            self.push_inline(trimmed)?;
            self.open.push(Open::Scene);
            return Ok(i + 1);
        }

        // Dramatis personae.
        if trimmed.starts_with("@:!!") {
            self.push_inline(trimmed)?;
            return Ok(i + 1);
        }
        if let Some(rest) = trimmed.strip_prefix("@:! ") {
            if rest.ends_with("!:@") {
                self.push_inline(trimmed)?;
            } else {
                let name = self.inline(rest)?.text;
                self.out.push(format!("@:! {name} !:@"));
            }
            return Ok(i + 1);
        }

        // Block stage direction.
        if trimmed == "@:[" {
            self.out.push(trimmed.to_string());
            return Ok(i + 1);
        }
        if let Some(rest) = trimmed.strip_prefix("@:[ ") {
            if trimmed.contains("]:@") {
                self.push_inline(trimmed)?;
                return Ok(i + 1);
            }
            self.out.push("@:[".to_string());
            self.push_inline(rest)?;
            self.out.push("]:@".to_string());
            return Ok(i + 1);
        }

        // Prose-fiction dialogue line: closes at paragraph end.
        if trimmed.starts_with("@:- ") {
            let mut j = i;
            let mut para = Vec::new();
            while j < lines.len() && !lines[j].trim().is_empty() {
                para.push(lines[j].trim_end().to_string());
                j += 1;
            }
            let has_close = para.last().is_some_and(|l| l.ends_with("-:@"));
            for (n, l) in para.iter().enumerate() {
                let mut text = self.inline(l)?.text;
                if n == para.len() - 1 && !has_close {
                    text.push_str(" -:@");
                }
                self.out.push(text);
            }
            return Ok(j);
        }

        // Dramatic pause.
        if let Some(_rest) = trimmed.strip_prefix("@:. ") {
            if trimmed.ends_with(".:@") {
                self.out.push(trimmed.to_string());
            } else {
                self.out.push(format!("{trimmed} .:@"));
            }
            return Ok(i + 1);
        }

        // Verse dialogue.
        if let Some(rest) = trimmed.strip_prefix("@:~ ") {
            self.close_dialogue();
            let lemma = self.inline(rest)?.text;
            self.out.push(format!("@:~ {lemma}"));
            self.open.push(Open::VerseDialogue);
            return Ok(i + 1);
        }

        // Prose dialogue.
        if let Some(rest) = trimmed.strip_prefix("@: ") {
            self.close_dialogue();
            let lemma = self.inline(rest)?.text;
            self.out.push(format!("@: {lemma}"));
            self.open.push(Open::Dialogue);
            return Ok(i + 1);
        }

        // Anything else `@:`-shaped passes through.
        self.push_inline(trimmed)?;
        Ok(i + 1)
    }

    /// Top-matter tail elision: `@=`, `@=_`, `@=:`, `@=;` line
    /// forms get their episim restored.
    fn top_matter(&mut self, trimmed: &str) -> Result<Option<String>> {
        for (sym, episim) in [("@=_", "_=@"), ("@=:", ":=@"), ("@=;", ";=@"), ("@=", "=@")] {
            if let Some(rest) = trimmed.strip_prefix(sym) {
                // Not top matter: divisions, ToC, abstract.
                if sym == "@="
                    && (rest.starts_with('=') || rest.starts_with('#') || rest.starts_with('"'))
                {
                    return Ok(None);
                }
                if rest.is_empty() {
                    return Ok(None);
                }
                if rest.trim_end().ends_with(episim) {
                    return Ok(Some(trimmed.to_string()));
                }
                let content = self.inline(rest.trim())?.text;
                return Ok(Some(format!("{sym} {content} {episim}")));
            }
        }
        Ok(None)
    }

    // ----- notes region ------------------------------------------

    fn record_callout(&mut self, family: usize, name: &str) {
        if !self
            .callout_order
            .iter()
            .any(|(f, n)| *f == family && n == name)
        {
            self.callout_order.push((family, name.to_string()));
        }
    }

    fn emit_notes_region(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        let notes = std::mem::take(&mut self.notes);
        for family in NOTE_REGION_ORDER {
            let mut defs: Vec<&NoteDef> = notes.iter().filter(|d| d.family == family).collect();
            defs.sort_by_key(|d| {
                self.callout_order
                    .iter()
                    .position(|(f, n)| *f == family && *n == d.name)
                    .unwrap_or(usize::MAX)
            });
            let (marker, episim, _) = NOTE_FAMILIES[family];
            for def in defs {
                self.out.push(String::new());
                self.out.push(marker.to_string());
                for l in &def.content {
                    self.out.push(l.clone());
                }
                self.out.push(format!("{episim}({})", def.name));
            }
        }
    }

    // ----- inline layer ------------------------------------------

    fn inline(&mut self, s: &str) -> Result<Inline> {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::new();
        let mut emph: Vec<Emph> = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];

            // Escapes.
            if c == '\\' && i + 1 < chars.len() {
                let n = chars[i + 1];
                if n == '@' {
                    out.push_str("\\@");
                } else if "*/-.#>`~\\|:[]!<".contains(n) {
                    out.push(n);
                } else {
                    out.push(c);
                    out.push(n);
                }
                i += 2;
                continue;
            }

            // Backtick code spans.
            if c == '`' {
                let run = run_len(&chars, i, '`');
                if let Some(end) = find_run(&chars, i + run, '`', run) {
                    let content: String = chars[i + run..end].iter().collect();
                    out.push_str("@@\"");
                    out.push_str(&content);
                    out.push_str("\"@@");
                    i = end + run;
                    continue;
                }
                out.push(c);
                i += 1;
                continue;
            }

            // Autolinks.
            if c == '<' {
                if let Some((url, end)) = autolink(&chars, i) {
                    out.push_str("@><");
                    out.push_str(&url);
                    out.push_str("><@");
                    i = end;
                    continue;
                }
            }

            // Reserved: links with text.
            if c == '[' {
                if let Some(end) = link_with_text(&chars, i) {
                    let snippet: String = chars[i..end.min(i + 30)].iter().collect();
                    return Err(syntax(&format!(
                        "`{snippet}…`: [text](url) links are reserved pending atrep F9 \
                         (litogramma has no hidden-href sim yet); write the URL as an \
                         autolink <url> or escape the bracket"
                    )));
                }
            }

            // `@`-tokens: strict sims are skip regions; note sugar
            // resolves here.
            if c == '@' {
                if let Some(consumed) = self.at_token(&chars, i, &mut out)? {
                    i = consumed;
                    continue;
                }
                out.push('@');
                i += 1;
                continue;
            }

            // Emphasis delimiters. BoldItal opens on `*/` and
            // closes on `/*`, so both characters dispatch to it.
            if c == '*' || c == '/' {
                let candidates: &[Emph] = if c == '*' {
                    &[Emph::BoldItal, Emph::Bold]
                } else {
                    &[Emph::BoldItal, Emph::Ital]
                };
                let mut handled = false;
                for &kind in candidates {
                    let width = kind.width();
                    let prev = if i == 0 { None } else { Some(chars[i - 1]) };
                    let next = chars.get(i + width).copied();
                    let closer = kind.closer();
                    // Close?
                    if emph.last() == Some(&kind)
                        && matches_at(&chars, i, closer)
                        && flank_close(prev, chars.get(i + closer.chars().count()).copied())
                    {
                        out.push_str(kind.episim());
                        emph.pop();
                        i += closer.chars().count();
                        handled = true;
                        break;
                    }
                    // Open?
                    if matches_at(&chars, i, kind.opener())
                        && flank_open(prev, next)
                        && !emph.contains(&kind)
                        && closer_ahead(&chars, i + width, kind)
                    {
                        out.push_str(kind.sim());
                        emph.push(kind);
                        i += width;
                        handled = true;
                        break;
                    }
                }
                if handled {
                    continue;
                }
                out.push(c);
                i += 1;
                continue;
            }

            // Dash normalization.
            if c == '\u{2014}' {
                out.push_str("--");
                i += 1;
                continue;
            }
            if c == '\u{2013}' {
                out.push('-');
                i += 1;
                continue;
            }

            out.push(c);
            i += 1;
        }
        if let Some(kind) = emph.last() {
            return Err(syntax(&format!(
                "emphasis opened with `{}` is never closed in its paragraph",
                kind.opener()
            )));
        }
        Ok(Inline { text: out })
    }

    /// Handle a `@`-initial token at `i`; returns the new index
    /// when consumed.
    fn at_token(&mut self, chars: &[char], i: usize, out: &mut String) -> Result<Option<usize>> {
        // Strict skip regions: copy verbatim through the episim.
        const SKIPS: [(&str, &str); 9] = [
            ("@@\"", "\"@@"),
            ("@$", "$@"),
            ("@><", "><@"),
            ("@*/", "/*@"),
            ("@/", "/@"),
            ("@*", "*@"),
            ("@,", ",@"),
            ("@%", "%@"),
            ("@>:", ":>@"),
        ];
        for (open, close) in SKIPS {
            if matches_at(chars, i, open) {
                let from = i + open.chars().count();
                if let Some(end) = find_str(chars, from, close) {
                    let upto = end + close.chars().count();
                    out.push_str(&chars[i..upto].iter().collect::<String>());
                    return Ok(Some(upto));
                }
                return Ok(None);
            }
        }

        // Inline stage direction: explicit close or end of line.
        if matches_at(chars, i, "@:(") {
            let from = i + 3;
            if let Some(end) = find_str(chars, from, "):@") {
                let upto = end + 3;
                out.push_str(&chars[i..upto].iter().collect::<String>());
                return Ok(Some(upto));
            }
            let eol = find_char(chars, from, '\n').unwrap_or(chars.len());
            out.push_str(&chars[i..eol].iter().collect::<String>());
            out.push_str(" ):@");
            return Ok(Some(eol));
        }
        if matches_at(chars, i, "@:_") {
            if let Some(end) = find_str(chars, i + 3, "_:@") {
                let upto = end + 3;
                out.push_str(&chars[i..upto].iter().collect::<String>());
                return Ok(Some(upto));
            }
            return Ok(None);
        }

        // Inline asides are a pilot gap.
        if matches_at(chars, i, "@|") {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some((kind, _)) = rest.split_once('|') {
                if !kind.is_empty()
                    && kind
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                {
                    return Err(syntax(
                        "inline asides are not supported by the pilot compiler; \
                         use the block form",
                    ));
                }
            }
            return Ok(None);
        }

        // Note families: strict callout, inline note, or callout
        // sugar.
        for (fi, (marker, episim, prefix)) in NOTE_FAMILIES.iter().enumerate() {
            if !matches_at(chars, i, marker) {
                continue;
            }
            let from = i + marker.chars().count();
            match chars.get(from) {
                // Strict callout monosim: copy through `)`.
                Some('(') => {
                    if let Some(end) = find_char(chars, from, ')') {
                        let name: String = chars[from + 1..end].iter().collect();
                        self.record_callout(fi, &name);
                        out.push_str(&chars[i..=end].iter().collect::<String>());
                        return Ok(Some(end + 1));
                    }
                    return Ok(None);
                }
                _ => {}
            }
            // An episim before the next note opener means an
            // inline note.
            let next_open = NOTE_FAMILIES
                .iter()
                .filter_map(|(m, _, _)| find_str(chars, from, m))
                .min();
            let eol = find_char(chars, from, '\n').unwrap_or(chars.len());
            let close = find_str(chars, from, episim);
            if let Some(end) = close {
                if end < eol && next_open.is_none_or(|n| end < n) {
                    let content: String = chars[from..end].iter().collect();
                    self.inline_note_n[fi] += 1;
                    let name = format!("{prefix}-{}", self.inline_note_n[fi]);
                    let content = self.inline(&content)?.text;
                    self.record_callout(fi, &name);
                    self.notes.push(NoteDef {
                        family: fi,
                        name: name.clone(),
                        content: vec![content],
                    });
                    out.push_str(&format!("{marker}({name})"));
                    return Ok(Some(end + episim.chars().count()));
                }
            }
            // Callout sugar: a bare alphanumeric name.
            let mut end = from;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '-') {
                end += 1;
            }
            if end > from {
                let name: String = chars[from..end].iter().collect();
                self.record_callout(fi, &name);
                out.push_str(&format!("{marker}({name})"));
                return Ok(Some(end));
            }
            return Ok(None);
        }

        Ok(None)
    }
}

// ----- inline helpers --------------------------------------------

struct Inline {
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Emph {
    Ital,
    Bold,
    BoldItal,
}

impl Emph {
    fn width(self) -> usize {
        match self {
            Emph::BoldItal => 2,
            _ => 1,
        }
    }
    fn opener(self) -> &'static str {
        match self {
            Emph::Ital => "/",
            Emph::Bold => "*",
            Emph::BoldItal => "*/",
        }
    }
    fn closer(self) -> &'static str {
        match self {
            Emph::Ital => "/",
            Emph::Bold => "*",
            Emph::BoldItal => "/*",
        }
    }
    fn sim(self) -> &'static str {
        match self {
            Emph::Ital => "@/",
            Emph::Bold => "@*",
            Emph::BoldItal => "@*/",
        }
    }
    fn episim(self) -> &'static str {
        match self {
            Emph::Ital => "/@",
            Emph::Bold => "*@",
            Emph::BoldItal => "/*@",
        }
    }
}

fn is_ws(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace())
}

fn flank_open(prev: Option<char>, next: Option<char>) -> bool {
    let before_ok = prev.is_none_or(|c| c.is_whitespace() || !c.is_alphanumeric());
    let after_ok = next.is_some_and(|c| !c.is_whitespace());
    before_ok && after_ok
}

fn flank_close(prev: Option<char>, next: Option<char>) -> bool {
    let before_ok = prev.is_some_and(|c| !c.is_whitespace());
    let after_ok = is_ws(next) || next.is_some_and(|c| !c.is_alphanumeric());
    before_ok && after_ok
}

fn closer_ahead(chars: &[char], from: usize, kind: Emph) -> bool {
    let closer = kind.closer();
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if matches_at(chars, i, closer) {
            let prev = if i == 0 { None } else { Some(chars[i - 1]) };
            let next = chars.get(i + closer.chars().count()).copied();
            if flank_close(prev, next) {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn matches_at(chars: &[char], i: usize, s: &str) -> bool {
    let mut j = i;
    for c in s.chars() {
        if chars.get(j) != Some(&c) {
            return false;
        }
        j += 1;
    }
    true
}

fn find_str(chars: &[char], from: usize, s: &str) -> Option<usize> {
    (from..chars.len()).find(|&i| matches_at(chars, i, s))
}

fn find_char(chars: &[char], from: usize, c: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == c)
}

fn run_len(chars: &[char], i: usize, c: char) -> usize {
    let mut n = 0;
    while chars.get(i + n) == Some(&c) {
        n += 1;
    }
    n
}

fn find_run(chars: &[char], from: usize, c: char, run: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == c {
            let n = run_len(chars, i, c);
            if n == run {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

fn autolink(chars: &[char], i: usize) -> Option<(String, usize)> {
    let close = find_char(chars, i + 1, '>')?;
    let url: String = chars[i + 1..close].iter().collect();
    let (scheme, rest) = url.split_once(':')?;
    if scheme.is_empty()
        || rest.is_empty()
        || !scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+.-".contains(c))
        || url.contains(char::is_whitespace)
    {
        return None;
    }
    Some((url, close + 1))
}

fn link_with_text(chars: &[char], i: usize) -> Option<usize> {
    let close = find_char(chars, i + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let paren = find_char(chars, close + 2, ')')?;
    Some(paren + 1)
}

// ----- block helpers ---------------------------------------------

fn syntax(msg: &str) -> Error {
    Error::new(ErrorKind::Syntax(format!("atramento: {msg}")))
}

fn is_atx_heading(t: &str) -> bool {
    let hashes = t.len() - t.trim_start_matches('#').len();
    if hashes == 0 || hashes > 6 {
        return false;
    }
    let rest = &t[hashes..];
    rest.starts_with(' ') || rest.starts_with("! ")
}

fn fence_rest(t: &str) -> Option<&str> {
    if t.starts_with("```") {
        Some(t.trim_start_matches('`'))
    } else {
        None
    }
}

fn figure_parts(t: &str) -> Option<(String, String, Option<String>)> {
    let rest = t.strip_prefix("![")?;
    let (caption, rest) = rest.split_once("](")?;
    let target = rest.strip_suffix(')')?;
    if target.contains(')') {
        return None;
    }
    let (path, lemma) = match target.split_once('|') {
        Some((p, l)) => (p.to_string(), Some(l.to_string())),
        None => (target.to_string(), None),
    };
    Some((caption.to_string(), path, lemma))
}

fn aside_kind(t: &str) -> Option<String> {
    let rest = t.strip_prefix("@|")?;
    if rest.is_empty()
        || !rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return None;
    }
    Some(rest.to_string())
}

fn note_def_parts(t: &str) -> Option<(usize, String, String)> {
    for (fi, (marker, _, _)) in NOTE_FAMILIES.iter().enumerate() {
        if let Some(rest) = t.strip_prefix(marker) {
            let mut end = 0;
            let chars: Vec<char> = rest.chars().collect();
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '-') {
                end += 1;
            }
            if end == 0 {
                return None;
            }
            let name: String = chars[..end].iter().collect();
            let after: String = chars[end..].iter().collect();
            let content = after
                .strip_prefix(':')
                .unwrap_or(&after)
                .strip_prefix(' ')?;
            if content.is_empty() {
                return None;
            }
            return Some((fi, name, content.to_string()));
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ListKind {
    Unordered,
    Ordered,
    Definition,
}

fn list_kind(t: &str) -> Option<ListKind> {
    if t.starts_with("- ") {
        return Some(ListKind::Unordered);
    }
    if t.starts_with(". ") {
        return Some(ListKind::Ordered);
    }
    if let Some((n, _)) = t.split_once(". ") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return Some(ListKind::Ordered);
        }
    }
    if t.starts_with(": ") && (t.contains(" :: ") || t.ends_with(" ::")) {
        return Some(ListKind::Definition);
    }
    None
}

fn is_list_item(t: &str) -> bool {
    list_kind(t).is_some()
}

fn is_list_head_sugar(t: &str) -> bool {
    if t.starts_with("@.. ") {
        return true;
    }
    for head in ["@::;", "@--", "@.."] {
        if let Some(rest) = t.strip_prefix(head) {
            return !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        }
    }
    false
}

fn is_drama_boundary(t: &str) -> bool {
    t.starts_with("@:") || t == "~:@"
}

fn is_indented(l: &str) -> bool {
    l.starts_with("  ") || l.starts_with('\t')
}

fn dedent(l: &str) -> String {
    l.strip_prefix("  ")
        .or_else(|| l.strip_prefix('\t'))
        .unwrap_or(l)
        .to_string()
}

fn alpha_to_n(s: &str) -> u32 {
    s.chars()
        .fold(0u32, |acc, c| acc * 26 + (c as u32 - 'A' as u32 + 1))
}

fn n_to_alpha(mut n: u32) -> String {
    let mut s = Vec::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.push(char::from(b'A' + r as u8));
        n = (n - 1) / 26;
    }
    s.iter().rev().collect()
}
