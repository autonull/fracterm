//! Projection - a selected presentation of part of a surface.

use super::*;

/// Row range for projections
#[derive(Debug, Clone)]
pub struct RowRange {
    pub start: i32,
    pub end: i32,
}

/// Column range for projections
#[derive(Debug, Clone)]
pub struct ColumnRange {
    pub start: i32,
    pub end: i32,
}

/// Filter specification for projections
#[derive(Debug, Clone)]
pub enum FilterSpec {
    /// Substring filter
    Substring {
        pattern: String,
        case_sensitive: bool,
    },
    /// Regex filter
    Regex {
        pattern: String,
        case_sensitive: bool,
    },
    /// Log level filter
    Levels {
        levels: Vec<String>,
    },
}

/// Filter mode: extract or highlight
#[derive(Debug, Clone)]
pub enum FilterMode {
    /// Extract matching lines
    Extract,
    /// Highlight matching lines in place
    Highlight,
}

/// Projection selector for filtering and range selection
#[derive(Debug, Clone)]
pub struct ProjectionSelector {
    /// Row range
    pub rows: Option<RowRange>,
    /// Column range
    pub columns: Option<ColumnRange>,
    /// Filter specification
    pub filter: Option<FilterSpec>,
    /// Search string
    pub search: Option<String>,
    /// Maximum number of lines
    pub max_lines: Option<usize>,
    /// Follow the source
    pub follow: bool,
}

impl ProjectionSelector {
    /// Create a new selector
    pub fn new() -> Self {
        Self {
            rows: None,
            columns: None,
            filter: None,
            search: None,
            max_lines: None,
            follow: true,
        }
    }

    /// Set the row range
    pub fn with_rows(mut self, start: i32, end: i32) -> Self {
        self.rows = Some(RowRange { start, end });
        self
    }

    /// Set the column range
    pub fn with_columns(mut self, start: i32, end: i32) -> Self {
        self.columns = Some(ColumnRange { start, end });
        self
    }

    /// Set a substring filter
    pub fn with_substring(mut self, pattern: &str, case_sensitive: bool) -> Self {
        self.filter = Some(FilterSpec::Substring {
            pattern: pattern.to_string(),
            case_sensitive,
        });
        self
    }

    /// Set a regex filter
    pub fn with_regex(mut self, pattern: &str, case_sensitive: bool) -> Self {
        self.filter = Some(FilterSpec::Regex {
            pattern: pattern.to_string(),
            case_sensitive,
        });
        self
    }

    /// Set log level filter
    pub fn with_levels(mut self, levels: Vec<String>) -> Self {
        self.filter = Some(FilterSpec::Levels { levels });
        self
    }

    /// Set the search string
    pub fn with_search(mut self, search: &str) -> Self {
        self.search = Some(search.to_string());
        self
    }

    /// Set the maximum number of lines
    pub fn with_max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    /// Set whether to follow the source
    pub fn with_follow(mut self, follow: bool) -> Self {
        self.follow = follow;
        self
    }
}

/// Projection presentation options
#[derive(Debug, Clone)]
pub struct ProjectionPresentation {
    /// Wrap lines
    pub wrap: bool,
    /// Reflow text
    pub reflow: bool,
    /// Show line numbers
    pub show_line_numbers: bool,
    /// Show source timestamp
    pub show_source_timestamp: bool,
    /// Highlight matches
    pub highlight_matches: bool,
    /// Font size scale
    pub font_size_scale: f64,
    /// Theme override
    pub theme_override: Option<Theme>,
}

impl ProjectionPresentation {
    /// Create a default presentation
    pub fn default() -> Self {
        Self {
            wrap: true,
            reflow: false,
            show_line_numbers: false,
            show_source_timestamp: false,
            highlight_matches: true,
            font_size_scale: 1.0,
            theme_override: None,
        }
    }

    /// Create a reading presentation
    pub fn reading() -> Self {
        Self {
            wrap: true,
            reflow: true,
            show_line_numbers: false,
            show_source_timestamp: false,
            highlight_matches: false,
            font_size_scale: 1.8,
            theme_override: Some(Theme::high_contrast()),
        }
    }
}

