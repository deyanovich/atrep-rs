//! Error types.
//!
//! Every error condition enumerated in the spec's Parsing chapter
//! ("Error Handling") has a variant here, plus implementation-side
//! conditions (I/O).

use std::fmt;
use std::path::PathBuf;

/// Source position of an offending construct (1-indexed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file.display(), self.line, self.col)
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    /// Input is not valid UTF-8.
    InvalidUtf8,
    /// First content line is not a dialektos declaration.
    MissingDeclaration,
    /// No `.lektos` (or `.dia`) file found for the declared identifier.
    UnresolvableDialektos(String),
    /// Invalid dialektos identifier or version in a declaration.
    InvalidDeclaration(String),
    /// Inactive sigil used as if it were the active one.
    MixedSigils,
    /// A run of more than four sigil characters.
    SigilRunTooLong(usize),
    /// No defined symbol matches after the sigil.
    UndefinedSim(String),
    /// A sim was opened but its episim never found.
    UnmatchedSim(String),
    /// An episim with no matching open sim.
    UnmatchedEpisim(String),
    /// An endo-simmere formatted so it mimics a para-simmere.
    EndoMimicsPara,
    /// Monosim parameter contains whitespace.
    MonosimWhitespace(String),
    /// Identifier violates the onym grammar.
    InvalidOnym(String),
    /// Identifier violates the genos grammar.
    InvalidGenos(String),
    /// Axioma reference precedes its definition (or is never defined).
    AxiomaBeforeDefinition(String),
    /// A deixis references an onym that is never declared.
    DeixisUndefinedOnym(String),
    /// A deixis onym is not attached to a para-simmere of its sim.
    DeixisTargetMismatch {
        symbol: String,
        onym: String,
    },
    /// Axioma-enlexis reference targets a standalone onym anchor.
    EnlexisStandaloneOnym(String),
    /// Axioma-enlexis would include a para-simmere in endo-context.
    EnlexisParaInEndoContext(String),
    /// Anaphor form (file transclusion) in endo-context.
    AnaphorInEndoContext,
    /// A transcluded file or media resource is missing/unreadable.
    MissingResource(String),
    /// A document transitively includes itself.
    TransclusionCycle(String),
    /// A simmere component violates its sim definition.
    ComponentViolation(String),
    /// Explicit taxis numbering inconsistent with its sibling run.
    TaxisInconsistent {
        expected: u64,
        found: u64,
    },
    /// Conflicting sim definitions during inheritance/import.
    SimConflict(String),
    /// Malformed `.lektos` / `.dia` definition file.
    InvalidLektos(String),
    /// Malformed `.exo` definition (or slot/declaration misuse).
    InvalidExo(String),
    /// Malformed `.hom`/`.iso` definition or rule misuse.
    InvalidMorph(String),
    /// No morphism file and no derivable embedding for the pair.
    UnresolvableMorph(String),
    /// Document sims with no mapping under the morphism.
    MorphUnmapped(String),
    /// An explicit morphism that is not total over its source
    /// dialektos's sims (spec: "Static Validation").
    MorphIncomplete(String),
    /// Two or more distinct shortest morphism routes.
    AmbiguousMorphRoute(String),
    /// No `.exo` file found for the dialektos/target pair.
    UnresolvableExo(String),
    /// A kanon node with no matching exomorphosis rule.
    ExoUnhandled(String),
    /// Two exomorphosis rules of equal specificity match one node.
    ExoAmbiguous(String),
    /// Any other syntax error.
    Syntax(String),
    Io(std::io::Error),
}

