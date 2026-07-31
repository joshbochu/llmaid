//! Core Mermaid `timeline` vertical slice: ordered periods, events, and sections.

use crate::parse::{self, ParseError, Warning};
use crate::scene::{EdgeKind, RoutedEdge, Scene, SceneGroup, ScenePath, SceneText};
use crate::temporal::{self, Extent, TemporalBand, TemporalEntry, TemporalGaps};
use crate::wrapping::{self, MIN_READABLE_COLUMNS};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineEvent {
    pub label: String,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelinePeriod {
    pub label: String,
    pub events: Vec<TimelineEvent>,
    pub section: Option<usize>,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineSection {
    pub label: String,
    pub first_period: usize,
    pub period_count: usize,
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct Timeline {
    pub title: Option<String>,
    pub sections: Vec<TimelineSection>,
    pub periods: Vec<TimelinePeriod>,
    pub warnings: Vec<Warning>,
}

impl Timeline {
    pub fn is_empty(&self) -> bool {
        self.periods.is_empty()
    }

    pub fn event_count(&self) -> usize {
        self.periods.iter().map(|period| period.events.len()).sum()
    }

    pub fn labels(&self) -> Vec<&str> {
        self.title
            .iter()
            .map(String::as_str)
            .chain(self.sections.iter().map(|section| section.label.as_str()))
            .chain(self.periods.iter().flat_map(|period| {
                std::iter::once(period.label.as_str())
                    .chain(period.events.iter().map(|event| event.label.as_str()))
            }))
            .collect()
    }
}

pub fn parse(src: &str) -> Result<Timeline, ParseError> {
    crate::parse::validate_terminal_source(src)?;
    let mut timeline = Timeline::default();
    let mut seen_header = false;
    let mut last_period: Option<usize> = None;
    let mut current_section: Option<usize> = None;
    let mut saw_content = false;

    for (line_index, raw) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !seen_header {
            if line == "timeline" {
                seen_header = true;
                continue;
            }
            if line.starts_with("timeline ") {
                return Err(error(
                    line_number,
                    "timeline direction is deferred in the core slice; use the exact `timeline` header",
                ));
            }
            return Err(error(line_number, "expected `timeline` header"));
        }
        if line == "timeline" || line.starts_with("timeline ") {
            return Err(error(line_number, "duplicate `timeline` header"));
        }

        if line == "title" || line.starts_with("title:") {
            return Err(error(
                line_number,
                "expected `title <non-empty text>` without `:`",
            ));
        }
        if let Some(raw_title) = line.strip_prefix("title ") {
            if saw_content {
                return Err(error(
                    line_number,
                    "place the optional title before periods and sections",
                ));
            }
            if timeline.title.is_some() {
                return Err(error(
                    line_number,
                    "timeline title may be declared only once",
                ));
            }
            let title = label(raw_title, line_number, "non-empty timeline title")?;
            timeline.title = Some(title);
            continue;
        }

        if line == "section" || line.starts_with("section:") {
            return Err(error(
                line_number,
                "expected a non-empty section name after `section`",
            ));
        }
        if let Some(raw_section) = line.strip_prefix("section ") {
            if let Some(section) = current_section
                && timeline.sections[section].period_count == 0
            {
                let empty = &timeline.sections[section];
                return Err(error(
                    empty.line,
                    format!(
                        "section `{}` has no period; add one or remove the section",
                        empty.label
                    ),
                ));
            }
            let name = label(raw_section, line_number, "non-empty section name")?;
            let index = timeline.sections.len();
            timeline.sections.push(TimelineSection {
                label: name,
                first_period: timeline.periods.len(),
                period_count: 0,
                line: line_number,
            });
            current_section = Some(index);
            last_period = None;
            saw_content = true;
            continue;
        }

        let Some(colon) = line.find(':') else {
            return Err(error(
                line_number,
                "expected `period : event` timeline syntax",
            ));
        };
        let period_text = line[..colon].trim();
        let event_text = &line[colon + 1..];
        let raw_events: Vec<&str> = event_text.split(':').collect();
        if raw_events.iter().any(|event| event.trim().is_empty()) {
            return Err(error(
                line_number,
                "expected a non-empty event after every `:`",
            ));
        }
        let events: Vec<TimelineEvent> = raw_events
            .iter()
            .map(|raw| {
                label(raw, line_number, "non-empty event").map(|label| TimelineEvent {
                    label,
                    line: line_number,
                })
            })
            .collect::<Result<_, _>>()?;
        if period_text.is_empty() {
            let Some(period) = last_period else {
                return Err(error(
                    line_number,
                    "expected a period before event continuation",
                ));
            };
            timeline.periods[period].events.extend(events);
            continue;
        }

        let period_label = label(period_text, line_number, "non-empty period")?;
        let period = timeline.periods.len();
        timeline.periods.push(TimelinePeriod {
            label: period_label,
            events,
            section: current_section,
            line: line_number,
        });
        if let Some(section) = current_section {
            timeline.sections[section].period_count += 1;
        }
        last_period = Some(period);
        saw_content = true;
    }

    if !seen_header {
        return Err(error(1, "expected `timeline` header"));
    }
    if let Some(section) = current_section
        && timeline.sections[section].period_count == 0
    {
        let empty = &timeline.sections[section];
        return Err(error(
            empty.line,
            format!(
                "section `{}` has no period; add one or remove the section",
                empty.label
            ),
        ));
    }
    Ok(timeline)
}

fn label(raw: &str, line: usize, expected: &str) -> Result<String, ParseError> {
    let label = parse::clean_terminal_label(raw, line)?;
    if label.is_empty() {
        return Err(error(line, format!("expected a {expected}")));
    }
    Ok(label)
}

fn error(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

pub fn dump(timeline: &Timeline) -> String {
    let mut output = String::from("timeline\n");
    if let Some(title) = &timeline.title {
        output.push_str(&format!("title=\"{}\"\n", escape(title)));
    } else {
        output.push_str("title=-\n");
    }
    for (index, section) in timeline.sections.iter().enumerate() {
        output.push_str(&format!(
            "section {index} first={} periods={} line={} label=\"{}\"\n",
            section.first_period,
            section.period_count,
            section.line,
            escape(&section.label)
        ));
    }
    for (index, period) in timeline.periods.iter().enumerate() {
        let section = period
            .section
            .map_or_else(|| "-".into(), |section| section.to_string());
        output.push_str(&format!(
            "period {index} section={section} line={} label=\"{}\"\n",
            period.line,
            escape(&period.label)
        ));
        for (event, value) in period.events.iter().enumerate() {
            output.push_str(&format!(
                "  event {event} line={} label=\"{}\"\n",
                value.line,
                escape(&value.label)
            ));
        }
    }
    output
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub fn scene(timeline: &Timeline, max_width: usize) -> Scene {
    if timeline.periods.is_empty() {
        return Scene::default();
    }
    let plain_periods: Vec<Vec<String>> = timeline
        .periods
        .iter()
        .map(|period| forced_lines(&period.label))
        .collect();
    let plain_events: Vec<Vec<Vec<String>>> = timeline
        .periods
        .iter()
        .map(|period| {
            period
                .events
                .iter()
                .map(|event| forced_lines(&event.label))
                .collect()
        })
        .collect();
    let normal = lower(
        timeline,
        &plain_periods,
        &plain_events,
        TemporalGaps::normal(),
    );
    if fits(&normal, max_width) {
        return normal;
    }
    let compact = lower(
        timeline,
        &plain_periods,
        &plain_events,
        TemporalGaps::compact(),
    );
    if fits(&compact, max_width) {
        return compact;
    }

    let (period_cap, event_cap) = wrap_caps(timeline, max_width);
    let periods: Vec<Vec<String>> = timeline
        .periods
        .iter()
        .map(|period| wrapping::wrap_words(&period.label, period_cap))
        .collect();
    let events: Vec<Vec<Vec<String>>> = timeline
        .periods
        .iter()
        .map(|period| {
            period
                .events
                .iter()
                .map(|event| wrapping::wrap_words(&event.label, event_cap))
                .collect()
        })
        .collect();
    let wrapped = lower(timeline, &periods, &events, TemporalGaps::compact());
    if wrapped.bounds().w < compact.bounds().w {
        wrapped
    } else {
        compact
    }
}

fn fits(scene: &Scene, max_width: usize) -> bool {
    scene.bounds().w.max(0) as usize <= max_width
}

fn lower(
    timeline: &Timeline,
    period_lines: &[Vec<String>],
    event_lines: &[Vec<Vec<String>>],
    gaps: TemporalGaps,
) -> Scene {
    let input: Vec<TemporalEntry> = timeline
        .periods
        .iter()
        .enumerate()
        .map(|(period, value)| TemporalEntry {
            leading: extent(&period_lines[period]),
            trailing: event_lines[period]
                .iter()
                .map(|lines| extent(lines))
                .collect(),
            band: value.section,
        })
        .collect();
    let bands: Vec<TemporalBand> = timeline
        .sections
        .iter()
        .map(|section| TemporalBand {
            first_entry: section.first_period,
            entry_count: section.period_count,
            title_width: section.label.width() as i32,
        })
        .collect();
    let placed = temporal::layout(&input, &bands, gaps);

    let mut texts = Vec::new();
    for (period, rect) in placed.leading_boxes.iter().copied().enumerate() {
        push_leading_text(&mut texts, rect, &period_lines[period]);
        for (event, trailing) in placed.trailing_boxes[period].iter().copied().enumerate() {
            push_trailing_text(&mut texts, trailing, &event_lines[period][event]);
        }
    }
    let edges = placed
        .connectors
        .iter()
        .enumerate()
        .map(|(edge, connector)| RoutedEdge {
            edge,
            points: connector.points.clone(),
            rounded: Vec::new(),
            kind: EdgeKind::Solid,
            label: None,
            arrow: None,
        })
        .collect();
    let paths = placed
        .spine_segments
        .iter()
        .enumerate()
        .map(|(path, points)| ScenePath {
            path,
            points: points.clone(),
            rounded: Vec::new(),
            kind: EdgeKind::Solid,
        })
        .collect();
    let groups = timeline
        .sections
        .iter()
        .enumerate()
        .map(|(section, value)| SceneGroup {
            subgraph: section,
            rect: placed.band_rects[section],
            title: SceneText::new(placed.band_title_points[section], value.label.clone()),
            separators: Vec::new(),
        })
        .collect();
    let mut scene = Scene {
        groups,
        paths,
        edges,
        texts,
        ..Scene::default()
    };
    if let Some(title) = &timeline.title {
        let bounds = scene.bounds();
        let title_width = title.width() as i32;
        scene.texts.push(SceneText::new(
            crate::scene::Point::new(placed.spine_x - title_width / 2, bounds.y - 2),
            title.clone(),
        ));
    }
    scene
}

fn extent(lines: &[String]) -> Extent {
    // The logical slot reserves two cells on the connector side: one painted
    // attachment cell and one visible blank cell before the text.
    let width = lines
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(1)
        .max(1) as i32
        + 2;
    let mut height = lines.len().max(1) as i32;
    if height % 2 == 0 {
        height += 1;
    }
    Extent { width, height }
}

fn push_leading_text(output: &mut Vec<SceneText>, rect: crate::scene::Rect, lines: &[String]) {
    let y = rect.y + (rect.h - lines.len() as i32) / 2;
    for (row, line) in lines.iter().enumerate() {
        output.push(SceneText::new(
            crate::scene::Point::new(rect.right() - 2 - line.width() as i32, y + row as i32),
            line.clone(),
        ));
    }
}

fn push_trailing_text(output: &mut Vec<SceneText>, rect: crate::scene::Rect, lines: &[String]) {
    let y = rect.y + (rect.h - lines.len() as i32) / 2;
    for (row, line) in lines.iter().enumerate() {
        output.push(SceneText::new(
            crate::scene::Point::new(rect.x + 2, y + row as i32),
            line.clone(),
        ));
    }
}

fn forced_lines(label: &str) -> Vec<String> {
    label.lines().map(str::to_string).collect()
}

fn wrap_caps(timeline: &Timeline, max_width: usize) -> (usize, usize) {
    let mut period = timeline
        .periods
        .iter()
        .map(|value| value.label.width())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut event = timeline
        .periods
        .iter()
        .flat_map(|value| &value.events)
        .map(|value| value.label.width())
        .max()
        .unwrap_or(1)
        .max(1);
    let minimum_period = period.min(MIN_READABLE_COLUMNS);
    let minimum_event = event.min(MIN_READABLE_COLUMNS);
    // Two borders plus one visible padding cell on both sides for each box,
    // compact connector gaps, and section frame padding when present.
    let overhead = 4 + 4 + usize::from(!timeline.sections.is_empty()) * 4;
    let budget = max_width.saturating_sub(overhead).max(2);
    while period + event > budget {
        if period >= event && period > minimum_period {
            period -= 1;
        } else if event > minimum_event {
            event -= 1;
        } else if period > minimum_period {
            period -= 1;
        } else {
            break;
        }
    }
    (period, event)
}