/// A projection surface is a node whose surface is a projection.
#[derive(Debug, Clone)]
pub struct ProjectionSurface {
    /// Source surface ID
    pub source: SurfaceId,
    /// Selector for the projection
    pub selector: ProjectionSelector,
    /// Live or snapshot mode
    pub mode: ProjectionMode,
    /// Presentation options
    pub presentation: ProjectionPresentation,
    /// Cached content
    pub content: Vec<String>,
    /// Whether the projection is dirty
    pub dirty: bool,
}

/// Projection mode: live or snapshot
#[derive(Debug, Clone)]
pub enum ProjectionMode {
    /// Live projection that updates with the source
    Live,
    /// Frozen snapshot of the source
    Snapshot,
}

impl ProjectionSurface {
    /// Create a new projection surface
    pub fn new(source: SurfaceId, selector: ProjectionSelector, mode: ProjectionMode) -> Self {
        Self {
            source,
            selector,
            mode,
            presentation: ProjectionPresentation::default(),
            content: vec![],
            dirty: true,
        }
    }

    /// Get the source surface ID
    pub fn source(&self) -> SurfaceId {
        self.source
    }

    /// Update the projection from source content
    pub fn update(&mut self, source_content: &[String]) {
        match self.mode {
            ProjectionMode::Live => {
                self.content = self.apply_selector(source_content);
                self.dirty = false;
            }
            ProjectionMode::Snapshot => {
                if self.dirty {
                    self.content = self.apply_selector(source_content);
                    self.dirty = false;
                }
            }
        }
    }

    /// Apply the selector to source content
    fn apply_selector(&self, source_content: &[String]) -> Vec<String> {
        let mut result: Vec<String> = source_content.to_vec();

        // Apply row range
        if let Some(rows) = &self.selector.rows {
            let start = rows.start.max(0) as usize;
            let end = rows.end.max(rows.start) as usize;
            if end < result.len() {
                result = result[start..=end].to_vec();
            } else if start < result.len() {
                result = result[start..].to_vec();
            } else {
                result.clear();
            }
        }

        // Apply column range
        if let Some(columns) = &self.selector.columns {
            let start = columns.start.max(0) as usize;
            let end = columns.end.max(columns.start) as usize;
            result = result
                .into_iter()
                .map(|line| {
                    let chars: Vec<char> = line.chars().collect();
                    if start < chars.len() {
                        chars[start..end.min(chars.len())].iter().collect()
                    } else {
                        String::new()
                    }
                })
                .collect();
        }

        // Apply filter
        if let Some(filter) = &self.selector.filter {
            result = self.apply_filter(&result, filter);
        }

        // Apply search
        if let Some(search) = &self.selector.search {
            result = result
                .into_iter()
                .filter(|line| line.contains(search))
                .collect();
        }

        // Apply max lines
        if let Some(max_lines) = self.selector.max_lines {
            if result.len() > max_lines {
                result = result[result.len() - max_lines..].to_vec();
            }
        }

        result
    }

    /// Apply the filter to content
    fn apply_filter(&self, content: &[String], filter: &FilterSpec) -> Vec<String> {
        match filter {
            FilterSpec::Substring {
                pattern,
                case_sensitive,
            } => content
                .iter()
                .filter(|line| {
                    if *case_sensitive {
                        line.contains(pattern)
                    } else {
                        line.to_lowercase().contains(&pattern.to_lowercase())
                    }
                })
                .cloned()
                .collect(),
            FilterSpec::Regex { .. } => {
                // Regex support would be added here
                content.to_vec()
            }
            FilterSpec::Levels { levels } => content
                .iter()
                .filter(|line| {
                    levels.iter().any(|level| {
                        line.contains(&format!("[{}]", level))
                            || line.contains(&format!(" {}", level))
                    })
                })
                .cloned()
                .collect(),
        }
    }

    /// Get the projected content
    pub fn content(&self) -> &[String] {
        &self.content
    }

    /// Mark the projection as dirty
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Set the mode
    pub fn set_mode(&mut self, mode: ProjectionMode) {
        self.mode = mode;
        self.dirty = true;
    }

    /// Toggle follow mode
    pub fn set_follow(&mut self, follow: bool) {
        self.selector.follow = follow;
    }

    /// Get the presentation
    pub fn presentation(&self) -> &ProjectionPresentation {
        &self.presentation
    }

    /// Set the presentation
    pub fn set_presentation(&mut self, presentation: ProjectionPresentation) {
        self.presentation = presentation;
    }
}