/// An error with optional source location and inclusion chain.
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub location: Option<Location>,
    /// Files traversed to reach the error site, outermost first.
    pub inclusion_chain: Vec<PathBuf>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(kind: ErrorKind) -> Self {
        Error {
            kind,
            location: None,
            inclusion_chain: Vec::new(),
        }
    }

    pub fn at(kind: ErrorKind, location: Location) -> Self {
        Error {
            kind,
            location: Some(location),
            inclusion_chain: Vec::new(),
        }
    }

    /// Record `file` as an inclusion step (outermost last pushed).
    pub fn via(mut self, file: &std::path::Path) -> Self {
        self.inclusion_chain.push(file.to_path_buf());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(loc) = &self.location {
            write!(f, "{loc}: ")?;
        }
        match &self.kind {
            ErrorKind::InvalidUtf8 => write!(f, "input is not valid UTF-8")?,
            ErrorKind::MissingDeclaration => {
                write!(f, "missing dialektos declaration on the first content line")?
            }
            ErrorKind::UnresolvableDialektos(id) => write!(
                f,
                "cannot resolve dialektos `{id}`: no local .lektos/.dia file"
            )?,
            ErrorKind::InvalidDeclaration(s) => write!(f, "invalid dialektos declaration: {s}")?,
            ErrorKind::MixedSigils => write!(f, "mixed sigils in one document")?,
            ErrorKind::SigilRunTooLong(n) => {
                write!(f, "sigil run of {n} characters (maximum is four)")?
            }
            ErrorKind::UndefinedSim(sym) => write!(f, "undefined sim `{sym}`")?,
            ErrorKind::UnmatchedSim(sym) => write!(f, "unmatched sim `{sym}` (episim not found)")?,
            ErrorKind::UnmatchedEpisim(sym) => write!(f, "unmatched episim `{sym}`")?,
            ErrorKind::EndoMimicsPara => {
                write!(f, "endo-simmere formatted to mimic a para-simmere")?
            }
            ErrorKind::MonosimWhitespace(p) => {
                write!(f, "monosim parameter contains whitespace: `{p}`")?
            }
            ErrorKind::InvalidOnym(s) => write!(f, "invalid onym identifier `{s}`")?,
            ErrorKind::InvalidGenos(s) => write!(f, "invalid genos identifier `{s}`")?,
            ErrorKind::AxiomaBeforeDefinition(r) => write!(
                f,
                "axioma reference `{r}` precedes its definition (or is never defined)"
            )?,
            ErrorKind::DeixisUndefinedOnym(r) => {
                write!(f, "deixis reference `{r}` is not a declared onym")?
            }
            ErrorKind::DeixisTargetMismatch { symbol, onym } => write!(
                f,
                "deixis `@{symbol}({onym})` does not point at a `@{symbol}` simmere"
            )?,
            ErrorKind::EnlexisStandaloneOnym(r) => write!(
                f,
                "axioma-enlexis reference `{r}` targets a standalone onym anchor"
            )?,
            ErrorKind::EnlexisParaInEndoContext(r) => write!(
                f,
                "axioma-enlexis reference `{r}` would include a para-simmere in endo-context"
            )?,
            ErrorKind::AnaphorInEndoContext => {
                write!(f, "anaphor (file transclusion) form in endo-context")?
            }
            ErrorKind::MissingResource(p) => write!(f, "missing or unreadable resource: {p}")?,
            ErrorKind::TransclusionCycle(p) => write!(f, "transclusion cycle through {p}")?,
            ErrorKind::ComponentViolation(msg) => write!(f, "{msg}")?,
            ErrorKind::TaxisInconsistent { expected, found } => {
                write!(f, "inconsistent taxis: expected {expected}, found {found}")?
            }
            ErrorKind::SimConflict(sym) => {
                write!(f, "conflicting definitions for sim symbol `{sym}`")?
            }
            ErrorKind::InvalidLektos(msg) => write!(f, "invalid dialektos definition: {msg}")?,
            ErrorKind::InvalidExo(msg) => write!(f, "invalid exomorphosis definition: {msg}")?,
            ErrorKind::InvalidMorph(msg) => write!(f, "invalid morphism definition: {msg}")?,
            ErrorKind::UnresolvableMorph(spec) => write!(
                f,
                "cannot resolve morphism `{spec}`: no .hom/.iso file and no \
                 derivable embedding"
            )?,
            ErrorKind::MorphUnmapped(syms) => {
                write!(f, "sims with no mapping under the morphism: {syms}")?
            }
            ErrorKind::MorphIncomplete(syms) => write!(
                f,
                "morphism is not total over its source dialektos; unaccounted \
                 sims (map, group, or drop explicitly with @--): {syms}"
            )?,
            ErrorKind::AmbiguousMorphRoute(routes) => write!(
                f,
                "ambiguous morphism route; add a direct morphism or morph \
                 explicitly through an intermediate: {routes}"
            )?,
            ErrorKind::UnresolvableExo(spec) => write!(
                f,
                "cannot resolve exomorphosis `{spec}`: no local or standard .exo file"
            )?,
            ErrorKind::ExoUnhandled(p) => {
                write!(f, "no exomorphosis rule matches `{p}` (and no default)")?
            }
            ErrorKind::ExoAmbiguous(p) => write!(
                f,
                "ambiguous exomorphosis rules of equal specificity for `{p}`"
            )?,
            ErrorKind::Syntax(msg) => write!(f, "{msg}")?,
            ErrorKind::Io(e) => write!(f, "I/O error: {e}")?,
        }
        if !self.inclusion_chain.is_empty() {
            let chain: Vec<String> = self
                .inclusion_chain
                .iter()
                .rev()
                .map(|p| p.display().to_string())
                .collect();
            write!(f, " (included via {})", chain.join(" -> "))?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(ErrorKind::Io(e))
    }
}
