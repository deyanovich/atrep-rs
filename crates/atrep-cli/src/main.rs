//! `atrep` — command-line tool of the pilot implementation.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "atrep",
    version,
    about = "Atrep pilot toolchain (spec draft v0.13)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and validate a document or dialektos definition.
    Check {
        /// Path to an .atd/.atk document or .lektos/.dia definition.
        file: PathBuf,
    },
    /// Verbalize: rewrite a document in the plerographic
    /// spelling (sim names in braces; spec: "Metagraphe").
    Plero {
        /// Path to the .atd/.atk document (either spelling).
        file: PathBuf,
        /// Spell sim names in this language's glossa
        /// (<dialektos>.<lang>.glossa), falling back to primary
        /// names where the glossa is silent.
        #[arg(long)]
        lang: Option<String>,
    },
    /// Symbolize: rewrite a document in the brachygraphic
    /// spelling (sim symbols; the canonical spelling).
    Brachy {
        /// Path to the .atd/.atk document (either spelling).
        file: PathBuf,
    },
    /// Emit the document's structural outline as kaiv data
    /// (blocks with line spans, milestones, onyms, deixes) for
    /// structure-aware editor tooling.
    Outline {
        /// Path to the .atd/.atk document (either spelling).
        file: PathBuf,
    },
    /// Canonicalize a deltos (.atd) into a kanon (.atk), or a
    /// dialektos definition (.dia) into its canonical .lektos.
    Kanonizo {
        /// Path to the .atd (or .dia) source.
        file: PathBuf,
        /// Output path (defaults to the input with extension .atk,
        /// or .lektos for a definition).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Remote-fetch timeout, in seconds.
        #[arg(long, default_value_t = 30)]
        timeout: u64,
        /// Remote-fetch retries after a failed attempt.
        #[arg(long, default_value_t = 2)]
        retries: u32,
    },
    /// Export a kanon to an external format via its .exo rules.
    Exo {
        /// Path to the .atd or .atk file (.atd is kanonized first).
        file: PathBuf,
        /// Target format identifier (e.g. md, html).
        target: String,
        /// Output path (defaults to the input with the target as
        /// its extension).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Named exo variant (<dialektos>.<target>.<variant>.exo,
        /// overlaid on the base exomorphosis).
        #[arg(long)]
        variant: Option<String>,
    },
    /// Collate witness kanons against a base witness: weave on a
    /// milestone scheme and derive an apparatus criticus of
    /// manuscript-notes anchored in the base text.
    Collate {
        /// Witness .atk files.
        files: Vec<PathBuf>,
        /// The milestone scheme to align on (e.g. stephanus).
        #[arg(long)]
        scheme: String,
        /// Comma-separated witness identifiers, one per file.
        #[arg(long = "as", value_delimiter = ',')]
        ids: Vec<String>,
        /// The witness whose text carries the apparatus.
        #[arg(long)]
        base: String,
        /// Output path for the .atd with the apparatus.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Weave two or more witness kanons of one dialektos along a
    /// shared milestone scheme into the zygoma (spec v0.12).
    Zygo {
        /// Witness .atk files, primary first.
        files: Vec<PathBuf>,
        /// The milestone scheme to align on (e.g. stephanus).
        #[arg(long)]
        scheme: String,
        /// Comma-separated witness identifiers, one per file.
        #[arg(long = "as", value_delimiter = ',')]
        ids: Vec<String>,
        /// Output path for the woven .atd.
        #[arg(short, long)]
        output: PathBuf,
        /// Subdivide oversized segments: recursively halve any
        /// coordinate whose largest witness slice exceeds this
        /// many characters, cutting every witness at the sentence
        /// boundary nearest its midpoint (continuation slices
        /// carry no milestone).
        #[arg(long)]
        split: Option<usize>,
    },
    /// Insert quasi-milestones at statistical midpoints so no
    /// segment exceeds --max-segment characters in any file: the
    /// in-file counterpart of zygo --split. Cuts prefer, in
    /// order: leaf-block boundaries (speech/poem-block and
    /// strophe heads -- verse cuts land between strophes, never
    /// inside a line), paragraph heads, sentence ends (verse
    /// line boundaries count here), commas, whitespace --
    /// nearest each witness's midpoint, and are named by the
    /// opening coordinate plus a binary path (17a|1, then
    /// 17a|0.1 / 17a|1.1; `^|...` under the implicit
    /// document-start anchor); the files stay milestone-aligned
    /// and re-emit in canonical form, in place.
    Quasialign {
        /// Files sharing a milestone scheme (.atd or .atk),
        /// rewritten in place.
        files: Vec<PathBuf>,
        /// Ceiling on segment size, in text characters.
        #[arg(long)]
        max_segment: usize,
        /// Witness namespace prepended to every inserted cut's
        /// coordinate (e.g. litogram:) unless the parent already
        /// carries it — positional cuts are witness-specific.
        #[arg(long)]
        cut_prefix: Option<String>,
        /// The milestone scheme to segment on; inferred when the
        /// files carry exactly one.
        #[arg(long)]
        scheme: Option<String>,
        /// Witness genos prefix (e.g. zyg-grc): in files carrying
        /// several witnesses as genos-tagged paradiaphanes, drive
        /// the alignment by this witness's stream alone and
        /// insert the cuts inside it.
        #[arg(long)]
        prefix: Option<String>,
        /// Refine: existing quasi cuts anchor further subdivision
        /// at the (smaller) ceiling; new cuts extend their binary
        /// paths (ch:3|0.1 -> ch:3|0.1.1). Without this flag,
        /// already-cut segments are left untouched.
        #[arg(long)]
        refine: bool,
    },
    /// Import an external format as an atrep document: Markdown
    /// into at-markdown, HTML into at-html (by file extension).
    Endo {
        /// Path to the .md or .html file.
        file: PathBuf,
        /// Versification scheme for scripture core milestones
        /// (usfm/usx/osis inputs), e.g. protestant.
        #[arg(long)]
        milestone_scheme: Option<String>,
        /// Inject line-number milestones on TEI verse `<l n>` lines
        /// under this scheme (e.g. grc:book-line or grc:line); one
        /// at line 1 and every fifth line thereafter.
        #[arg(long)]
        line_milestones: Option<String>,
        /// Output path (defaults to the input with extension .atd).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Map a kanon to another dialektos via a .hom/.iso morphism
    /// (or an embedding derived from the dialektos lineage).
    Morph {
        /// Path to the .atd or .atk file (.atd is kanonized first).
        file: PathBuf,
        /// Target dialektos identifier.
        target: String,
        /// Named morphism variant (<a>.<b>.<variant>.hom); direct
        /// only, no route composition.
        #[arg(long)]
        variant: Option<String>,
        /// Output path (defaults to <stem>.<target>.atk).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compose morphisms along a chain of dialektoi into a
    /// single .hom file (each adjacent pair resolves directly
    /// or transitively).
    Compose {
        /// Dialektos identifiers, source to target (2 or more).
        #[arg(num_args = 2.., required = true)]
        chain: Vec<String>,
        /// Output path (defaults to <first>.<last>.hom).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compute the litos ID of a kanon (.atk).
    Litos {
        /// Path to the .atk file (media resolved relative to it).
        file: PathBuf,
        /// Also write the .atk.litos file next to the input.
        #[arg(long)]
        save_litos_file: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("atrep: error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Metagraphe: reparse (either spelling in) and serialize in the
/// requested spelling.
fn respell(file: &std::path::Path, plero: bool, lang: Option<&str>) -> atrep::Result<String> {
    let text = std::fs::read_to_string(file).map_err(|e| {
        atrep::error::Error::new(atrep::error::ErrorKind::MissingResource(format!(
            "{}: {e}",
            file.display()
        )))
    })?;
    let doc = atrep::parser::parse_document(&text, file)?;
    if plero {
        let dir = file.parent().unwrap_or(std::path::Path::new("."));
        let dial = atrep::dialektos::resolve(dir, &doc.dialect_id)?;
        atrep::dendron::serialize_plerographic_in(&doc, &dial, lang)
    } else {
        Ok(atrep::dendron::serialize(&doc))
    }
}

/// The outline as authored kaiv text (namespace-array table
/// headers, almost-verbatim values with `$$` doubling).
fn outline_kaiv(file: &std::path::Path) -> atrep::Result<String> {
    let text = std::fs::read_to_string(file).map_err(|e| {
        atrep::error::Error::new(atrep::error::ErrorKind::MissingResource(format!(
            "{}: {e}",
            file.display()
        )))
    })?;
    let (doc, blocks, dial) = atrep::parser::parse_document_outline(&text, file)?;
    let o = atrep::outline::assemble(&doc, blocks, &text, &dial);
    let esc = |v: &str| {
        if let Some(rest) = v.strip_prefix('$') {
            format!("$${rest}")
        } else {
            v.to_string()
        }
    };
    let mut s = String::from(
        ".!kaiv

",
    );
    s.push_str(&format!(
        "dialektos={}
",
        esc(&o.dialektos)
    ));
    for b in &o.blocks {
        s.push_str(
            "
[/@blocks]
",
        );
        s.push_str(&format!(
            "kind={}
",
            b.kind
        ));
        if let Some(sym) = &b.symbol {
            s.push_str(&format!(
                "symbol={}
",
                esc(sym)
            ));
        }
        if let Some(n) = &b.name {
            s.push_str(&format!(
                "name={}
",
                esc(n)
            ));
        }
        s.push_str(&format!(
            "depth={}
start={}
end={}
",
            b.depth, b.start, b.end
        ));
        if !b.lemma.is_empty() {
            s.push_str(&format!(
                "lemma={}
",
                esc(&b.lemma)
            ));
        }
        if let Some(onym) = &b.onym {
            s.push_str(&format!(
                "onym={}
",
                esc(onym)
            ));
        }
        for g in &b.genoses {
            s.push_str(&format!(
                "@genoses+={}
",
                esc(g)
            ));
        }
    }
    for (ns, points) in [
        ("milestones", &o.milestones),
        ("onyms", &o.onyms),
        ("deixes", &o.deixes),
    ] {
        for p in points.iter() {
            s.push_str(&format!(
                "
[/@{ns}]
key={}
line={}
",
                esc(&p.key),
                p.line
            ));
        }
    }
    Ok(s)
}

fn run() -> atrep::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { file } => {
            atrep::check_any(&file)?;
            println!("{}: OK", file.display());
        }
        Command::Plero { file, lang } => print!("{}", respell(&file, true, lang.as_deref())?),
        Command::Brachy { file } => print!("{}", respell(&file, false, None)?),
        Command::Outline { file } => print!("{}", outline_kaiv(&file)?),
        Command::Kanonizo {
            file,
            output,
            timeout,
            retries,
        } => {
            let source = std::fs::read_to_string(&file)?;
            if atrep::is_definition_source(&source) {
                let result = atrep::kanonizo::kanonizo_definition_file(&file)?;
                let out = output.unwrap_or_else(|| file.with_extension("lektos"));
                std::fs::write(&out, &result.kanon)?;
                println!("{}", out.display());
            } else {
                let opts = atrep::kanonizo::KanonizoOptions {
                    fetch: atrep::fetch::FetchConfig {
                        timeout: std::time::Duration::from_secs(timeout),
                        retries,
                        ..Default::default()
                    },
                };
                let fetcher = atrep::fetch::HttpFetcher::new(&opts.fetch);
                let result = atrep::kanonizo::kanonizo_file_with(&file, &fetcher, &opts)?;
                let out = output.unwrap_or_else(|| file.with_extension("atk"));
                atrep::kanonizo::write_outputs(&result, &out)?;
                println!("{}", out.display());
                if !result.media.is_empty() {
                    println!("{}", out.with_extension("atk.tar.gz").display());
                }
            }
        }
        Command::Exo {
            file,
            target,
            output,
            variant,
        } => {
            let doc = if file.extension().is_some_and(|e| e == "atk") {
                atrep::check_file(&file)?
            } else {
                atrep::kanonizo::kanonizo_file(&file)?.document
            };
            let dir = file
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            // Built-in exomorphoses (fb2, rnc, opencorpora, proiel):
            // formats whose output the template language cannot
            // produce.
            let (rendered, aux) = match atrep::native_export_in(&doc, &target, &dir) {
                Some(rendered) => (rendered?, Vec::new()),
                None => {
                    let exo = atrep::exo::resolve_exo_variant(
                        &dir,
                        &doc.dialect_id,
                        &target,
                        variant.as_deref(),
                    )?;
                    atrep::exo::render_with_aux(&doc, &exo, &dir)?
                }
            };
            let out = output.unwrap_or_else(|| file.with_extension(&target));
            std::fs::write(&out, rendered)?;
            println!("{}", out.display());
            // Associated exos produce companion files (e.g. the
            // .bib beside a LaTeX export).
            for (dialect, aux_target, content) in aux {
                let aux_out = if dialect == "*asset" {
                    // An asset carries its own filename.
                    match out.parent() {
                        Some(dir) => dir.join(&aux_target),
                        None => std::path::PathBuf::from(&aux_target),
                    }
                } else {
                    out.with_extension(&aux_target)
                };
                std::fs::write(&aux_out, content)?;
                println!("{}", aux_out.display());
            }
        }
        Command::Collate {
            files,
            scheme,
            ids,
            base,
            output,
        } => {
            if files.len() != ids.len() {
                return Err(atrep::error::Error::new(atrep::error::ErrorKind::Syntax(
                    format!(
                        "{} files but {} ids (--as takes one id per file)",
                        files.len(),
                        ids.len()
                    ),
                )));
            }
            let mut witnesses = Vec::new();
            for (file, id) in files.iter().zip(&ids) {
                let doc = atrep::check_file(file)?;
                witnesses.push((id.clone(), doc));
            }
            let collated = atrep::zygosis::collation(&witnesses, &scheme, &base)?;
            std::fs::write(&output, atrep::dendron::serialize(&collated))?;
            println!("{}", output.display());
        }
        Command::Zygo {
            files,
            scheme,
            ids,
            output,
            split,
        } => {
            if files.len() != ids.len() {
                return Err(atrep::error::Error::new(atrep::error::ErrorKind::Syntax(
                    format!(
                        "{} files but {} ids (--as takes one id per file)",
                        files.len(),
                        ids.len()
                    ),
                )));
            }
            let mut witnesses = Vec::new();
            for (file, id) in files.iter().zip(&ids) {
                let doc = atrep::check_file(file)?;
                witnesses.push((id.clone(), doc));
            }
            let zygoma = atrep::zygosis::zygosis_split(&witnesses, &scheme, split)?;
            std::fs::write(&output, atrep::dendron::serialize(&zygoma))?;
            println!("{}", output.display());
        }
        Command::Quasialign {
            refine,
            files,
            max_segment,
            scheme,
            prefix,
            cut_prefix,
        } => {
            let mut docs = Vec::new();
            for file in &files {
                docs.push(atrep::check_file(file)?);
            }
            let scheme = match scheme {
                Some(s) => s,
                None => {
                    let mut schemes = std::collections::BTreeSet::new();
                    for doc in &docs {
                        collect_schemes(&doc.blocks, &mut schemes);
                    }
                    match schemes.len() {
                        1 => schemes.into_iter().next().unwrap(),
                        0 => {
                            return Err(atrep::error::Error::new(atrep::error::ErrorKind::Syntax(
                                "no milestones found; nothing to quasialign".into(),
                            )));
                        }
                        _ => {
                            return Err(atrep::error::Error::new(atrep::error::ErrorKind::Syntax(
                                format!(
                                    "several milestone schemes present ({}); pick one with --scheme",
                                    schemes.into_iter().collect::<Vec<_>>().join(", ")
                                ),
                            )));
                        }
                    }
                }
            };
            let report = atrep::zygosis::quasialign(
                &mut docs,
                &scheme,
                max_segment,
                prefix.as_deref(),
                cut_prefix.as_deref(),
                refine,
            )?;
            for ((file, doc), n) in files.iter().zip(&docs).zip(&report.inserted) {
                std::fs::write(file, atrep::dendron::serialize(doc))?;
                println!("{}: {n} quasi-milestone(s) inserted", file.display());
            }
            for coord in &report.skipped {
                eprintln!("atrep: `{coord}` already carries quasi cuts; left untouched");
            }
        }
        Command::Endo {
            file,
            milestone_scheme,
            line_milestones,
            output,
        } => {
            let source = std::fs::read_to_string(&file)?;
            let ext = file
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            if ext == "atr" {
                // Atramento is text-to-text: the litogramma deltos
                // is written directly, preserving the passthrough
                // property byte for byte.
                let lit = atrep::atramento::atramento_to_litogramma(&source)?;
                let out = output.unwrap_or_else(|| file.with_extension("atd"));
                std::fs::write(&out, lit)?;
                println!("{}", out.display());
                return Ok(());
            }
            let out = output.clone().unwrap_or_else(|| file.with_extension("atd"));
            // FictionBook binaries land beside the document as
            // media/<id>, where its image blocks point.
            let mut media: Vec<atrep::fb2::Fb2Media> = Vec::new();
            let mut fb2_import = |source: &str| -> atrep::Result<atrep::Document> {
                let (doc, m) = atrep::fb2::fb2_to_document_with_media(source)?;
                media = m;
                Ok(doc)
            };
            let doc = match ext.as_str() {
                "html" | "htm" => atrep::endo::html_to_document(&source)?,
                "rst" => atrep::endo::rst_to_document(&source)?,
                "org" => atrep::endo::org_to_document(&source)?,
                "dj" | "djot" => atrep::endo::djot_to_document(&source)?,
                "dbk" | "docbook" => atrep::endo::docbook_to_document(&source)?,
                "bib" => atrep::endo::bibtex_to_document(&source)?,
                "jats" => atrep::endo::jats_to_document(&source)?,
                "usfm" | "sfm" => atrep::endo::usfm_to_document(&source)?,
                "tanzil" => atrep::endo::tanzil_to_document(
                    &source,
                    milestone_scheme.as_deref().unwrap_or("kufan"),
                )?,
                "usx" => atrep::endo::usx_to_document(&source)?,
                "osis" => atrep::endo::osis_to_document(&source)?,
                "fb2" => fb2_import(&source)?,
                "rnc" => atrep::epimerismos::rnc_to_document(&source)?,
                "opencorpora" | "oc" => atrep::epimerismos::opencorpora_to_document(&source)?,
                "proiel" => atrep::epimerismos::proiel_to_document(&source)?,
                "conllu" | "conll" => atrep::epimerismos::conllu_to_document(&source)?,
                "xml" | "tei" => match xml_root(&source) {
                    "FictionBook" => fb2_import(&source)?,
                    "proiel" => atrep::epimerismos::proiel_to_document(&source)?,
                    "annotation" => atrep::epimerismos::opencorpora_to_document(&source)?,
                    "html" if source.contains("<ana ") || source.contains("<se>") => {
                        atrep::epimerismos::rnc_to_document(&source)?
                    }
                    _ => atrep::endo::tei_to_document_lines(&source, line_milestones.as_deref())?,
                },
                _ => atrep::endo::markdown_to_document(&source)?,
            };
            let mut doc = doc;
            if let Some(scheme) = &milestone_scheme {
                atrep::endo::usfm_apply_scheme(&mut doc, scheme);
            }
            std::fs::write(&out, atrep::dendron::serialize(&doc))?;
            println!("{}", out.display());
            if !media.is_empty() {
                let dir = out
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("media");
                std::fs::create_dir_all(&dir)?;
                for m in &media {
                    let path = dir.join(&m.id);
                    std::fs::write(&path, &m.bytes)?;
                    println!("{}", path.display());
                }
            }
        }
        Command::Morph {
            file,
            target,
            variant,
            output,
        } => {
            let doc = if file.extension().is_some_and(|e| e == "atk") {
                atrep::check_file(&file)?
            } else {
                atrep::kanonizo::kanonizo_file(&file)?.document
            };
            let dir = file
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let route = match &variant {
                Some(v) => vec![atrep::morph::resolve_morph_variant(
                    &dir,
                    &doc.dialect_id,
                    &target,
                    Some(v),
                )?],
                None => atrep::morph::resolve_route(&dir, &doc.dialect_id, &target)?,
            };
            if route.len() > 1 {
                let hops: Vec<&str> = std::iter::once(route[0].source.as_str())
                    .chain(route.iter().map(|m| m.target.as_str()))
                    .collect();
                eprintln!("atrep: fusing {}", hops.join(" => "));
            }
            let out_doc = atrep::morph::apply_route(&doc, &route)?;
            let out = output.unwrap_or_else(|| file.with_extension(format!("{target}.atk")));
            std::fs::write(&out, atrep::dendron::serialize(&out_doc))?;
            println!("{}", out.display());
        }
        Command::Compose { chain, output } => {
            let dir = std::path::PathBuf::from(".");
            let mut route: Vec<atrep::morph::Morph> = Vec::new();
            for pair in chain.windows(2) {
                route.extend(atrep::morph::resolve_route(&dir, &pair[0], &pair[1])?);
            }
            let mut fused = route[0].clone();
            for next in &route[1..] {
                fused = atrep::morph::compose(&fused, next)?;
            }
            let hops: Vec<&str> = std::iter::once(route[0].source.as_str())
                .chain(route.iter().map(|m| m.target.as_str()))
                .collect();
            eprintln!("atrep: composed {}", hops.join(" => "));
            let out = output
                .unwrap_or_else(|| PathBuf::from(format!("{}.{}.hom", fused.source, fused.target)));
            std::fs::write(&out, atrep::morph::serialize_hom(&fused))?;
            println!("{}", out.display());
        }
        Command::Litos {
            file,
            save_litos_file,
        } => {
            let doc = atrep::check_file(&file)?;
            let dir = file
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let lookup = atrep::litosis::media_from_dir(&dir);
            let resolve = |id: &str| atrep::dialektos::resolve(&dir, id).ok();
            let result = atrep::litosis::litosis_with(&doc, &lookup, &resolve)?;
            if save_litos_file {
                let litos_path = PathBuf::from(format!("{}.litos", file.display()));
                std::fs::write(&litos_path, &result.litos)?;
            }
            println!("{}", result.litos_id);
        }
    }
    Ok(())
}

/// Collect every milestone scheme in the blocks, for
/// quasialign's single-scheme inference.
fn collect_schemes(blocks: &[atrep::dendron::Block], out: &mut std::collections::BTreeSet<String>) {
    use atrep::dendron::{Block, Inline};
    fn inlines(v: &[Inline], out: &mut std::collections::BTreeSet<String>) {
        for inline in v {
            match inline {
                Inline::Milestone { scheme, .. } => {
                    out.insert(scheme.clone());
                }
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    inlines(content, out)
                }
                _ => {}
            }
        }
    }
    for block in blocks {
        match block {
            Block::Paragraph(v) => inlines(v, out),
            Block::Para { children, .. } | Block::ParaDiaphane { children, .. } => {
                collect_schemes(children, out)
            }
            Block::Stichoi { strophes, .. } => {
                for strophe in strophes {
                    for line in &strophe.0 {
                        inlines(line, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The root element's name of an XML source: the first tag that is
/// not a declaration, processing instruction or comment.
fn xml_root(source: &str) -> &str {
    let mut rest = source;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => rest = &rest[end + 3..],
                None => return "",
            }
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            match rest.find('>') {
                Some(end) => rest = &rest[end + 1..],
                None => return "",
            }
            continue;
        }
        let name_end = rest[1..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        return &rest[1..name_end];
    }
    ""
}
